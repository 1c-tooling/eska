//! All admitted source roots share one database, source quota and publication clock.

use std::path::Path;

use eska::project_core::source_format::SourcePath;
use ide_db::RootDatabaseImpl;

use super::root::RootInputs;
use crate::{
    AnalysisSession, SessionRequestError, SessionRevision,
    composition::{RootId, SourceKey},
};

#[cfg(all(test, any(feature = "analyzer-cli", feature = "check-cli")))]
mod tests;

mod summary;
pub use summary::{CompositionSummary, ContributionInputs, ContributionSummary};

impl AnalysisSession {
    /// The same source-relative public paths continue to address this selected root.
    #[must_use]
    pub const fn selected_root_id(&self) -> RootId {
        self.selected_root.id
    }

    /// Explicit identities qualify equal paths in different admitted roots without rewriting them.
    #[must_use]
    pub fn selected_source(&self, source: &SourcePath) -> SourceKey {
        SourceKey {
            root: self.selected_root.id,
            source: source.clone(),
        }
    }

    /// Root identity is session-local and always resolves to its verified authoring directory.
    #[must_use]
    pub fn source_root(&self, root: RootId) -> Option<&Path> {
        self.root_inputs(root)
            .ok()
            .map(|root| root.inputs.source_reader().root())
    }

    /// The control digest covers the admitted topology and all immutable source headers/manifests.
    #[must_use]
    pub fn composition_sha256(&self) -> Option<&str> {
        self.composition_sha256.as_deref()
    }

    /// Enumerate the selected root first, then other roots by ID; IDs retain native workspace indices.
    pub fn root_ids(&self) -> impl Iterator<Item = RootId> + '_ {
        std::iter::once(self.selected_root.id).chain(self.other_roots.keys().copied())
    }

    /// Resolve immutable root-local inputs without a second reader or analysis session.
    pub(crate) fn root_inputs(&self, id: RootId) -> Result<&RootInputs, SessionRequestError> {
        if self.selected_root.id == id {
            Ok(&self.selected_root)
        } else {
            self.other_roots
                .get(&id)
                .ok_or(SessionRequestError::UnknownRoot(id))
        }
    }

    /// Borrow one admitted root without granting an independent clock or reader replacement.
    pub(crate) fn root_inputs_mut(
        &mut self,
        id: RootId,
    ) -> Result<&mut RootInputs, SessionRequestError> {
        if self.selected_root.id == id {
            Ok(&mut self.selected_root)
        } else {
            self.other_roots
                .get_mut(&id)
                .ok_or(SessionRequestError::UnknownRoot(id))
        }
    }

    /// Inspect a declared query using its explicit owning root without a neighboring read.
    #[must_use]
    pub fn query_input_at(&self, key: &SourceKey) -> Option<&crate::QueryTextSnapshot> {
        self.root_inputs(key.root)
            .ok()?
            .query_inputs
            .get(&key.source)
            .map(crate::QueryTextInput::snapshot)
    }

    /// Query ownership remains that root's verified declaration independently of semantic readiness.
    #[must_use]
    pub fn query_declaration_at(&self, key: &SourceKey) -> Option<&crate::QueryTextDeclaration> {
        self.root_inputs(key.root)
            .ok()?
            .query_inputs
            .get(&key.source)
            .map(crate::QueryTextInput::declaration)
    }

    /// Inspect an exact current resource without loading its owner or another root.
    #[must_use]
    pub fn metadata_resource_at(&self, key: &SourceKey) -> Option<&crate::MetadataResourceInput> {
        self.root_inputs(key.root)
            .ok()?
            .metadata_resources
            .get(&key.source)
            .map(|entry| &entry.input)
    }

    /// Inspect an exact current module without creating another kernel file identity.
    #[must_use]
    pub fn module_at(&self, key: &SourceKey) -> Option<&crate::ModuleInput> {
        self.root_inputs(key.root)
            .ok()?
            .modules
            .get(&key.source)
            .map(|entry| &entry.model)
    }

    /// Inspect a current owner independently of another root with the same authoring path.
    #[must_use]
    pub fn owner_at(&self, key: &SourceKey) -> Option<&crate::OwnerModel> {
        self.root_inputs(key.root)
            .ok()?
            .owners
            .get(&key.source)
            .map(|entry| &entry.model)
    }

    /// Read current metadata from an explicit source root, without loading saved bytes.
    ///
    /// # Errors
    /// Rejects stale snapshots, unknown roots and undeclared owners.
    pub fn metadata_at(
        &self,
        key: &SourceKey,
        expected: SessionRevision,
    ) -> Result<crate::MetadataView, SessionRequestError> {
        self.validate_revision(expected)?;
        let root = self.root_inputs(key.root)?;
        let input = root
            .inputs
            .owner(&key.source)
            .ok_or_else(|| eska::analysis_core::UpdateError::UnknownOwner(key.source.clone()))?;
        let model = root
            .owners
            .get(&key.source)
            .ok_or_else(|| eska::analysis_core::UpdateError::UnknownOwner(key.source.clone()))?;
        Ok(crate::MetadataView {
            revision: expected,
            source: key.source.clone(),
            origin: input.origin,
            source_sha256: input.source_sha256.clone(),
            input: input.state.clone(),
            model: model.model.clone(),
        })
    }

    /// Load one current saved owner under the host's shared quota and publication clock.
    ///
    /// # Errors
    /// Rejects stale/unknown input and editor authority before publication.
    pub fn load_saved_at(
        &mut self,
        key: &SourceKey,
        expected: SessionRevision,
        request: &eska::analysis_core::InputRequest,
    ) -> Result<(), SessionRequestError> {
        self.validate_revision(expected)?;
        let next = self.revision.next()?;
        let previous = self
            .root_inputs(key.root)?
            .owners
            .get(&key.source)
            .map_or(0, |owner| owner.source_bytes);
        let request = self.source_budget_request(previous, request);
        let root = self.root_inputs_mut(key.root)?;
        root.inputs
            .load_saved(&key.source, root.inputs.revision(), &request)?;
        self.publish_at(key, next)
    }

    /// Apply a root-qualified editor owner without changing a same-named neighboring source.
    ///
    /// # Errors
    /// Rejects stale host/document versions and undeclared input without mutation.
    pub fn apply_overlay_at(
        &mut self,
        key: &SourceKey,
        expected: SessionRevision,
        version: i64,
        text: &str,
        request: &eska::analysis_core::InputRequest,
    ) -> Result<(), SessionRequestError> {
        self.validate_revision(expected)?;
        let next = self.revision.next()?;
        let previous = self
            .root_inputs(key.root)?
            .owners
            .get(&key.source)
            .map_or(0, |owner| owner.source_bytes);
        let request = self.source_budget_request(previous, request);
        let root = self.root_inputs_mut(key.root)?;
        root.inputs
            .apply_overlay(&key.source, root.inputs.revision(), version, text, &request)?;
        self.publish_at(key, next)
    }

    /// Restore only this root's current disk value, including failure instead of a saved fallback.
    ///
    /// # Errors
    /// Rejects stale snapshots and unknown root/source identities before a bounded read.
    pub fn close_overlay_at(
        &mut self,
        key: &SourceKey,
        expected: SessionRevision,
        request: &eska::analysis_core::InputRequest,
    ) -> Result<(), SessionRequestError> {
        self.validate_revision(expected)?;
        let next = self.revision.next()?;
        let previous = self
            .root_inputs(key.root)?
            .owners
            .get(&key.source)
            .map_or(0, |owner| owner.source_bytes);
        let request = self.source_budget_request(previous, request);
        let root = self.root_inputs_mut(key.root)?;
        root.inputs
            .close_overlay(&key.source, root.inputs.revision(), &request)?;
        self.publish_at(key, next)
    }

    /// Publish a changed canonical dependency using the same host quota and global revision.
    pub(super) fn publish_at(
        &mut self,
        key: &SourceKey,
        next: SessionRevision,
    ) -> Result<(), SessionRequestError> {
        let scope = self.scope;
        let (root, database) = self.root_database_mut(key.root)?;
        let (previous, current) = root.publish(database, &key.source, next, scope)?;
        self.loaded_metadata_source_bytes = self.loaded_metadata_source_bytes - previous + current;
        self.revision = next;
        Ok(())
    }

    /// Disjoint fields allow root publication into the one shared tracked database.
    pub(crate) fn root_database_mut(
        &mut self,
        id: RootId,
    ) -> Result<(&mut RootInputs, &mut RootDatabaseImpl), SessionRequestError> {
        let root = if self.selected_root.id == id {
            &mut self.selected_root
        } else {
            self.other_roots
                .get_mut(&id)
                .ok_or(SessionRequestError::UnknownRoot(id))?
        };
        Ok((root, &mut self.database))
    }

    /// Incomplete current overlays must not silently resolve through an older base contribution.
    pub(crate) fn require_current_composition(&self) -> Result<(), SessionRequestError> {
        if self.composition_sha256.is_none() {
            return Ok(());
        }
        for id in self.visible_root_ids(self.selected_root_id()) {
            let root = self.root_inputs(id)?;
            if let Some((source, _)) = root
                .owners
                .iter()
                .find(|(_, entry)| !entry.model.descriptors_complete())
            {
                return Err(SessionRequestError::CompositionInputIncomplete(SourceKey {
                    root: id,
                    source: source.clone(),
                }));
            }
            if let Some((source, _)) = root
                .modules
                .iter()
                .find(|(_, entry)| !matches!(entry.model.state, crate::ModuleState::Ready { .. }))
            {
                return Err(SessionRequestError::CompositionInputIncomplete(SourceKey {
                    root: id,
                    source: source.clone(),
                }));
            }
        }
        Ok(())
    }

    /// Visible-root readiness qualifies selected coverage while unrelated siblings remain pending.
    pub(crate) fn composition_models_complete(&self) -> bool {
        self.visible_root_ids(self.selected_root_id())
            .into_iter()
            .all(|id| {
                self.root_inputs(id)
                    .is_ok_and(|root| root.incomplete_metadata == 0)
            })
    }

    /// Only declared current bodies in the selected root's visibility contribute to completeness.
    pub(crate) fn composition_modules_complete(&self) -> bool {
        self.visible_root_ids(self.selected_root_id())
            .into_iter()
            .all(|id| {
                self.root_inputs(id)
                    .is_ok_and(|root| root.incomplete_modules == 0)
            })
    }

    /// Native topology visibility is anchored to each root, independently of declaration order.
    pub(crate) fn visible_root_ids(&self, root: RootId) -> Vec<RootId> {
        let snapshot = self.database.workspace_configs_snapshot();
        let index = root.index() as usize;
        if index < snapshot.paths.len()
            && snapshot.kinds[index] == ide_db::metadata::RootKind::Extension
        {
            std::iter::once(0)
                .chain(snapshot.closures[index].iter().copied())
                .chain(std::iter::once(index))
                .filter_map(|index| RootId::from_index(index).ok())
                .collect()
        } else {
            vec![root]
        }
    }

    /// Install every listing/slot in one database before any resolver may inspect this composition.
    ///
    /// # Errors
    /// Refuses invalid root identity, missing selected roots and exhausted global file identities.
    #[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
    pub fn from_composition(
        composition: crate::composition::AdmittedComposition,
        options: crate::SessionOptions,
    ) -> Result<Self, crate::SessionError> {
        use crate::{AnalysisScope, SessionError, composition::FileIdAllocator};
        use bsl_metadata::Configuration;
        use eska::analysis_core::MetadataInputs;
        use ide_db::GraphConfigCache;
        use std::{collections::BTreeMap, sync::Arc};
        let (selected, roots, mut snapshot, fingerprint) = composition.into_parts();
        let mut database = RootDatabaseImpl::new();
        let cache = Arc::new(GraphConfigCache::new());
        for root in &roots {
            let reader = root.reader();
            let header = reader
                .configuration()
                .declarations
                .first()
                .ok_or(SessionError::MissingListing)?;
            let uuid = header
                .uuid
                .as_str()
                .parse()
                .map_err(|_| SessionError::MissingListing)?;
            cache.insert(
                reader.root().to_owned(),
                Arc::new(Configuration::with_identity(uuid, &header.name)),
            );
        }
        // Canonical slots and graph headers use verified physical roots. Native listing lookup
        // normalizes separators but not `..`, so its configured roots must use that same identity.
        for ((_, path), canonical) in snapshot.paths.iter_mut().zip(&snapshot.canonical_paths) {
            path.clone_from(canonical);
        }
        database.set_graph_config_cache(cache);
        database.set_workspace_configs_snapshot(snapshot);
        let mut allocator = FileIdAllocator::default();
        let mut inputs = BTreeMap::new();
        for root in roots {
            let id = root.id();
            let state = RootInputs::register(
                MetadataInputs::from_reader(root.into_reader()),
                &mut database,
                AnalysisScope::CanonicalConfiguration,
                None,
                id,
                &mut allocator,
            )?;
            inputs.insert(id, state);
        }
        let selected_root = inputs
            .remove(&selected)
            .ok_or(SessionError::MissingListing)?;
        Ok(Self {
            scope: AnalysisScope::CanonicalConfiguration,
            selected_root,
            other_roots: inputs,
            composition_sha256: Some(fingerprint),
            database,
            revision: SessionRevision::default(),
            loaded_module_bytes: 0,
            loaded_metadata_source_bytes: 0,
            loaded_resource_bytes: 0,
            loaded_query_bytes: 0,
            options,
        })
    }
}
