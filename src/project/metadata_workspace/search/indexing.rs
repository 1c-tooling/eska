use super::{IndexFailure, IndexProgress, IndexState, Record, SearchIndex};
use crate::project::{
    configurator::ConfiguratorTree,
    metadata_model::{LocalizedText, MetadataKind, NodeId, ObjectId},
    metadata_parser::PropertiesMode,
    metadata_workspace::{ProjectSession, WorkspaceError},
};
use std::collections::{BTreeMap, BTreeSet};

impl ProjectSession {
    /// Explicitly schedule full metadata coverage; opening/rendering the tree never calls this.
    pub fn start_search_index(&mut self) {
        self.search_index = SearchIndex::default();
        self.search_index.state = IndexState::Building;
        self.search_index
            .pending
            .insert(self.source.root().id().clone(), Vec::new());
    }

    /// Visit up to 64 descriptors per call; zero performs no work, cancellation pauses between calls.
    ///
    /// Every failure is retained in `search_failures` and progress remains visibly incomplete.
    pub fn index_search_step(&mut self, descriptors: usize) -> IndexProgress {
        if self.search_index.state != IndexState::Building {
            return self.search_progress();
        }
        for _ in 0..descriptors.min(64) {
            let Some((owner, ancestors)) = self.search_index.pending.pop_first() else {
                break;
            };
            if let Err(error) = self.index_descriptor(&owner, &ancestors) {
                self.search_index
                    .failures
                    .insert(owner.clone(), IndexFailure::Read(error));
                self.search_index.dirty.insert(owner);
            }
        }
        self.search_index.settle();
        self.search_progress()
    }

    /// Inspect structured failures without copying IO errors or hiding unsupported XML fragments.
    pub fn search_failures(&self) -> impl Iterator<Item = (&ObjectId, &IndexFailure)> {
        self.search_index.failures.iter()
    }

    /// Prepare one descriptor's search data without loading BSL, payloads, or visible tree nodes.
    fn index_descriptor(
        &mut self,
        owner: &ObjectId,
        ancestors: &[NodeId],
    ) -> Result<(), WorkspaceError> {
        let parsed = self.load(owner, PropertiesMode::Summary)?;
        let tree = ConfiguratorTree::build(&self.schema, &parsed, &BTreeMap::new())
            .map_err(WorkspaceError::Tree)?;
        self.verify_source(owner)?;
        let references: BTreeSet<_> = parsed
            .references
            .iter()
            .map(|reference| reference.id.clone())
            .collect();
        let removed: Vec<_> = self
            .search_index
            .references
            .get(owner)
            .into_iter()
            .flatten()
            .filter(|id| !references.contains(*id))
            .cloned()
            .collect();
        for id in removed {
            self.remove_search_subtree(&id)?;
        }
        // First-time descriptors cannot own old records. Avoid a quadratic full-index scan.
        if self.search_index.references.contains_key(owner) {
            self.search_index
                .records
                .retain(|_, record| &record.owner != owner);
        }
        for object in &parsed.objects {
            let id = object.metadata.id();
            self.insert_search_record(
                owner,
                id,
                object.metadata.kind(),
                object.metadata.name(),
                &object.synonyms,
                full_ancestry(&tree, id, ancestors)?,
            )?;
        }
        for reference in &parsed.references {
            let id = &reference.id;
            let ancestry = full_ancestry(&tree, id, ancestors)?;
            if !self.search_index.references.contains_key(id)
                && !self.search_index.failures.contains_key(id)
            {
                self.search_index
                    .pending
                    .insert(id.clone(), ancestry[..ancestry.len() - 1].to_vec());
            }
            if !self.search_index.records.contains_key(id) {
                self.insert_search_record(
                    owner,
                    id,
                    reference.kind,
                    &reference.name,
                    &[],
                    ancestry,
                )?;
            }
        }
        self.search_index
            .references
            .insert(owner.clone(), references);
        self.search_index.dirty.remove(owner);
        self.search_index.failures.remove(owner);
        if !parsed.diagnostics.is_empty() {
            self.search_index.failures.insert(
                owner.clone(),
                IndexFailure::Unsupported(parsed.diagnostics.clone()),
            );
        }
        Ok(())
    }

    /// Normalize text once, retaining distinct identities even when labels are identical.
    fn insert_search_record(
        &mut self,
        owner: &ObjectId,
        id: &ObjectId,
        kind: MetadataKind,
        name: &str,
        synonyms: &[LocalizedText],
        ancestry: Vec<NodeId>,
    ) -> Result<(), WorkspaceError> {
        let (descriptor, paths) = self
            .source
            .descriptor_candidates(id)
            .map_err(WorkspaceError::Source)?;
        for path in paths {
            self.search_index.paths.insert(path, descriptor.clone());
        }
        self.search_index.records.insert(
            id.clone(),
            Record {
                owner: owner.clone(),
                kind,
                name: name.to_owned(),
                folded: name.to_lowercase(),
                synonyms: synonyms.to_vec(),
                folded_synonyms: synonyms
                    .iter()
                    .map(|text| (text.language.clone(), text.content.to_lowercase()))
                    .collect(),
                ancestry,
            },
        );
        Ok(())
    }

    /// Delete removed descendants from both the search index and descriptor cache.
    fn remove_search_subtree(&mut self, owner: &ObjectId) -> Result<(), WorkspaceError> {
        let node = NodeId::Object(owner.clone());
        let ids: Vec<_> = self
            .search_index
            .records
            .iter()
            .filter(|(_, record)| record.ancestry.contains(&node))
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            self.search_index.records.remove(&id);
            self.search_index.pending.remove(&id);
            self.search_index.references.remove(&id);
            self.search_index.failures.remove(&id);
            self.search_index.dirty.remove(&id);
            let (_, paths) = self
                .source
                .descriptor_candidates(&id)
                .map_err(WorkspaceError::Source)?;
            for path in paths {
                self.cache.remove(&path);
                self.fingerprints.remove(&path);
                self.search_index.paths.remove(&path);
            }
        }
        Ok(())
    }
}

/// Combine the local projection with its known global virtual ancestry.
fn full_ancestry(
    tree: &ConfiguratorTree,
    owner: &ObjectId,
    ancestors: &[NodeId],
) -> Result<Vec<NodeId>, WorkspaceError> {
    let mut current = Some(NodeId::Object(owner.clone()));
    let mut path = Vec::new();
    while let Some(id) = current.take() {
        let node = tree
            .node(&id)
            .ok_or_else(|| WorkspaceError::UnknownNode(id.clone()))?;
        path.push(id);
        current.clone_from(&node.parent);
    }
    path.reverse();
    let mut result = ancestors.to_vec();
    result.extend(path);
    Ok(result)
}
