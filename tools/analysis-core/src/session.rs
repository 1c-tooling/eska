//! One resident metadata session connects the shared input controller to tracked kernel slots.

mod analysis_inputs;
mod completion_inputs;
pub mod composition;
#[cfg(test)]
mod configuration_identity_tests;
#[cfg(test)]
mod event_subscription_tests;
mod external;
#[cfg(test)]
mod external_tests;
mod forms;
#[cfg(test)]
mod http_service_tests;
#[cfg(test)]
mod integration_completion_tests;
#[cfg(test)]
mod integration_service_tests;
pub mod metadata_resources;
#[cfg(test)]
mod module_tests;
pub mod query_text;
mod root;
mod saved_changes;
mod scope;
pub use scope::AnalysisScope;
#[cfg(test)]
mod common_attribute_tests;
mod common_attributes;
#[cfg(test)]
mod common_command_tests;
mod common_commands;
#[cfg(test)]
mod local_syntax_tests;
#[cfg(test)]
mod scheduled_job_tests;
#[cfg(test)]
mod session_parameter_tests;
#[cfg(test)]
mod structural_descriptor_tests;
#[cfg(test)]
mod subsystem_tests;
#[cfg(test)]
pub mod tests;
#[cfg(test)]
mod web_service_tests;

use std::{collections::BTreeMap, path::Path, sync::Arc};

use bsl_metadata::Configuration;
use eska::{
    analysis_core::{InputRequest, InputRevision, InputSummary, MetadataInputs, UpdateError},
    project_core::source_format::{Error, SourcePath, SourceReader},
};
use ide_db::{
    GraphConfigCache, RootDatabaseImpl,
    metadata::{
        CanonicalMetadata, CanonicalMetadataInput, resolve_common_module, resolve_defined_type,
        resolve_event_subscription, resolve_http_service, resolve_integration_service,
        resolve_metadata_object, resolve_register, resolve_role, resolve_scheduled_job,
        resolve_subsystem, resolve_web_service,
    },
};
use salsa::Setter;

use crate::{
    ModelState, OwnerModel, SessionOptions, SessionRequestError, SessionRevision,
    binding::OwnerBinding,
};

/// Opening failures remain structured; a transport or CLI decides their presentation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionError {
    Source(Error),
    UnsupportedProjectType(eska::project_core::ProjectType),
    NonUtf8Root,
    TooManyFiles,
    MissingListing,
}

/// Input readiness, main-descriptor projection and full metadata coverage are independent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionSummary {
    pub revision: SessionRevision,
    pub inputs: InputSummary,
    pub ready_models: usize,
    pub structure_only: usize,
    pub source_only: usize,
    pub pending: usize,
    pub unsupported: usize,
    pub invalid: usize,
    pub unavailable: usize,
    pub descriptors: usize,
    pub modeled_descriptors: usize,
    pub main_models_complete: bool,
    /// Coverage of parsed descriptors only; resources and BSL are outside this count.
    pub descriptor_models_complete: bool,
    /// Own models do not supply unknown globals, common modules or foreign metadata of an owning base.
    pub metadata_context_complete: bool,
}

/// A binding and its current model always share the same tracked slot.
struct OwnerEntry {
    binding: OwnerBinding,
    slot: CanonicalMetadataInput,
    model: OwnerModel,
    source_bytes: u64,
    primary_revision: Option<ProjectionRevision>,
    forms: BTreeMap<SourcePath, forms::FormEntry>,
}

/// A resource edit must not recreate models whose current YAML and resource inputs did not change.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ProjectionRevision {
    owner: InputRevision,
    resource: Option<SessionRevision>,
}

/// The composition is immutable until reopening; updates publish only changed canonical dependencies.
pub struct AnalysisSession {
    scope: AnalysisScope,
    pub(super) selected_root: root::RootInputs,
    pub(super) other_roots: BTreeMap<crate::composition::RootId, root::RootInputs>,
    pub(super) composition_sha256: Option<String>,
    pub(super) database: RootDatabaseImpl,
    pub(super) revision: SessionRevision,
    /// Navigation converts kernel `FileId` values to physical authoring paths without path guesses.
    pub(super) loaded_module_bytes: u64,
    pub(super) loaded_metadata_source_bytes: u64,
    pub(super) loaded_resource_bytes: u64,
    pub(super) loaded_query_bytes: u64,
    pub(super) options: SessionOptions,
}

impl AnalysisSession {
    /// Register pending slots from the authoritative composition, reading no unrelated owner.
    ///
    /// # Errors
    /// Returns source opening failures, a non-UTF-8 kernel root or an exhausted file-ID range.
    pub fn open(root: &Path) -> Result<Self, SessionError> {
        Self::open_with_options(root, SessionOptions::default())
    }

    /// Apply an explicit current-module-text cap without changing the format's hard read bound.
    ///
    /// # Errors
    /// Returns the same composition, root and file-identity failures as `open`.
    pub fn open_with_options(root: &Path, options: SessionOptions) -> Result<Self, SessionError> {
        Self::with_database_and_options(root, RootDatabaseImpl::new(), options)
    }

    /// Adopt the caller's verified composition; project and analysis use the same reader snapshot.
    ///
    /// # Errors
    /// Returns non-UTF-8 roots or exhausted file identities, without reopening source files.
    pub fn from_reader(
        reader: SourceReader,
        options: SessionOptions,
    ) -> Result<Self, SessionError> {
        let scope =
            AnalysisScope::canonical_for(reader.project_type().map_err(SessionError::Source)?);
        Self::with_inputs(
            MetadataInputs::from_reader(reader),
            RootDatabaseImpl::new(),
            options,
            scope,
        )
    }

    /// Admit source syntax explicitly without creating an unverified metadata configuration.
    ///
    /// # Errors
    /// Canonical admission rejects unsupported project types; both modes validate source identities.
    pub fn from_reader_with_scope(
        reader: SourceReader,
        options: SessionOptions,
        scope: AnalysisScope,
    ) -> Result<Self, SessionError> {
        Self::with_inputs(
            MetadataInputs::from_reader(reader),
            RootDatabaseImpl::new(),
            options,
            scope,
        )
    }

    /// State the exact admitted analysis scope without promoting raw source readiness to semantics.
    #[must_use]
    pub const fn scope(&self) -> AnalysisScope {
        self.scope
    }

    /// Tests may observe Salsa executions without enabling instrumentation for every consumer.
    #[cfg(test)]
    fn with_database(root: &Path, database: RootDatabaseImpl) -> Result<Self, SessionError> {
        Self::with_database_and_options(root, database, SessionOptions::default())
    }

    /// Allocate one stable source-root map for canonical descriptor identities and module bodies.
    fn with_database_and_options(
        root: &Path,
        database: RootDatabaseImpl,
        options: SessionOptions,
    ) -> Result<Self, SessionError> {
        let inputs = MetadataInputs::open(root).map_err(SessionError::Source)?;
        let scope = AnalysisScope::canonical_for(
            inputs
                .source_reader()
                .project_type()
                .map_err(SessionError::Source)?,
        );
        Self::with_inputs(inputs, database, options, scope)
    }

    /// Keep one authoritative listing for all consumers, whether opened directly or via a project.
    fn with_inputs(
        inputs: MetadataInputs,
        mut database: RootDatabaseImpl,
        options: SessionOptions,
        scope: AnalysisScope,
    ) -> Result<Self, SessionError> {
        let reader = inputs.source_reader();
        let external_kind = external::admit(reader, scope)?;
        set_configuration_context(reader, &mut database, scope)?;
        let mut allocator = crate::composition::FileIdAllocator::default();
        let selected_root = root::RootInputs::register(
            inputs,
            &mut database,
            scope,
            external_kind,
            crate::composition::RootId::from_index(0).map_err(|_| SessionError::TooManyFiles)?,
            &mut allocator,
        )?;
        Ok(Self {
            scope,
            selected_root,
            other_roots: BTreeMap::new(),
            composition_sha256: None,
            database,
            revision: SessionRevision::default(),
            loaded_module_bytes: 0,
            loaded_metadata_source_bytes: 0,
            loaded_resource_bytes: 0,
            loaded_query_bytes: 0,
            options,
        })
    }

    /// Address this session's current input state, including failed accepted updates.
    #[must_use]
    pub const fn revision(&self) -> SessionRevision {
        self.revision
    }

    /// Return the exact source root without reopening or resolving its path a second time.
    #[must_use]
    pub fn root(&self) -> &Path {
        self.selected_root.inputs.source_reader().root()
    }

    /// Enumerate only declared owner files, independent of their current readability.
    pub fn owner_paths(&self) -> impl Iterator<Item = &SourcePath> {
        self.selected_root.owners.keys()
    }

    /// Return current projection facts, including the exact source revision and unsupported state.
    #[must_use]
    pub fn owner(&self, path: &SourcePath) -> Option<&OwnerModel> {
        self.selected_root
            .owners
            .get(path)
            .map(|entry| &entry.model)
    }

    /// Share current metadata for an IDE tree/property request, including unsupported BSL families.
    ///
    /// # Errors
    /// Returns a stale revision or undeclared owner; unavailable input remains explicitly visible.
    pub fn metadata(
        &self,
        path: &SourcePath,
        expected: SessionRevision,
    ) -> Result<crate::MetadataView, SessionRequestError> {
        self.validate_revision(expected)?;
        let input = self.selected_root.inputs.owner(path).ok_or_else(|| {
            SessionRequestError::Metadata(UpdateError::UnknownOwner(path.clone()))
        })?;
        let model = self.owner(path).ok_or_else(|| {
            SessionRequestError::Metadata(UpdateError::UnknownOwner(path.clone()))
        })?;
        Ok(crate::MetadataView {
            revision: expected,
            source: path.clone(),
            origin: input.origin,
            source_sha256: input.source_sha256.clone(),
            input: input.state.clone(),
            model: model.clone(),
        })
    }

    /// Read the stable native main descriptor identity without generating that file.
    #[must_use]
    pub fn native_path(&self, path: &SourcePath) -> Option<&SourcePath> {
        self.selected_root
            .owners
            .get(path)
            .map(|entry| &entry.binding.native_path)
    }

    /// Refresh one saved value; invalid or missing current input clears its kernel slot.
    ///
    /// # Errors
    /// Returns rejected controller updates; active overlays and stale requests remain intact.
    pub fn load_saved(
        &mut self,
        path: &SourcePath,
        expected: SessionRevision,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        self.load_saved_at(&self.selected_source(path), expected, request)
    }

    /// Parse an editor's YAML using the saved reader and publish exactly that current model.
    ///
    /// # Errors
    /// Returns stale revisions/document versions or an undeclared owner before accepting text.
    pub fn apply_overlay(
        &mut self,
        path: &SourcePath,
        expected: SessionRevision,
        document_version: i64,
        text: &str,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        self.apply_overlay_at(
            &self.selected_source(path),
            expected,
            document_version,
            text,
            request,
        )
    }

    /// Explicitly restore disk input when the editor closes, never the cached saved projection.
    ///
    /// # Errors
    /// Returns stale requests or an undeclared owner; disk failures become the current state.
    pub fn close_overlay(
        &mut self,
        path: &SourcePath,
        expected: SessionRevision,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        self.close_overlay_at(&self.selected_source(path), expected, request)
    }

    /// Resolve through the actual kernel; pending/unsupported/invalid values never fall back.
    ///
    /// # Errors
    /// Returns a stale session revision or an undeclared owner.
    pub fn resolve(
        &self,
        path: &SourcePath,
        expected: SessionRevision,
    ) -> Result<Option<CanonicalMetadata>, SessionRequestError> {
        self.validate_revision(expected)?;
        let entry = self.selected_root.owners.get(path).ok_or_else(|| {
            SessionRequestError::Metadata(UpdateError::UnknownOwner(path.clone()))
        })?;
        let ModelState::Ready(value) = &entry.model.state else {
            return Ok(None);
        };
        let Some(name) = &entry.binding.name else {
            return Ok(None);
        };
        Ok(match value.as_ref() {
            CanonicalMetadata::Form(_) => {
                ide_db::metadata::resolve_form(&self.database, entry.slot, name.clone())
                    .map(CanonicalMetadata::Form)
            }
            CanonicalMetadata::Role(_) => {
                resolve_role(&self.database, self.selected_root.listing, name.clone())
                    .map(CanonicalMetadata::Role)
            }
            CanonicalMetadata::IntegrationService(_) => resolve_integration_service(
                &self.database,
                self.selected_root.listing,
                name.clone(),
            )
            .map(CanonicalMetadata::IntegrationService),
            CanonicalMetadata::WebService(_) => {
                resolve_web_service(&self.database, self.selected_root.listing, name.clone())
                    .map(CanonicalMetadata::WebService)
            }
            CanonicalMetadata::HTTPService(_) => {
                resolve_http_service(&self.database, self.selected_root.listing, name.clone())
                    .map(CanonicalMetadata::HTTPService)
            }
            CanonicalMetadata::Subsystem(_) => {
                resolve_subsystem(&self.database, self.selected_root.listing, name.clone())
                    .map(CanonicalMetadata::Subsystem)
            }
            CanonicalMetadata::ScheduledJob(_) => {
                resolve_scheduled_job(&self.database, self.selected_root.listing, name.clone())
                    .map(CanonicalMetadata::ScheduledJob)
            }
            CanonicalMetadata::EventSubscription(_) => {
                resolve_event_subscription(&self.database, self.selected_root.listing, name.clone())
                    .map(CanonicalMetadata::EventSubscription)
            }
            CanonicalMetadata::Object(object) => resolve_metadata_object(
                &self.database,
                self.selected_root.listing,
                object.mdo_type,
                name.clone(),
            )
            .map(CanonicalMetadata::Object),
            CanonicalMetadata::Register(register) => resolve_register(
                &self.database,
                self.selected_root.listing,
                register.mdo_type(),
                name.clone(),
            )
            .map(CanonicalMetadata::Register),
            CanonicalMetadata::CommonModule(_) => {
                resolve_common_module(&self.database, self.selected_root.listing, name.clone())
                    .map(CanonicalMetadata::CommonModule)
            }
            CanonicalMetadata::SessionParameter(_) => ide_db::metadata::resolve_session_parameter(
                &self.database,
                self.selected_root.listing,
                name.clone(),
            )
            .map(CanonicalMetadata::SessionParameter),
            CanonicalMetadata::CommonAttribute(_) => ide_db::metadata::resolve_common_attribute(
                &self.database,
                self.selected_root.listing,
                name.clone(),
            )
            .map(CanonicalMetadata::CommonAttribute),
            CanonicalMetadata::CommonCommand(_) => ide_db::metadata::resolve_common_command(
                &self.database,
                self.selected_root.listing,
                name.clone(),
            )
            .map(CanonicalMetadata::CommonCommand),
            CanonicalMetadata::DefinedType(model) => {
                resolve_defined_type(&self.database, self.selected_root.listing, name.clone())
                    .map(|_| CanonicalMetadata::DefinedType(Arc::clone(model)))
            }
        })
    }

    /// Never equate a ready main descriptor with complete nested metadata or BSL analysis.
    #[must_use]
    pub fn summary(&self) -> SessionSummary {
        let mut summary = SessionSummary {
            revision: self.revision,
            inputs: self.selected_root.inputs.summary(),
            ready_models: 0,
            structure_only: 0,
            source_only: 0,
            pending: 0,
            unsupported: 0,
            invalid: 0,
            unavailable: 0,
            descriptors: 0,
            modeled_descriptors: 0,
            main_models_complete: false,
            descriptor_models_complete: false,
            metadata_context_complete: self.scope.has_configuration_context(),
        };
        for entry in self.selected_root.owners.values() {
            summary.descriptors += entry.model.descriptors;
            summary.modeled_descriptors += entry.model.modeled_descriptors;
            match entry.model.state {
                ModelState::Pending => summary.pending += 1,
                ModelState::Ready(_) => summary.ready_models += 1,
                ModelState::StructureOnly => {
                    summary.structure_only += 1;
                }
                ModelState::SourceOnly => summary.source_only += 1,
                ModelState::Unsupported(_) => summary.unsupported += 1,
                ModelState::Invalid { .. } => summary.invalid += 1,
                ModelState::Unavailable(_) => summary.unavailable += 1,
            }
        }
        summary.main_models_complete = summary.inputs.metadata_input_complete
            && self.scope.has_semantic_context()
            && summary.pending == 0
            && summary.unsupported == 0
            && summary.invalid == 0
            && summary.unavailable == 0;
        summary.descriptor_models_complete =
            self.scope.has_semantic_context() && self.composition_models_complete();
        summary
    }

    /// Validate the shared clock before either a metadata or module request can affect inputs.
    pub(super) fn validate_revision(
        &self,
        expected: SessionRevision,
    ) -> Result<(), SessionRequestError> {
        if self.revision != expected {
            return Err(SessionRequestError::StaleRevision {
                expected,
                actual: self.revision,
            });
        }
        Ok(())
    }

    /// Reject context-dependent operations before they can read or publish source inputs.
    pub(super) fn require_semantic_context(
        &self,
        expected: SessionRevision,
    ) -> Result<(), SessionRequestError> {
        self.validate_revision(expected)?;
        if self.scope == AnalysisScope::LocalSyntax {
            return Err(SessionRequestError::ContextRequired);
        }
        Ok(())
    }

    /// Full diagnostics cannot mistake an external owner's missing global context for missing symbols.
    pub(super) fn require_configuration_context(
        &self,
        expected: SessionRevision,
    ) -> Result<(), SessionRequestError> {
        self.require_semantic_context(expected)?;
        if !self.scope.has_configuration_context() {
            return Err(SessionRequestError::ConfigurationContextRequired);
        }
        Ok(())
    }
}

/// External owners never install a Configuration; syntax retains its existing empty context.
fn set_configuration_context(
    reader: &SourceReader,
    database: &mut RootDatabaseImpl,
    scope: AnalysisScope,
) -> Result<(), SessionError> {
    if scope == AnalysisScope::CanonicalExternal {
        return external::install(reader, database);
    }
    let cache = Arc::new(GraphConfigCache::new());
    if scope == AnalysisScope::CanonicalConfiguration {
        let declaration = reader
            .configuration()
            .declarations
            .first()
            .ok_or(SessionError::MissingListing)?;
        let uuid = declaration.uuid.as_str().parse().map_err(|_| {
            SessionError::Source(Error {
                code: "native_uuid_invalid",
                location: vec![reader.configuration().path.as_str().into()],
            })
        })?;
        cache.insert(
            reader.root().to_path_buf(),
            Arc::new(Configuration::with_identity(uuid, &declaration.name)),
        );
        database.set_all_config_paths(vec![(None, reader.root().to_path_buf())]);
    } else {
        database.set_all_config_paths(Vec::new());
    }
    database.set_graph_config_cache(cache);
    Ok(())
}

/// Extension/base composition is admitted separately; a standalone CFE cannot claim complete analysis.
fn require_supported_project(reader: &SourceReader) -> Result<(), SessionError> {
    let project_type = reader.project_type().map_err(SessionError::Source)?;
    if project_type != eska::project_core::ProjectType::Configuration {
        return Err(SessionError::UnsupportedProjectType(project_type));
    }
    Ok(())
}

/// Current unavailable values clear the slot; its clock tracks the dependencies actually published.
fn set_slot(
    database: &mut RootDatabaseImpl,
    slot: CanonicalMetadataInput,
    state: &ModelState,
    revision: SessionRevision,
) {
    let value = match state {
        ModelState::Ready(value) => Some(Arc::clone(value)),
        _ => None,
    };
    slot.set_value(database).to(value);
    slot.set_source_revision(database).to(revision.to_string());
}
