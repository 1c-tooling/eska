//! Read-only resolution and tree comparison for Git revisions.

use std::{convert::Infallible, ops::ControlFlow};

use gix::{ObjectId, bstr::BString};

use super::{
    repository::{Error, Operation, Repository},
    status::Change,
};

/// One revision resolved and peeled to a commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedCommit {
    pub id: ObjectId,
}

/// One changed tree path with blob IDs retained for semantic metadata comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeChange {
    pub path: BString,
    pub change: Change,
    pub before: Option<ObjectId>,
    pub after: Option<ObjectId>,
}

impl Repository {
    /// Resolve a branch, tag, commit ID or revision expression and peel it to a commit.
    ///
    /// # Errors
    /// Returns `Operation::Revision` when the revision is missing, ambiguous or not commit-like.
    pub fn resolve_commit(&self, revision: &str) -> Result<ResolvedCommit, Error> {
        let id = self
            .inner
            .rev_parse_single(revision.as_bytes())
            .map_err(|source| Error::operation(Operation::Revision, source))?;
        let object = id
            .object()
            .map_err(|source| Error::operation(Operation::Revision, source))?;
        let commit = object
            .peel_to_commit()
            .map_err(|source| Error::operation(Operation::Revision, source))?;
        Ok(ResolvedCommit { id: commit.id })
    }

    /// Find the best merge base of two resolved commits.
    ///
    /// # Errors
    /// Returns `Operation::MergeBase` when history cannot be read or no merge base exists.
    pub fn merge_base_commit(
        &self,
        left: ResolvedCommit,
        right: ResolvedCommit,
    ) -> Result<ResolvedCommit, Error> {
        let id = self
            .inner
            .merge_base(left.id, right.id)
            .map_err(|source| Error::operation(Operation::MergeBase, source))?;
        Ok(ResolvedCommit { id: id.detach() })
    }

    /// Compare two commit trees without rename detection or worktree access.
    ///
    /// # Errors
    /// Returns `Operation::TreeDiff` when commits, trees or diff data cannot be read.
    pub fn diff_commits(
        &self,
        from: ResolvedCommit,
        to: ResolvedCommit,
    ) -> Result<Vec<TreeChange>, Error> {
        let old_commit = self
            .inner
            .find_commit(from.id)
            .map_err(|source| Error::operation(Operation::TreeDiff, source))?;
        let old_tree = old_commit
            .tree()
            .map_err(|source| Error::operation(Operation::TreeDiff, source))?;
        let new_commit = self
            .inner
            .find_commit(to.id)
            .map_err(|source| Error::operation(Operation::TreeDiff, source))?;
        let new_tree = new_commit
            .tree()
            .map_err(|source| Error::operation(Operation::TreeDiff, source))?;
        let mut platform = old_tree
            .changes()
            .map_err(|source| Error::operation(Operation::TreeDiff, source))?;
        platform.options(|options| {
            options.track_rewrites(None);
        });
        let mut changes = Vec::new();
        platform
            .for_each_to_obtain_tree(&new_tree, |change| {
                record_tree_change(&mut changes, change);
                Ok::<_, Infallible>(ControlFlow::Continue(()))
            })
            .map_err(|source| Error::operation(Operation::TreeDiff, source))?;
        changes.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(changes)
    }

    /// Read one blob by object ID for semantic projection of a changed descriptor.
    ///
    /// # Errors
    /// Returns `Operation::TreeDiff` when the object is missing or is not a blob.
    pub fn blob(&self, id: ObjectId) -> Result<Vec<u8>, Error> {
        self.inner
            .find_blob(id)
            .map(|mut blob| std::mem::take(&mut blob.data))
            .map_err(|source| Error::operation(Operation::TreeDiff, source))
    }
}

/// Convert one gix tree change into the stable eska file-state model.
fn record_tree_change(
    output: &mut Vec<TreeChange>,
    change: gix::object::tree::diff::Change<'_, '_, '_>,
) {
    use gix::object::tree::diff::Change as GitChange;

    match change {
        GitChange::Addition {
            location,
            entry_mode,
            id,
            ..
        } if !entry_mode.is_tree() => output.push(TreeChange {
            path: location.to_owned(),
            change: Change::Added,
            before: None,
            after: Some(id.detach()),
        }),
        GitChange::Deletion {
            location,
            entry_mode,
            id,
            ..
        } if !entry_mode.is_tree() => output.push(TreeChange {
            path: location.to_owned(),
            change: Change::Deleted,
            before: Some(id.detach()),
            after: None,
        }),
        GitChange::Modification {
            location,
            previous_entry_mode,
            previous_id,
            entry_mode,
            id,
        } if !entry_mode.is_tree() && !previous_entry_mode.is_tree() => output.push(TreeChange {
            path: location.to_owned(),
            change: if entry_type(previous_entry_mode) == entry_type(entry_mode) {
                Change::Modified
            } else {
                Change::TypeChanged
            },
            before: Some(previous_id.detach()),
            after: Some(id.detach()),
        }),
        GitChange::Rewrite {
            source_location,
            source_entry_mode,
            source_id,
            entry_mode,
            location,
            id,
            copy,
            ..
        } => {
            if !copy && !source_entry_mode.is_tree() {
                output.push(TreeChange {
                    path: source_location.to_owned(),
                    change: Change::Deleted,
                    before: Some(source_id.detach()),
                    after: None,
                });
            }
            if !entry_mode.is_tree() {
                output.push(TreeChange {
                    path: location.to_owned(),
                    change: Change::Added,
                    before: None,
                    after: Some(id.detach()),
                });
            }
        }
        GitChange::Addition { .. }
        | GitChange::Deletion { .. }
        | GitChange::Modification { .. } => {}
    }
}

/// Compare only the Git entry type, treating executable-bit changes as modifications.
const fn entry_type(mode: gix::object::tree::EntryMode) -> u16 {
    mode.value() & 0o170_000
}
