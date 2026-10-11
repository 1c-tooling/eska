//! One resident metadata session connects the shared input controller to tracked kernel slots.

mod analysis_inputs;
#[cfg(test)]
mod event_subscription_tests;
#[cfg(test)]
mod http_service_tests;
#[cfg(test)]
mod integration_completion_tests;
#[cfg(test)]
mod integration_service_tests;
pub mod metadata_resources;
#[cfg(test)]
mod module_tests;
mod saved_changes;
#[cfg(test)]
mod scheduled_job_tests;
#[cfg(test)]
mod subsystem_tests;
#[cfg(test)]
pub mod tests;
#[cfg(test)]
mod web_service_tests;

use std::{collections::BTreeMap, path::Path, sync::Arc};

use base_db::{SourceDatabase, SourceRoot, SourceRootId};
use bsl_metadata::Configuration;
use eska::{
    analysis_core::{InputRequest, InputState, InputSummary, MetadataInputs, UpdateError},
    project_core::source_format::{Error, SourcePath, SourceReader},
};
use ide_db::{
    GraphConfigCache, RootDatabaseImpl,
    metadata::{
        CanonicalMetadata, CanonicalMetadataInput, CanonicalMetadataInputs, MetadataListingData,
        MetadataListingInput, resolve_common_module, resolve_defined_type,
        resolve_event_subscription, resolve_http_service, resolve_integration_service,
        resolve_metadata_object, resolve_register, resolve_role, resolve_scheduled_job,
        resolve_subsystem, resolve_web_service,
    },
};
use salsa::Setter;
use vfs::{FileId, VfsPath, file_set::FileSet};

use crate::{
    ModelState, OwnerModel, SessionOptions, SessionRequestError, SessionRevision,
    binding::OwnerBinding, modules::ModuleEntry, projection,
};

/// Opening failures remain structured; a transport or CLI decides their presentation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionError {
    Source(Error),
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
    pub pending: usize,
    pub unsupported: usize,
    pub invalid: usize,
    pub unavailable: usize,
    pub descriptors: usize,
    pub modeled_descriptors: usize,
    pub main_models_complete: bool,
    /// Coverage of parsed descriptors only; resources and BSL are outside this count.
    pub descriptor_models_complete: bool,
}

/// A binding and its current model always share the same tracked slot.
struct OwnerEntry {
    binding: OwnerBinding,
    slot: CanonicalMetadataInput,
    model: OwnerModel,
    source_bytes: u64,
}

/// The composition is immutable until reopening; updates replace exactly one canonical input.
pub struct AnalysisSession {
    pub(super) inputs: MetadataInputs,
    pub(super) database: RootDatabaseImpl,
    pub(super) revision: SessionRevision,
    pub(super) modules: BTreeMap<SourcePath, ModuleEntry>,
    pub(super) loaded_module_bytes: u64,
    pub(super) loaded_metadata_source_bytes: u64,
    pub(super) loaded_resource_bytes: u64,
    metadata_resources: BTreeMap<SourcePath, metadata_resources::MetadataResourceEntry>,
    pub(super) options: SessionOptions,
    pub(super) incomplete_metadata: usize,
    pub(super) incomplete_modules: usize,
    listing: MetadataListingInput,
    owners: BTreeMap<SourcePath, OwnerEntry>,
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
        Self::with_inputs(
            MetadataInputs::from_reader(reader),
            RootDatabaseImpl::new(),
            options,
        )
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
        Self::with_inputs(inputs, database, options)
    }

    /// Keep one authoritative listing for all consumers, whether opened directly or via a project.
    fn with_inputs(
        inputs: MetadataInputs,
        mut database: RootDatabaseImpl,
        options: SessionOptions,
    ) -> Result<Self, SessionError> {
        let reader = inputs.source_reader();
        let root_key = reader
            .root()
            .to_str()
            .ok_or(SessionError::NonUtf8Root)?
            .to_owned();
        // No fallback may load a partial native configuration from the ESKA directory.
        let cache = Arc::new(GraphConfigCache::new());
        let name = reader
            .configuration()
            .declarations
            .first()
            .map_or("", |declaration| declaration.name.as_str());
        cache.insert(
            reader.root().to_path_buf(),
            Arc::new(Configuration::new(name)),
        );
        database.set_graph_config_cache(cache);
        database.set_all_config_paths(vec![(None, reader.root().to_path_buf())]);

        let mut file_set = FileSet::new();
        let mut owners = BTreeMap::new();
        let mut structure = MetadataListingData::default();
        let mut slots = CanonicalMetadataInputs::default();
        for (index, path) in inputs.owner_paths().enumerate() {
            let identity = reader.declared_owner(path).map_err(SessionError::Source)?;
            let main = FileId(u32::try_from(index).map_err(|_| SessionError::TooManyFiles)?);
            let binding = OwnerBinding {
                kind: identity.kind,
                name: identity.name.map(str::to_owned),
                main,
                native_path: identity.native_path.clone(),
            };
            file_set.insert(
                main,
                VfsPath::new(reader.root().join(identity.native_path.as_path())),
            );
            database.set_file_source_root(main, SourceRootId(0));
            database.set_file_unreadable(main);
            binding.add_to_listing(&mut structure);
            let slot = CanonicalMetadataInput::new(&database, None, "pending".into());
            slots.insert(main, slot);
            let model = OwnerModel {
                revision: SessionRevision::default(),
                origin: eska::analysis_core::InputOrigin::Saved,
                source_sha256: None,
                descriptors: 0,
                parse_issues: 0,
                modeled_descriptors: 0,
                state: ModelState::Pending,
            };
            owners.insert(
                path.clone(),
                OwnerEntry {
                    binding,
                    slot,
                    model,
                    source_bytes: 0,
                },
            );
        }
        let modules =
            crate::binding::register_modules(reader, owners.len(), &mut database, &mut file_set)?;
        crate::binding::connect_common_bodies(&mut structure, &modules)?;
        crate::binding::connect_service_bodies(&mut structure, &modules)?;
        database.set_source_root(SourceRootId(0), SourceRoot::new_local(file_set));
        database.set_metadata_listing(&root_key, structure);
        let listing = database
            .metadata_listing(&root_key)
            .ok_or(SessionError::MissingListing)?;
        listing
            .set_canonical_metadata(&mut database)
            .to(Arc::new(slots));
        let metadata_resources = metadata_resources::register(reader);
        Ok(Self {
            incomplete_metadata: owners.len(),
            incomplete_modules: modules.len(),
            inputs,
            database,
            listing,
            owners,
            modules,
            revision: SessionRevision::default(),
            loaded_module_bytes: 0,
            loaded_metadata_source_bytes: 0,
            loaded_resource_bytes: 0,
            metadata_resources,
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
        self.inputs.source_reader().root()
    }

    /// Enumerate only declared owner files, independent of their current readability.
    pub fn owner_paths(&self) -> impl Iterator<Item = &SourcePath> {
        self.owners.keys()
    }

    /// Return current projection facts, including the exact source revision and unsupported state.
    #[must_use]
    pub fn owner(&self, path: &SourcePath) -> Option<&OwnerModel> {
        self.owners.get(path).map(|entry| &entry.model)
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
        let input = self.inputs.owner(path).ok_or_else(|| {
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
        self.owners
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
        self.validate_revision(expected)?;
        let next = self.revision.next()?;
        let previous = self.owners.get(path).map_or(0, |entry| entry.source_bytes);
        let request = self.source_budget_request(previous, request);
        self.inputs
            .load_saved(path, self.inputs.revision(), &request)?;
        self.publish(path, next)
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
        self.validate_revision(expected)?;
        let next = self.revision.next()?;
        let previous = self.owners.get(path).map_or(0, |entry| entry.source_bytes);
        let request = self.source_budget_request(previous, request);
        self.inputs.apply_overlay(
            path,
            self.inputs.revision(),
            document_version,
            text,
            &request,
        )?;
        self.publish(path, next)
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
        self.validate_revision(expected)?;
        let next = self.revision.next()?;
        let previous = self.owners.get(path).map_or(0, |entry| entry.source_bytes);
        let request = self.source_budget_request(previous, request);
        self.inputs
            .close_overlay(path, self.inputs.revision(), &request)?;
        self.publish(path, next)
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
        let entry = self.owners.get(path).ok_or_else(|| {
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
            CanonicalMetadata::Role(_) => resolve_role(&self.database, self.listing, name.clone())
                .map(CanonicalMetadata::Role),
            CanonicalMetadata::IntegrationService(_) => {
                resolve_integration_service(&self.database, self.listing, name.clone())
                    .map(CanonicalMetadata::IntegrationService)
            }
            CanonicalMetadata::WebService(_) => {
                resolve_web_service(&self.database, self.listing, name.clone())
                    .map(CanonicalMetadata::WebService)
            }
            CanonicalMetadata::HTTPService(_) => {
                resolve_http_service(&self.database, self.listing, name.clone())
                    .map(CanonicalMetadata::HTTPService)
            }
            CanonicalMetadata::Subsystem(_) => {
                resolve_subsystem(&self.database, self.listing, name.clone())
                    .map(CanonicalMetadata::Subsystem)
            }
            CanonicalMetadata::ScheduledJob(_) => {
                resolve_scheduled_job(&self.database, self.listing, name.clone())
                    .map(CanonicalMetadata::ScheduledJob)
            }
            CanonicalMetadata::EventSubscription(_) => {
                resolve_event_subscription(&self.database, self.listing, name.clone())
                    .map(CanonicalMetadata::EventSubscription)
            }
            CanonicalMetadata::Object(object) => {
                resolve_metadata_object(&self.database, self.listing, object.mdo_type, name.clone())
                    .map(CanonicalMetadata::Object)
            }
            CanonicalMetadata::Register(register) => resolve_register(
                &self.database,
                self.listing,
                register.mdo_type(),
                name.clone(),
            )
            .map(CanonicalMetadata::Register),
            CanonicalMetadata::CommonModule(_) => {
                resolve_common_module(&self.database, self.listing, name.clone())
                    .map(CanonicalMetadata::CommonModule)
            }
            CanonicalMetadata::DefinedType(model) => {
                resolve_defined_type(&self.database, self.listing, name.clone())
                    .map(|_| CanonicalMetadata::DefinedType(Arc::clone(model)))
            }
        })
    }

    /// Never equate a ready main descriptor with complete nested metadata or BSL analysis.
    #[must_use]
    pub fn summary(&self) -> SessionSummary {
        let mut summary = SessionSummary {
            revision: self.revision,
            inputs: self.inputs.summary(),
            ready_models: 0,
            structure_only: 0,
            pending: 0,
            unsupported: 0,
            invalid: 0,
            unavailable: 0,
            descriptors: 0,
            modeled_descriptors: 0,
            main_models_complete: false,
            descriptor_models_complete: false,
        };
        for entry in self.owners.values() {
            summary.descriptors += entry.model.descriptors;
            summary.modeled_descriptors += entry.model.modeled_descriptors;
            match entry.model.state {
                ModelState::Pending => summary.pending += 1,
                ModelState::Ready(_) => summary.ready_models += 1,
                ModelState::StructureOnly => {
                    summary.structure_only += 1;
                }
                ModelState::Unsupported(_) => summary.unsupported += 1,
                ModelState::Invalid { .. } => summary.invalid += 1,
                ModelState::Unavailable(_) => summary.unavailable += 1,
            }
        }
        summary.main_models_complete = summary.inputs.metadata_input_complete
            && summary.pending == 0
            && summary.unsupported == 0
            && summary.invalid == 0
            && summary.unavailable == 0;
        summary.descriptor_models_complete = self.incomplete_metadata == 0;
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

    /// Publish source revision and typed value together before exposing the new owner status.
    fn publish(
        &mut self,
        path: &SourcePath,
        next: SessionRevision,
    ) -> Result<(), SessionRequestError> {
        let input = self.inputs.owner(path).ok_or_else(|| {
            SessionRequestError::Metadata(UpdateError::UnknownOwner(path.clone()))
        })?;
        let resource = metadata_resources::dependency(self.inputs.source_reader(), path).and_then(
            |resource| {
                self.metadata_resources
                    .get(resource)
                    .map(|entry| (resource, &entry.input))
            },
        );
        let entry = self.owners.get_mut(path).ok_or_else(|| {
            SessionRequestError::Metadata(UpdateError::UnknownOwner(path.clone()))
        })?;
        let model = projection::project(entry.binding.kind, input, next, resource);
        let source_bytes = match &input.state {
            InputState::Ready(snapshot) => snapshot.source_bytes,
            InputState::Pending | InputState::Unavailable(_) => 0,
        };
        let was_complete = entry.model.descriptors_complete();
        let is_complete = model.descriptors_complete();
        let value = match &model.state {
            ModelState::Ready(value) => Some(Arc::clone(value)),
            _ => None,
        };
        entry.slot.set_value(&mut self.database).to(value);
        entry
            .slot
            .set_source_revision(&mut self.database)
            .to(format!(
                "{:?}:{}",
                model.revision,
                model.source_sha256.as_deref().unwrap_or_default(),
            ));
        self.loaded_metadata_source_bytes -= entry.source_bytes;
        self.loaded_metadata_source_bytes += source_bytes;
        entry.source_bytes = source_bytes;
        entry.model = model;
        if was_complete && !is_complete {
            self.incomplete_metadata += 1;
        } else if !was_complete && is_complete {
            self.incomplete_metadata -= 1;
        }
        self.revision = next;
        Ok(())
    }
}
