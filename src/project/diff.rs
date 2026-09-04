//! Locale-independent file changes scoped to one project inside a Git worktree.

use std::path::{Path, PathBuf};

use gix::bstr::{BString, ByteSlice};

use super::Project;
use crate::vcs::{
    repository::{Error as RepositoryError, Repository},
    status::Change,
};

/// File-level project diff. Object-aware details can be added without changing the command.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectDiff {
    pub files: Vec<FileChange>,
}

/// One changed path relative to the project root, retaining both Git comparison stages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    pub path: BString,
    pub index: Option<Change>,
    pub worktree: Option<Change>,
}

/// Errors produced while reading a project diff.
#[derive(Debug)]
pub enum DiffError {
    Repository(RepositoryError),
    ProjectOutsideRepository {
        project: PathBuf,
        repository: PathBuf,
    },
}

/// Inspect current file changes without modifying the worktree, index or references.
///
/// # Errors
/// Returns a structured error when the repository cannot be read or does not contain the project.
pub fn inspect(project: &Project) -> Result<ProjectDiff, DiffError> {
    let repository = Repository::discover(project.root()).map_err(DiffError::Repository)?;
    if !project.root().starts_with(repository.work_dir()) {
        return Err(DiffError::ProjectOutsideRepository {
            project: project.root().to_owned(),
            repository: repository.work_dir().to_owned(),
        });
    }

    let status = repository.status().map_err(DiffError::Repository)?;
    let files = status
        .entries
        .into_iter()
        .filter_map(|entry| {
            project_relative_path(repository.work_dir(), project.root(), entry.path.as_bstr()).map(
                |path| FileChange {
                    path,
                    index: entry.index,
                    worktree: entry.worktree,
                },
            )
        })
        .collect();
    Ok(ProjectDiff { files })
}

/// Convert a repository-relative byte path into a project-relative byte path.
fn project_relative_path(
    work_dir: &Path,
    project_root: &Path,
    repository_path: &gix::bstr::BStr,
) -> Option<BString> {
    let absolute = work_dir.join(gix::path::from_bstr(repository_path));
    let relative = absolute.strip_prefix(project_root).ok()?;
    Some(gix::path::to_unix_separators_on_windows(gix::path::into_bstr(relative)).into_owned())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use gix::bstr::ByteSlice;

    use super::project_relative_path;

    /// Project scoping must remove an ancestor repository prefix without admitting sibling paths.
    #[test]
    fn relativizes_only_paths_inside_the_project() {
        let worktree = Path::new("/workspace");
        let project = Path::new("/workspace/components/app");

        assert_eq!(
            project_relative_path(
                worktree,
                project,
                b"components/app/src/module.bsl".as_bstr()
            ),
            Some("src/module.bsl".into())
        );
        assert_eq!(
            project_relative_path(worktree, project, b"components/other/file".as_bstr()),
            None
        );
    }
}
