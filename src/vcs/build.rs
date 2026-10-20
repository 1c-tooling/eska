//! VCS orchestration supplies provenance and source-change checks to the independent build runtime.

use std::path::Path;

use gix::bstr::ByteSlice;

use super::repository::{Error as RepositoryError, Head, Repository};
use crate::project::{Project, artifact::ArtifactError, build::GitProvenance};

/// Capture repository metadata before temporary build paths can affect its dirty state.
pub fn provenance(root: &Path) -> GitProvenance {
    let repository = match Repository::discover(root) {
        Ok(repository) => repository,
        Err(RepositoryError::NotFound { .. }) => return GitProvenance::absent(),
        Err(_) => return GitProvenance::unavailable(),
    };
    let commit = match repository.head() {
        Ok(Head::Unborn { .. }) => None,
        Ok(head) => head.id().map(|id| id.to_string()),
        Err(_) => return GitProvenance::unavailable(),
    };
    repository.status().map_or_else(
        |_| GitProvenance::unavailable(),
        |status| GitProvenance::available(commit, status.is_dirty()),
    )
}

/// Count only changed paths inside the selected source and preserve Git-less confirmation rules.
pub fn source_changes(
    project: &Project,
    empty_scaffold: bool,
    files: usize,
) -> Result<usize, ArtifactError> {
    let repository = match Repository::discover(project.root()) {
        Ok(repository) => repository,
        Err(RepositoryError::NotFound { .. }) => return Ok(if empty_scaffold { 0 } else { files }),
        Err(_) => return Err(ArtifactError::ChangeInspection),
    };
    let status = repository
        .status()
        .map_err(|_| ArtifactError::ChangeInspection)?;
    Ok(status
        .entries
        .iter()
        .filter(|entry| {
            repository
                .work_dir()
                .join(gix::path::from_bstr(entry.path.as_bstr()).as_ref())
                .starts_with(project.source())
                && !(empty_scaffold && entry.path.ends_with(b"/.gitkeep"))
        })
        .count())
}
