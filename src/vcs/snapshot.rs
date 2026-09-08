//! Operation-scoped Git snapshots for reading multiple changed files.

use std::{fs, io};

use gix::bstr::BStr;

use super::{
    repository::{Error, Operation, Repository},
    status::FileVersions,
};

/// Retain HEAD and index for one analysis; worktree files remain live reads.
pub struct FileVersionReader<'repo> {
    repository: &'repo Repository,
    head: Option<gix::Tree<'repo>>,
    index: gix::worktree::Index,
}

impl Repository {
    /// Read HEAD, index and regular worktree file contents for one repository-relative path.
    ///
    /// # Errors
    /// Returns a repository error for unreadable Git data or worktree files.
    pub fn file_versions(&self, path: &BStr) -> Result<FileVersions, Error> {
        self.file_version_reader()?.read(path)
    }

    /// Prepare one read-only HEAD/index snapshot without retaining it across operations.
    pub(crate) fn file_version_reader(&self) -> Result<FileVersionReader<'_>, Error> {
        self.check_index_size()?;
        let head = self
            .head()?
            .id()
            .map(|id| {
                self.inner
                    .find_commit(id)
                    .map_err(|source| Error::operation(Operation::Status, source))?
                    .tree()
                    .map_err(|source| Error::operation(Operation::Status, source))
            })
            .transpose()?;
        let index = self
            .inner
            .index_or_empty()
            .map_err(|source| Error::operation(Operation::Status, source))?;
        Ok(FileVersionReader {
            repository: self,
            head,
            index,
        })
    }
}

impl FileVersionReader<'_> {
    /// Read one file using the retained Git snapshots and its current worktree contents.
    pub(crate) fn read(&self, path: &BStr) -> Result<FileVersions, Error> {
        let head = self.head_blob(path)?;
        let index = self.index_blob(path)?;
        let worktree_path = self.repository.work_dir().join(gix::path::from_bstr(path));
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

    /// Read a blob from the retained HEAD tree; unborn HEAD has no tree.
    fn head_blob(&self, path: &BStr) -> Result<Option<Vec<u8>>, Error> {
        let Some(tree) = &self.head else {
            return Ok(None);
        };
        let relative_path = gix::path::from_bstr(path);
        let Some(entry) = tree
            .lookup_entry_by_path(relative_path.as_ref())
            .map_err(|source| Error::operation(Operation::Status, source))?
        else {
            return Ok(None);
        };
        let mut blob = self
            .repository
            .inner
            .find_blob(entry.object_id())
            .map_err(|source| Error::operation(Operation::Status, source))?;
        Ok(Some(std::mem::take(&mut blob.data)))
    }

    /// Read a stage-zero or ours blob from the retained index.
    fn index_blob(&self, path: &BStr) -> Result<Option<Vec<u8>>, Error> {
        let Some(entry) = self.index.entry_by_path(path) else {
            return Ok(None);
        };
        let mut blob = self
            .repository
            .inner
            .find_blob(entry.id)
            .map_err(|source| Error::operation(Operation::Status, source))?;
        Ok(Some(std::mem::take(&mut blob.data)))
    }
}
