//! File permissions are checked from their current dependencies without a project scan.

use super::{
    FileReason, FileSupport, ObjectSupport, ObjectSupportSnapshot, ProjectSession, SupportSnapshot,
    file_policies,
};
use crate::project::{
    metadata_model::ObjectId,
    metadata_parser::{self, ParsedDescriptor, PropertiesMode},
    metadata_workspace::WorkspaceError,
    support::{Reason, State, Support},
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
    sync::Arc,
};

/// A policy dependency cannot authorize mutation through a different writable alias.
fn unaliased(root: &Path, path: &Path) -> bool {
    let mut physical = root.to_path_buf();
    for part in path.components() {
        physical.push(part);
        match std::fs::symlink_metadata(&physical) {
            Ok(metadata) if metadata.file_type().is_symlink() => return false,
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return false,
        }
    }
    true
}

/// Keep only a bounded set of parsed dependencies; content identities survive Git checkouts.
#[derive(Debug, Default)]
pub(in crate::project::metadata_workspace) struct SupportDescriptors {
    entries: BTreeMap<(PathBuf, [u8; 32]), (Arc<ParsedDescriptor>, usize)>,
    bytes: usize,
}

impl ProjectSession {
    /// Read rules for the requested declared objects without listing files or expanding the tree.
    ///
    /// These presentation states do not authorize a file edit; file checks still verify all physical owners.
    pub fn support_objects(&mut self, ids: &[ObjectId]) -> ObjectSupportSnapshot {
        let (policy, mut diagnostics) = self.source_support_rules();
        let mut read = BTreeMap::new();
        let objects = ids
            .iter()
            .map(|id| {
                let parsed = self.support_object_dependency(id, &mut read);
                let metadata = parsed
                    .as_ref()
                    .filter(|parsed| parsed.diagnostics.is_empty())
                    .and_then(|parsed| {
                        parsed
                            .objects
                            .iter()
                            .find(|object| object.metadata.id() == id)
                    })
                    .map(|object| &object.metadata);
                let Some(metadata) = metadata else {
                    diagnostics.push(format!("descriptor_unavailable:{id}"));
                    return ObjectSupport {
                        object_id: id.clone(),
                        uuid: String::new(),
                        state: State::Unknown,
                        reason: Reason::Unavailable,
                    };
                };
                let (state, reason) = policy
                    .as_deref()
                    .map_or((State::Unknown, Reason::Unavailable), |policy| {
                        policy.object(metadata.uuid())
                    });
                ObjectSupport {
                    object_id: id.clone(),
                    uuid: metadata.uuid().to_owned(),
                    state,
                    reason,
                }
            })
            .collect();
        ObjectSupportSnapshot {
            objects,
            diagnostics,
        }
    }

    /// Classify only requested physical files. Unknown ownership never grants write permission.
    ///
    /// # Errors
    /// Rejects paths outside the source before reading any descriptor.
    pub fn support_files(&mut self, paths: &[PathBuf]) -> Result<SupportSnapshot, WorkspaceError> {
        for path in paths {
            if path.as_os_str().is_empty()
                || path
                    .components()
                    .any(|part| !matches!(part, Component::Normal(_)))
            {
                return Err(WorkspaceError::InvalidChangedPath(path.clone()));
            }
        }
        let (policy, diagnostics) = self.source_support_rules();
        let mut snapshot = SupportSnapshot {
            next_offset: None,
            objects: Vec::new(),
            files: Vec::new(),
            diagnostics,
            suppliers: Vec::new(),
        };
        let mut read = BTreeMap::new();
        for path in paths {
            let result = self.classify_support_file(path, policy.as_deref(), &mut read);
            if let Some((objects, file)) = result {
                snapshot.objects.extend(objects);
                snapshot.files.push(file);
            } else {
                snapshot
                    .diagnostics
                    .push(format!("source_unavailable:{}", path.display()));
                snapshot.files.push(FileSupport {
                    path: path.clone(),
                    objects: Vec::new(),
                    read_only: true,
                    reason: FileReason::Unavailable,
                    mixed: false,
                    unknown: true,
                });
            }
        }
        snapshot
            .objects
            .sort_by(|left, right| left.object_id.cmp(&right.object_id));
        snapshot
            .objects
            .dedup_by(|left, right| left.object_id == right.object_id);
        Ok(snapshot)
    }

    /// Display and file checks share content-verified supplier rules and the same alias boundary.
    fn source_support_rules(&mut self) -> (Option<Arc<Support>>, Vec<String>) {
        let source = self.project().source().to_path_buf();
        let kind = self.project().configuration().project_type();
        if unaliased(&source, Path::new("Ext/ParentConfigurations.bin")) {
            self.support_rules.read(&source, kind)
        } else {
            (None, vec!["support_aliased".to_owned()])
        }
    }

    /// Predefined identities belong to their payload, while their metadata owner must remain declared.
    fn support_object_dependency(
        &mut self,
        id: &ObjectId,
        read: &mut BTreeMap<ObjectId, Arc<ParsedDescriptor>>,
    ) -> Option<Arc<ParsedDescriptor>> {
        if id.as_str().split('/').count() > 128 {
            return None;
        }
        if let Some(parsed) = read.get(id) {
            return Some(Arc::clone(parsed));
        }
        let (owner, paths) = self.source.descriptor_candidates(id).ok()?;
        if !paths.iter().any(|path| path.ends_with("Predefined.xml")) {
            return self.support_dependency(id, read);
        }
        if !self
            .support_dependency(&owner, read)?
            .diagnostics
            .is_empty()
        {
            return None;
        }
        let location = self.source.object_descriptor(id).ok()??;
        if !unaliased(self.project().source(), &location.path) {
            return None;
        }
        let input = self.source.read_xml(&location.path).ok()??;
        let parsed = Arc::new(
            metadata_parser::parse_predefined(&input, &owner, PropertiesMode::Summary).ok()?,
        );
        for object in &parsed.objects {
            read.insert(object.metadata.id().clone(), Arc::clone(&parsed));
        }
        Some(parsed)
    }

    /// Resolve shared XML and predefined items without inheriting inline-child locks into modules.
    fn classify_support_file(
        &mut self,
        path: &Path,
        policy: Option<&Support>,
        read: &mut BTreeMap<ObjectId, Arc<ParsedDescriptor>>,
    ) -> Option<(Vec<ObjectSupport>, FileSupport)> {
        if path == Path::new("Ext/ParentConfigurations.bin")
            || path.starts_with("Ext/ParentConfigurations")
        {
            return None;
        }
        // A lexical ownership match cannot authorize writes through a symlink alias.
        if !unaliased(self.project().source(), path) {
            return None;
        }
        let owner = self.source_owner(path)?;
        let parsed = self.support_dependency(&owner, read)?;
        let location = self.source.object_descriptor(&owner).ok()??;
        if path == location.path && !parsed.diagnostics.is_empty() {
            return None;
        }
        let mut relevant = if path == location.path {
            parsed.objects.clone()
        } else {
            parsed
                .objects
                .iter()
                .filter(|object| object.metadata.id() == &owner)
                .cloned()
                .collect()
        };
        if path
            .file_name()
            .is_some_and(|name| name == "Predefined.xml")
        {
            let input = self.source.read_xml(path).ok()??;
            let predefined =
                metadata_parser::parse_predefined(&input, &owner, PropertiesMode::Summary).ok()?;
            if !predefined.diagnostics.is_empty() {
                return None;
            }
            relevant.extend(predefined.objects);
        }
        if relevant.is_empty() {
            return None;
        }
        let objects: Vec<_> = relevant
            .into_iter()
            .map(|object| {
                let metadata = object.metadata;
                let (state, reason) = policy
                    .map_or((State::Unknown, Reason::Unavailable), |policy| {
                        policy.object(metadata.uuid())
                    });
                ObjectSupport {
                    object_id: metadata.id().clone(),
                    uuid: metadata.uuid().to_owned(),
                    state,
                    reason,
                }
            })
            .collect();
        let files = BTreeMap::from([(
            path.to_path_buf(),
            objects
                .iter()
                .map(|object| object.object_id.clone())
                .collect(),
        )]);
        let file = file_policies(files, &objects, &BTreeSet::new(), true).pop()?;
        Some((objects, file))
    }

    /// Verify declarations along the owner chain, reusing only content-validated parses.
    fn support_dependency(
        &mut self,
        id: &ObjectId,
        read: &mut BTreeMap<ObjectId, Arc<ParsedDescriptor>>,
    ) -> Option<Arc<ParsedDescriptor>> {
        if let Some(parsed) = read.get(id) {
            return Some(Arc::clone(parsed));
        }
        let root = self.source.root().id().clone();
        if id != &root {
            let parent = id.parent().unwrap_or(root);
            let ancestor = self.support_dependency(&parent, read)?;
            if ancestor
                .objects
                .iter()
                .any(|object| object.metadata.id() == id)
            {
                // Inline children share the already verified physical descriptor in this batch.
                read.insert(id.clone(), Arc::clone(&ancestor));
                return Some(ancestor);
            }
            if !ancestor
                .references
                .iter()
                .any(|reference| &reference.id == id)
            {
                return None;
            }
        }
        let location = self.source.object_descriptor(id).ok()??;
        if !unaliased(self.project().source(), &location.path) {
            return None;
        }
        let input = self.source.read_xml(&location.path).ok()??;
        let hash: [u8; 32] = Sha256::digest(input.as_bytes()).into();
        let key = (location.path, hash);
        let parsed = if let Some((parsed, _)) = self.support_descriptors.entries.get(&key) {
            Arc::clone(parsed)
        } else {
            let (owner, _) = self.source.descriptor_candidates(id).ok()?;
            let cache_key = format!("support-descriptor-1:{owner}:{hash:x?}");
            let parsed = self
                .source
                .disk_cache
                .as_ref()
                .and_then(|cache| cache.get(&cache_key, hash))
                .or_else(|| {
                    let parsed =
                        metadata_parser::parse(&input, owner.parent(), PropertiesMode::Summary)
                            .ok()?;
                    if let Some(cache) = &self.source.disk_cache {
                        cache.put(&cache_key, hash, &parsed);
                    }
                    Some(parsed)
                })?;
            let parsed = Arc::new(parsed);
            if self.support_descriptors.entries.len() >= 128
                || self.support_descriptors.bytes + input.len() > 8 * 1024 * 1024
            {
                // Each batch shares the root. Keep its current parse instead of decoding it again
                // whenever a full batch of children reaches the cache limit.
                let root = read.get(self.source.root().id());
                self.support_descriptors
                    .entries
                    .retain(|_, (parsed, _)| root.is_some_and(|root| Arc::ptr_eq(root, parsed)));
                self.support_descriptors.bytes = self
                    .support_descriptors
                    .entries
                    .values()
                    .map(|(_, bytes)| bytes)
                    .sum();
            }
            if input.len() <= 8 * 1024 * 1024 {
                if self.support_descriptors.bytes + input.len() > 8 * 1024 * 1024 {
                    self.support_descriptors.entries.clear();
                    self.support_descriptors.bytes = 0;
                }
                self.support_descriptors.bytes += input.len();
                self.support_descriptors
                    .entries
                    .insert(key, (Arc::clone(&parsed), input.len()));
            }
            parsed
        };
        if !parsed
            .objects
            .iter()
            .any(|object| object.metadata.id() == id)
        {
            return None;
        }
        read.insert(id.clone(), Arc::clone(&parsed));
        Some(parsed)
    }
}
