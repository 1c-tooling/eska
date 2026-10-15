//! Each admitted source retains its own current inputs while sharing the host database and clock.

use std::{collections::BTreeMap, sync::Arc};

use base_db::{SourceDatabase, SourceRoot, SourceRootId};
use eska::{
    analysis_core::{InputState, MetadataInputs, UpdateError},
    project_core::source_format::SourcePath,
};
use ide_db::{
    RootDatabaseImpl,
    metadata::{
        CanonicalMetadataInput, CanonicalMetadataInputs, MetadataListingData, MetadataListingInput,
    },
};
use salsa::Setter;
use vfs::{FileId, VfsPath, file_set::FileSet};

use super::{
    OwnerEntry, ProjectionRevision, external, forms, metadata_resources, query_text, set_slot,
};
use crate::{
    AnalysisScope, ModelState, OwnerModel, SessionError, SessionRequestError, SessionRevision,
    binding::OwnerBinding,
    composition::{FileIdAllocator, RootId},
    modules::ModuleEntry,
    projection,
};

/// Root-local maps distinguish equal authoring paths without creating another analysis host.
pub struct RootInputs {
    pub id: RootId,
    pub inputs: MetadataInputs,
    pub external_kind: Option<bsl_metadata::ExternalObjectKind>,
    pub modules: BTreeMap<SourcePath, ModuleEntry>,
    pub module_sources: BTreeMap<FileId, SourcePath>,
    pub query_inputs: BTreeMap<SourcePath, crate::QueryTextInput>,
    pub(super) metadata_resources: BTreeMap<SourcePath, metadata_resources::MetadataResourceEntry>,
    pub incomplete_metadata: usize,
    pub incomplete_modules: usize,
    pub listing: MetadataListingInput,
    pub(super) owners: BTreeMap<SourcePath, OwnerEntry>,
}

impl RootInputs {
    /// Enumerate only registered text resources without opening their saved payloads.
    #[cfg(feature = "analyzer-cli")]
    pub(crate) fn resource_paths(&self) -> impl Iterator<Item = &SourcePath> {
        self.metadata_resources.keys()
    }

    /// Register every listing and tracked identity without reading current owner or body payloads.
    pub(super) fn register(
        inputs: MetadataInputs,
        database: &mut RootDatabaseImpl,
        scope: AnalysisScope,
        external_kind: Option<bsl_metadata::ExternalObjectKind>,
        id: RootId,
        allocator: &mut FileIdAllocator,
    ) -> Result<Self, SessionError> {
        let reader = inputs.source_reader();
        let root_key = reader
            .root()
            .to_str()
            .ok_or(SessionError::NonUtf8Root)?
            .to_owned();
        let source_root = SourceRootId(id.index());
        let mut file_set = FileSet::new();
        let mut owners = BTreeMap::new();
        let mut structure = MetadataListingData::default();
        let mut slots = CanonicalMetadataInputs::default();
        for path in inputs.owner_paths() {
            let identity = reader.declared_owner(path).map_err(SessionError::Source)?;
            let main = allocator
                .allocate()
                .map_err(|_| SessionError::TooManyFiles)?;
            let mut binding = OwnerBinding {
                kind: identity.kind,
                name: identity.name.map(str::to_owned),
                main,
                native_path: identity.native_path.clone(),
                nested: identity.parent.is_some(),
            };
            file_set.insert(
                main,
                VfsPath::new(reader.root().join(identity.native_path.as_path())),
            );
            database.set_file_source_root(main, source_root);
            database.set_file_unreadable(main);
            if scope != AnalysisScope::CanonicalExternal
                || !external::bind_root(reader, path, &mut binding, &mut structure)?
            {
                binding.add_to_listing(&mut structure);
            }
            let slot = CanonicalMetadataInput::new(database, None, "pending".into());
            slots.insert(main, slot);
            owners.insert(
                path.clone(),
                OwnerEntry {
                    binding,
                    slot,
                    model: OwnerModel {
                        revision: SessionRevision::default(),
                        origin: eska::analysis_core::InputOrigin::Saved,
                        source_sha256: None,
                        descriptors: 0,
                        parse_issues: 0,
                        modeled_descriptors: 0,
                        state: ModelState::Pending,
                        forms: Vec::new(),
                    },
                    source_bytes: 0,
                    primary_revision: None,
                    forms: BTreeMap::new(),
                },
            );
        }
        forms::register(
            reader,
            &mut owners,
            database,
            &mut file_set,
            &mut slots,
            source_root,
            allocator,
        )?;
        let modules = crate::binding::register_modules(
            reader,
            source_root,
            allocator,
            database,
            &mut file_set,
        )?;
        crate::binding::connect_common_bodies(&mut structure, &modules)?;
        crate::binding::connect_service_bodies(&mut structure, &modules)?;
        database.set_source_root(source_root, SourceRoot::new_local(file_set));
        database.set_metadata_listing(&root_key, structure);
        let listing = database
            .metadata_listing(&root_key)
            .ok_or(SessionError::MissingListing)?;
        listing.set_canonical_metadata(database).to(Arc::new(slots));
        let metadata_resources = metadata_resources::register(reader);
        let query_inputs = query_text::register(reader)?;
        Ok(Self {
            id,
            external_kind,
            incomplete_metadata: owners.len(),
            incomplete_modules: modules.len(),
            inputs,
            listing,
            owners,
            module_sources: crate::binding::source_paths_by_file(&modules),
            modules,
            query_inputs,
            metadata_resources,
        })
    }
    /// Project only this root's current owner/resources into its stable slots in the shared database.
    pub(super) fn publish(
        &mut self,
        database: &mut RootDatabaseImpl,
        path: &SourcePath,
        next: SessionRevision,
        scope: AnalysisScope,
    ) -> Result<(u64, u64), SessionRequestError> {
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
        let dependencies = ProjectionRevision {
            owner: input.revision,
            resource: resource.map(|(_, input)| input.revision),
        };
        let mut model = if entry.primary_revision == Some(dependencies) {
            entry.model.clone()
        } else {
            let model = if scope == AnalysisScope::LocalSyntax {
                projection::source_only(input, next)
            } else if let Some(kind) = self
                .external_kind
                .filter(|_| path == &self.inputs.source_reader().configuration().path)
            {
                external::project(kind, input, next)
            } else {
                projection::project(entry.binding.kind, input, next, resource)
            };
            set_slot(database, entry.slot, &model.state, next);
            entry.primary_revision = Some(dependencies);
            model
        };
        forms::publish(
            &mut entry.forms,
            input,
            &self.metadata_resources,
            database,
            next,
            scope,
        );
        model.forms = entry
            .forms
            .values()
            .map(|entry| entry.model.clone())
            .collect();
        model.modeled_descriptors = usize::from(matches!(
            model.state,
            ModelState::Ready(_) | ModelState::StructureOnly
        )) + model
            .forms
            .iter()
            .filter(|form| matches!(form.state, ModelState::Ready(_)))
            .count();
        model.revision = next;
        let source_bytes = match &input.state {
            InputState::Ready(snapshot) => snapshot.source_bytes,
            InputState::Pending | InputState::Unavailable(_) => 0,
        };
        let was_complete = entry.model.descriptors_complete();
        let is_complete = model.descriptors_complete();
        let previous_bytes = entry.source_bytes;
        entry.source_bytes = source_bytes;
        entry.model = model;
        if was_complete && !is_complete {
            self.incomplete_metadata += 1;
        } else if !was_complete && is_complete {
            self.incomplete_metadata -= 1;
        }
        Ok((previous_bytes, source_bytes))
    }
}
