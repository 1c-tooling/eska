//! File status relative to the repository root, without parsing Designer XML.

use std::{collections::BTreeMap, fs, io};

use gix::bstr::{BStr, BString};

use super::repository::{Error, Operation, Repository};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Added,
    Modified,
    Deleted,
    TypeChanged,
    Untracked,
    IntentToAdd,
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathStatus {
    /// Git-relative byte path with `/` separators, without lossy UTF-8 conversion.
    pub path: BString,
    /// HEAD-to-index change. Unmerged entries are always `Conflict` here.
    pub index: Option<Change>,
    /// Index-to-worktree change, including untracked and intent-to-add entries.
    pub worktree: Option<Change>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Status {
    /// One entry per changed path, sorted by byte path. Ignored files are excluded.
    pub entries: Vec<PathStatus>,
}

/// Blob snapshots used to refine a changed metadata XML file without invoking Git.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileVersions {
    pub head: Option<Vec<u8>>,
    pub index: Option<Vec<u8>>,
    pub worktree: Option<Vec<u8>>,
}

impl Status {
    #[must_use]
    pub const fn is_dirty(&self) -> bool {
        !self.entries.is_empty()
    }

    /// All changed paths, including both endpoints of moves, without scanning again.
    pub fn changed_paths(&self) -> impl Iterator<Item = &BStr> {
        self.entries.iter().map(|entry| entry.path.as_ref())
    }
}

impl Repository {
    /// Compare HEAD, index and worktree; list individual untracked files across the repository.
    /// Renames are represented as deletion/addition pairs, avoiding similarity scans.
    /// Stat refresh suggestions are discarded, so this operation never writes the index.
    /// Repository-local ignore rules and attributes are handled by `gix`.
    ///
    /// # Errors
    /// Returns an index preflight error or `Operation::Status` for unreadable repository data.
    pub fn status(&self) -> Result<Status, Error> {
        self.check_index_size()?;
        let platform = self
            .inner
            .status(gix::progress::Discard)
            .map_err(|source| Error::operation(Operation::Status, source))?
            .untracked_files(gix::status::UntrackedFiles::Files)
            .tree_index_track_renames(gix::status::tree_index::TrackRenames::Disabled)
            .index_worktree_rewrites(None)
            .dirwalk_options(|options| options.empty_patterns_match_prefix(false));
        let iter = platform
            .into_iter(Vec::<BString>::new())
            .map_err(|source| Error::operation(Operation::Status, source))?;
        let mut paths = BTreeMap::new();
        for item in iter {
            match item.map_err(|source| Error::operation(Operation::Status, source))? {
                gix::status::Item::TreeIndex(change) => record_index(&mut paths, &change),
                gix::status::Item::IndexWorktree(change) => record_worktree(&mut paths, &change),
            }
        }
        Ok(Status {
            entries: paths.into_values().collect(),
        })
    }

    /// Read HEAD, index and regular worktree file contents for one repository-relative path.
    ///
    /// # Errors
    /// Returns `Operation::Status` when a Git object or index cannot be read, and `Error::Io`
    /// when the worktree path cannot be inspected or read.
    pub fn file_versions(&self, path: &BStr) -> Result<FileVersions, Error> {
        self.check_index_size()?;
        let head = self.head_blob(path)?;
        let index = self.index_blob(path)?;
        let worktree_path = self.work_dir().join(gix::path::from_bstr(path));
        let worktree = match fs::symlink_metadata(&worktree_path) {
            Ok(metadata) if metadata.file_type().is_file() => {
                Some(fs::read(&worktree_path).map_err(|source| Error::Io {
                    path: worktree_path.clone(),
                    source,
                })?)
            }
            Ok(_) => None,
            Err(source) if source.kind() == io::ErrorKind::NotFound => None,
            Err(source) => {
                return Err(Error::Io {
                    path: worktree_path,
                    source,
                });
            }
        };
        Ok(FileVersions {
            head,
            index,
            worktree,
        })
    }

    /// Read a blob at HEAD, treating an unborn HEAD or absent path as no content.
    fn head_blob(&self, path: &BStr) -> Result<Option<Vec<u8>>, Error> {
        let Some(id) = self.head()?.id() else {
            return Ok(None);
        };
        let commit = self
            .inner
            .find_commit(id)
            .map_err(|source| Error::operation(Operation::Status, source))?;
        let tree = commit
            .tree()
            .map_err(|source| Error::operation(Operation::Status, source))?;
        let relative_path = gix::path::from_bstr(path);
        let Some(entry) = tree
            .lookup_entry_by_path(relative_path.as_ref())
            .map_err(|source| Error::operation(Operation::Status, source))?
        else {
            return Ok(None);
        };
        let mut blob = self
            .inner
            .find_blob(entry.object_id())
            .map_err(|source| Error::operation(Operation::Status, source))?;
        Ok(Some(std::mem::take(&mut blob.data)))
    }

    /// Read a stage-zero or ours blob from the index, treating an absent path as no content.
    fn index_blob(&self, path: &BStr) -> Result<Option<Vec<u8>>, Error> {
        let index = self
            .inner
            .index_or_empty()
            .map_err(|source| Error::operation(Operation::Status, source))?;
        let Some(entry) = index.entry_by_path(path) else {
            return Ok(None);
        };
        let mut blob = self
            .inner
            .find_blob(entry.id)
            .map_err(|source| Error::operation(Operation::Status, source))?;
        Ok(Some(std::mem::take(&mut blob.data)))
    }

    fn check_index_size(&self) -> Result<(), Error> {
        let path = self.inner.index_path();
        match fs::metadata(&path) {
            // gix-index 0.48 subtracts the checksum length before validating the header.
            // Reject truncated files before that code can panic. Normal Git updates use rename;
            // concurrent in-place corruption is outside the snapshot guarantees of gix.
            Ok(metadata)
                if metadata.len() < 12 + self.inner.object_hash().len_in_bytes() as u64 =>
            {
                Err(Error::InvalidIndex { path })
            }
            Ok(_) => Ok(()),
            Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(Error::Io { path, source }),
        }
    }
}

fn entry<'a>(paths: &'a mut BTreeMap<BString, PathStatus>, path: &BStr) -> &'a mut PathStatus {
    paths.entry(path.to_owned()).or_insert_with(|| PathStatus {
        path: path.to_owned(),
        index: None,
        worktree: None,
    })
}

fn record_index(paths: &mut BTreeMap<BString, PathStatus>, change: &gix::diff::index::Change) {
    use gix::diff::index::Change as GitChange;
    let kind = match change {
        GitChange::Addition { .. } => Change::Added,
        GitChange::Deletion { .. } => Change::Deleted,
        GitChange::Modification {
            previous_entry_mode,
            entry_mode,
            ..
        } => {
            // The executable bit is a modification, not a file type change.
            if (*previous_entry_mode == gix::index::entry::Mode::SYMLINK)
                != (*entry_mode == gix::index::entry::Mode::SYMLINK)
                || previous_entry_mode.is_submodule() != entry_mode.is_submodule()
            {
                Change::TypeChanged
            } else {
                Change::Modified
            }
        }
        GitChange::Rewrite {
            source_location,
            copy,
            ..
        } => {
            if !copy {
                entry(paths, source_location).index = Some(Change::Deleted);
            }
            Change::Added
        }
    };
    let value = entry(paths, change.location());
    if value.index != Some(Change::Conflict) {
        value.index = Some(kind);
    }
}

fn record_worktree(
    paths: &mut BTreeMap<BString, PathStatus>,
    change: &gix::status::index_worktree::Item,
) {
    use gix::status::index_worktree::{Item, iter::Summary};
    let Some(summary) = change.summary() else {
        return;
    };
    if let Item::Rewrite {
        source,
        copy: false,
        ..
    } = change
    {
        entry(paths, source.rela_path()).worktree = Some(Change::Deleted);
    }
    let value = entry(paths, change.rela_path());
    let kind = match summary {
        Summary::Added | Summary::Renamed | Summary::Copied => Change::Untracked,
        Summary::Removed => Change::Deleted,
        Summary::Modified => Change::Modified,
        Summary::TypeChange => Change::TypeChanged,
        Summary::IntentToAdd => Change::IntentToAdd,
        Summary::Conflict => {
            value.index = Some(Change::Conflict);
            return;
        }
    };
    value.worktree = Some(kind);
}
