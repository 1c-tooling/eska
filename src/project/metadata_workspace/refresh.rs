use super::{ProjectSession, WorkspaceError};
use crate::project::{
    configurator::ChildrenState,
    metadata_model::{NodeId, ObjectId},
};
use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};

/// Consumers discard older search results and update affected logical objects at this generation.
#[derive(Clone, Debug)]
pub struct RefreshReport {
    pub generation: u64,
    pub affected: Vec<ObjectId>,
}

impl ProjectSession {
    /// Return the event generation shared by navigation and future search results.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Reject an operation based on a superseded client snapshot.
    ///
    /// # Errors
    /// Returns both expected and actual generations when they differ.
    pub const fn check_generation(&self, expected: u64) -> Result<(), WorkspaceError> {
        if expected == self.generation {
            Ok(())
        } else {
            Err(WorkspaceError::StaleGeneration {
                expected,
                actual: self.generation,
            })
        }
    }

    /// Invalidate owners of source-relative client events, including both sides of a rename.
    ///
    /// Events are serialized with reads by the mutable session borrow. New or ambiguous paths
    /// conservatively refresh the root rather than guessing ownership from a missing file.
    ///
    /// # Errors
    /// Rejects escaping paths, exhausted generations, or an invalid root during root refresh.
    pub fn changed_paths(&mut self, paths: &[PathBuf]) -> Result<RefreshReport, WorkspaceError> {
        let mut owners = BTreeSet::new();
        for path in paths {
            if path.as_os_str().is_empty()
                || path
                    .components()
                    .any(|part| !matches!(part, Component::Normal(_)))
            {
                return Err(WorkspaceError::InvalidChangedPath(path.clone()));
            }
            if path == Path::new("ConfigDumpInfo.xml") {
                continue;
            }
            if let Some(owner) = self.path_owner(path) {
                owners.insert(owner);
            } else {
                owners.extend(self.by_path.values().cloned());
                owners.insert(self.source.root().id().clone());
            }
        }
        self.invalidate(&owners)
    }

    /// Force a branch reread, recovering from missed file events.
    ///
    /// # Errors
    /// Returns unknown nodes, bad source layout or a root parse failure.
    pub fn refresh(&mut self, id: &NodeId) -> Result<RefreshReport, WorkspaceError> {
        self.node(id)?;
        let owner = match id {
            NodeId::Object(owner)
            | NodeId::Collection { owner, .. }
            | NodeId::Module { owner, .. } => owner,
        };
        let (owner, _) = self
            .source
            .descriptor_candidates(owner)
            .map_err(WorkspaceError::Source)?;
        if &owner == self.source.root().id() {
            let mut owners: BTreeSet<_> = self.by_path.values().cloned().collect();
            owners.insert(owner);
            self.invalidate(&owners)
        } else {
            self.invalidate(&BTreeSet::from([owner]))
        }
    }

    /// Find exact XML ownership or the nearest known descriptor enclosing an artifact path.
    fn path_owner(&self, path: &Path) -> Option<ObjectId> {
        if let Some(owner) = self.by_path.get(path) {
            return Some(owner.clone());
        }
        let mut parent = path.parent();
        while let Some(directory) = parent {
            if let Some(owner) = self.by_path.get(&directory.with_extension("xml")) {
                return Some(owner.clone());
            }
            parent = directory.parent();
        }
        path.starts_with("Ext")
            .then(|| self.source.root().id().clone())
    }

    /// Invalidate dependencies before any new XML can become visible.
    fn invalidate(&mut self, owners: &BTreeSet<ObjectId>) -> Result<RefreshReport, WorkspaceError> {
        if owners.is_empty() {
            return Ok(RefreshReport {
                generation: self.generation,
                affected: Vec::new(),
            });
        }
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or(WorkspaceError::GenerationExhausted)?;
        let root = self.source.root().id().clone();
        let mut affected = BTreeSet::new();
        for owner in owners.iter().filter(|id| **id != root) {
            self.collapse(owner, &mut affected);
        }
        if owners.contains(&root) {
            self.refresh_root(&mut affected)?;
        }
        Ok(RefreshReport {
            generation: self.generation,
            affected: affected.into_iter().collect(),
        })
    }

    /// Remove descendants while retaining the owner's stable selectable placeholder.
    fn collapse(&mut self, owner: &ObjectId, affected: &mut BTreeSet<ObjectId>) {
        let id = NodeId::Object(owner.clone());
        let mut pending = self
            .nodes
            .get(&id)
            .map_or_else(Vec::new, |node| node.children.clone());
        while let Some(child) = pending.pop() {
            if let Some(node) = self.nodes.remove(&child) {
                pending.extend(node.children);
            }
            if let NodeId::Object(object) = child {
                self.expanded.remove(&object);
                self.objects.remove(&object);
                affected.insert(object);
            }
        }
        self.expanded.remove(owner);
        affected.insert(owner.clone());
        if let Some(node) = self.nodes.get_mut(&id) {
            node.children.clear();
            node.diagnostics.clear();
            node.state = ChildrenState::Unloaded;
        }
        if let Some(object) = self.objects.get_mut(owner) {
            object.uuid = None;
            object.synonyms.clear();
        }
        let paths: Vec<_> = self
            .by_path
            .iter()
            .filter(|(_, id)| affected.contains(*id))
            .map(|(path, _)| path.clone())
            .collect();
        for path in paths {
            self.cache.remove(&path);
            self.fingerprints.remove(&path);
            if self
                .by_path
                .get(&path)
                .is_some_and(|id| !self.objects.contains_key(id))
            {
                self.by_path.remove(&path);
            }
        }
    }

    /// Rebuild root membership while retaining unchanged exposed branches and their cached data.
    fn refresh_root(&mut self, affected: &mut BTreeSet<ObjectId>) -> Result<(), WorkspaceError> {
        let owner = self.source.root().id().clone();
        let old_nodes = std::mem::take(&mut self.nodes);
        let old_objects = std::mem::take(&mut self.objects);
        let old_expanded = std::mem::take(&mut self.expanded);
        self.cache.remove(&self.source.descriptor().to_path_buf());
        self.fingerprints.remove(self.source.descriptor());
        self.by_path.clear();
        if let Err(error) = self.expand(&owner) {
            self.nodes = old_nodes;
            self.objects = old_objects;
            self.expanded = old_expanded;
            self.collapse(&owner, affected);
            self.cache.clear();
            self.fingerprints.clear();
            if let Some(node) = self.nodes.get_mut(&self.root) {
                node.state = ChildrenState::Error;
            }
            return Err(error);
        }
        // Root lists reference top-level owners. Graft only those still declared by the root.
        affected.extend(self.expanded.iter().cloned());
        let retained: Vec<_> = self
            .objects
            .keys()
            .filter(|id| {
                **id != owner && old_expanded.contains(*id) && !self.expanded.contains(*id)
            })
            .cloned()
            .collect();
        for id in retained {
            let node_id = NodeId::Object(id.clone());
            let parent = self
                .nodes
                .get(&node_id)
                .and_then(|node| node.parent.clone());
            let mut pending = vec![node_id.clone()];
            while let Some(id) = pending.pop() {
                if let Some(old) = old_nodes.get(&id) {
                    pending.extend(old.children.clone());
                    self.nodes.insert(id.clone(), old.clone());
                }
                if let NodeId::Object(id) = id
                    && let Some(object) = old_objects.get(&id)
                {
                    self.objects.insert(id.clone(), object.clone());
                    if old_expanded.contains(&id) {
                        self.expanded.insert(id);
                    }
                }
            }
            if let Some(node) = self.nodes.get_mut(&node_id) {
                node.parent = parent;
            }
        }
        affected.insert(owner);
        for id in old_objects.keys().chain(self.objects.keys()) {
            if old_objects.get(id) != self.objects.get(id) {
                affected.insert(id.clone());
            }
        }
        let ids: Vec<_> = self.objects.keys().cloned().collect();
        for id in ids {
            self.register_paths(&id)?;
        }
        // Removed descriptors must not survive and reappear from an old cache entry.
        let removed: Vec<_> = old_objects
            .keys()
            .filter(|id| !self.objects.contains_key(*id))
            .cloned()
            .collect();
        for id in removed {
            let (_, paths) = self
                .source
                .descriptor_candidates(&id)
                .map_err(WorkspaceError::Source)?;
            for path in paths {
                if !self.by_path.contains_key(&path) {
                    self.cache.remove(&path);
                    self.fingerprints.remove(&path);
                }
            }
        }
        Ok(())
    }
}
