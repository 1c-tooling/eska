//! File permissions are checked from their current dependencies without a project scan.

use super::{
    FileReason, FileSupport, ObjectSupport, ProjectSession, SupportSnapshot, file_policies,
};
use crate::project::{
    metadata_model::{MetadataKind, ObjectId},
    metadata_parser::{self, ParsedDescriptor, PropertiesMode},
    metadata_workspace::WorkspaceError,
    object_model::logical_path_for_source,
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
    entries: BTreeMap<(PathBuf, [u8; 32]), Arc<ParsedDescriptor>>,
    bytes: usize,
}

impl ProjectSession {
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
        let source = self.project().source().to_path_buf();
        let (policy, diagnostics) = if unaliased(&source, Path::new("Ext/ParentConfigurations.bin"))
        {
            self.support_rules.read(&source)
        } else {
            (None, vec!["support_aliased".to_owned()])
        };
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

    /// Translate an existing Designer path through the shared ownership vocabulary.
    fn support_owner(&self, path: &Path) -> Option<ObjectId> {
        let logical = logical_path_for_source(self.project().configuration().project_type(), path)?;
        let mut owner = None;
        for part in logical.parts {
            let Some(kind) = MetadataKind::ALL
                .iter()
                .find(|kind| kind.as_str() == part.kind)
            else {
                break;
            };
            let name = part
                .name
                .as_deref()
                .unwrap_or_else(|| self.source.root().name());
            owner = Some(ObjectId::from_parts(owner.as_ref(), kind.as_str(), name));
        }
        owner
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
        let owner = self.support_owner(path)?;
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
            if !ancestor
                .references
                .iter()
                .any(|reference| &reference.id == id)
                && !ancestor
                    .objects
                    .iter()
                    .any(|object| object.metadata.id() == id)
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
        let parsed = if let Some(parsed) = self.support_descriptors.entries.get(&key) {
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
                self.support_descriptors.entries.clear();
                self.support_descriptors.bytes = 0;
            }
            if input.len() <= 8 * 1024 * 1024 {
                self.support_descriptors.bytes += input.len();
                self.support_descriptors
                    .entries
                    .insert(key, Arc::clone(&parsed));
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
