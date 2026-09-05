//! Reusable semantic ownership pipeline over file-level project changes.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use gix::bstr::{BStr, BString, ByteSlice};

use super::{
    Project,
    diff::{ProjectDiff, RevisionProjectDiff},
    object_model::{LogicalObject, ObjectId, ObjectModel},
};
use crate::vcs::status::Change;

/// The comparison edge represented by one file change.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ChangeStage {
    /// HEAD to index in a workspace comparison.
    Index,
    /// Index to worktree in a workspace comparison.
    Worktree,
    /// One committed tree to another committed tree.
    Revision,
}

/// One normalized file-level change retaining its exact comparison edge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangedPath {
    path: BString,
    stage: ChangeStage,
    change: Change,
}

impl ChangedPath {
    /// Return the project-relative path without lossy UTF-8 conversion.
    #[must_use]
    pub fn path(&self) -> &BStr {
        self.path.as_bstr()
    }

    /// Return the comparison edge that produced this change.
    #[must_use]
    pub const fn stage(&self) -> ChangeStage {
        self.stage
    }

    /// Return the normalized file state reported by the repository layer.
    #[must_use]
    pub const fn change(&self) -> Change {
        self.change
    }
}

/// Deterministic set of file changes accepted by semantic analyzers.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ChangeSet {
    changes: Vec<ChangedPath>,
}

impl ChangeSet {
    /// Normalize both workspace comparison edges from a file-level project diff.
    #[must_use]
    pub fn from_workspace(diff: &ProjectDiff) -> Self {
        let changes = diff.files.iter().flat_map(|file| {
            [
                file.index.map(|change| ChangedPath {
                    path: file.path.clone(),
                    stage: ChangeStage::Index,
                    change,
                }),
                file.worktree.map(|change| ChangedPath {
                    path: file.path.clone(),
                    stage: ChangeStage::Worktree,
                    change,
                }),
            ]
            .into_iter()
            .flatten()
        });
        Self::normalized(changes)
    }

    /// Normalize one committed-tree comparison from a revision project diff.
    #[must_use]
    pub fn from_revision(diff: &RevisionProjectDiff) -> Self {
        Self::normalized(diff.files.iter().map(|file| ChangedPath {
            path: file.path.clone(),
            stage: ChangeStage::Revision,
            change: file.change,
        }))
    }

    /// Return normalized changes sorted by path and comparison stage.
    #[must_use]
    pub fn changes(&self) -> &[ChangedPath] {
        &self.changes
    }

    /// Return whether the set contains no file changes.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// Sort changes and conservatively merge duplicate path/stage entries.
    fn normalized(changes: impl IntoIterator<Item = ChangedPath>) -> Self {
        let mut normalized = BTreeMap::new();
        for change in changes {
            normalized
                .entry((change.path, change.stage))
                .and_modify(|current| *current = merge_change(*current, change.change))
                .or_insert(change.change);
        }
        Self {
            changes: normalized
                .into_iter()
                .map(|((path, stage), change)| ChangedPath {
                    path,
                    stage,
                    change,
                })
                .collect(),
        }
    }
}

/// Semantic role of one changed path relative to an affected logical object.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ObjectPathRole {
    Descriptor,
    Module,
    Form,
    Artifact,
}

/// One file change attributed to one logical metadata object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectPathChange {
    path: BString,
    stage: ChangeStage,
    change: Change,
    role: ObjectPathRole,
}

impl ObjectPathChange {
    /// Return the original project-relative path.
    #[must_use]
    pub fn path(&self) -> &BStr {
        self.path.as_bstr()
    }

    /// Return the comparison edge of the attributed change.
    #[must_use]
    pub const fn stage(&self) -> ChangeStage {
        self.stage
    }

    /// Return the normalized file state.
    #[must_use]
    pub const fn change(&self) -> Change {
        self.change
    }

    /// Return how the path participates in the logical object.
    #[must_use]
    pub const fn role(&self) -> ObjectPathRole {
        self.role
    }
}

/// All file changes attributed to one stable logical object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectChangeSummary {
    id: ObjectId,
    metadata_type: &'static str,
    name: String,
    changes: Vec<ObjectPathChange>,
}

impl ObjectChangeSummary {
    /// Return the stable logical object identifier.
    #[must_use]
    pub const fn id(&self) -> &ObjectId {
        &self.id
    }

    /// Return the stable machine-facing metadata type.
    #[must_use]
    pub const fn metadata_type(&self) -> &'static str {
        self.metadata_type
    }

    /// Return the metadata name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return all changes attributed to this object in deterministic order.
    #[must_use]
    pub fn changes(&self) -> &[ObjectPathChange] {
        &self.changes
    }
}

/// Aggregate counts over normalized comparison edges.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ChangeCounts {
    pub added: usize,
    pub modified: usize,
    pub deleted: usize,
    pub type_changed: usize,
    pub untracked: usize,
    pub intent_to_add: usize,
    pub conflicts: usize,
}

impl ChangeCounts {
    /// Record one normalized repository change.
    const fn record(&mut self, change: Change) {
        match change {
            Change::Added => self.added += 1,
            Change::Modified => self.modified += 1,
            Change::Deleted => self.deleted += 1,
            Change::TypeChanged => self.type_changed += 1,
            Change::Untracked => self.untracked += 1,
            Change::IntentToAdd => self.intent_to_add += 1,
            Change::Conflict => self.conflicts += 1,
        }
    }
}

/// Deterministic semantic ownership projection of one `ChangeSet`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ChangeSummary {
    files: usize,
    counts: ChangeCounts,
    objects: Vec<ObjectChangeSummary>,
    unowned: Vec<ChangedPath>,
}

impl ChangeSummary {
    /// Return the number of unique project-relative paths.
    #[must_use]
    pub const fn files(&self) -> usize {
        self.files
    }

    /// Return counts over comparison edges, preserving staged/unstaged separation.
    #[must_use]
    pub const fn counts(&self) -> ChangeCounts {
        self.counts
    }

    /// Return affected logical objects in stable `ObjectId` order.
    #[must_use]
    pub fn objects(&self) -> &[ObjectChangeSummary] {
        &self.objects
    }

    /// Return changes outside the source tree or without a discovered owner.
    #[must_use]
    pub fn unowned_changes(&self) -> &[ChangedPath] {
        &self.unowned
    }
}

/// Stateless analyzer that projects changed paths through one Designer object model.
#[derive(Debug)]
pub struct SemanticChangeAnalyzer<'a> {
    project: &'a Project,
    objects: &'a ObjectModel,
}

impl<'a> SemanticChangeAnalyzer<'a> {
    /// Bind the analyzer to matching project and object-model snapshots.
    #[must_use]
    pub const fn new(project: &'a Project, objects: &'a ObjectModel) -> Self {
        Self { project, objects }
    }

    /// Attribute file-level changes to logical objects without parsing file contents.
    #[must_use]
    pub fn analyze(&self, changes: &ChangeSet) -> ChangeSummary {
        let mut files = BTreeSet::new();
        let mut counts = ChangeCounts::default();
        let mut objects: BTreeMap<ObjectId, ObjectChangeSummary> = BTreeMap::new();
        let mut unowned = Vec::new();

        for change in changes.changes() {
            files.insert(change.path.clone());
            counts.record(change.change);
            let Some(source_path) = self.source_relative_path(change.path()) else {
                unowned.push(change.clone());
                continue;
            };
            let owners = self.objects.objects_for_changed_path(&source_path);
            if owners.is_empty() {
                unowned.push(change.clone());
                continue;
            }
            for owner in owners {
                let value =
                    objects
                        .entry(owner.id().clone())
                        .or_insert_with(|| ObjectChangeSummary {
                            id: owner.id().clone(),
                            metadata_type: owner.metadata_type(),
                            name: owner.name().to_owned(),
                            changes: Vec::new(),
                        });
                value.changes.push(ObjectPathChange {
                    path: change.path.clone(),
                    stage: change.stage,
                    change: change.change,
                    role: object_path_role(owner, &source_path),
                });
            }
        }

        ChangeSummary {
            files: files.len(),
            counts,
            objects: objects.into_values().collect(),
            unowned,
        }
    }

    /// Convert a project-relative byte path into a source-relative platform path.
    fn source_relative_path(&self, project_path: &BStr) -> Option<PathBuf> {
        let absolute = self.project.root().join(gix::path::from_bstr(project_path));
        absolute
            .strip_prefix(self.project.source())
            .ok()
            .map(Path::to_path_buf)
    }
}

/// Classify one changed path relative to its resolved logical owner.
fn object_path_role(owner: &LogicalObject, source_path: &Path) -> ObjectPathRole {
    if owner.module_paths().any(|path| path == source_path) {
        ObjectPathRole::Module
    } else if owner.metadata_type() == "form" {
        ObjectPathRole::Form
    } else if owner.descriptor_path() == source_path {
        ObjectPathRole::Descriptor
    } else {
        ObjectPathRole::Artifact
    }
}

/// Conservatively combine duplicate states without losing conflicts.
fn merge_change(current: Change, incoming: Change) -> Change {
    if current == incoming {
        current
    } else if current == Change::Conflict || incoming == Change::Conflict {
        Change::Conflict
    } else {
        Change::Modified
    }
}

#[cfg(test)]
mod tests {
    use gix::bstr::ByteSlice;

    use super::{ChangeSet, ChangeStage};
    use crate::{
        project::diff::{FileChange, ProjectDiff},
        vcs::status::Change,
    };

    /// Workspace conversion retains both comparison edges and deterministic ordering.
    #[test]
    fn normalizes_workspace_edges() {
        let diff = ProjectDiff {
            files: vec![
                FileChange {
                    path: b"src/B.bsl".as_bstr().to_owned(),
                    index: None,
                    worktree: Some(Change::Untracked),
                },
                FileChange {
                    path: b"src/A.bsl".as_bstr().to_owned(),
                    index: Some(Change::Modified),
                    worktree: Some(Change::Modified),
                },
            ],
            display: Vec::new(),
        };

        let changes = ChangeSet::from_workspace(&diff);

        assert_eq!(changes.changes().len(), 3);
        assert_eq!(changes.changes()[0].path(), b"src/A.bsl".as_bstr());
        assert_eq!(changes.changes()[0].stage(), ChangeStage::Index);
        assert_eq!(changes.changes()[1].stage(), ChangeStage::Worktree);
        assert_eq!(changes.changes()[2].path(), b"src/B.bsl".as_bstr());
    }
}
