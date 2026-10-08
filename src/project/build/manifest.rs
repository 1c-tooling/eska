//! Repository inspection belongs to the invoking application, not the native builder.

use crate::{
    build_core::{BuildProvenance, GitProvenance},
    project::Project,
    vcs::repository::{Error as RepositoryError, Head, Repository},
};

/// Inspect Git before temporary build paths can change the reported dirty state.
pub(super) fn capture(project: &Project, project_name: Option<&str>) -> BuildProvenance {
    BuildProvenance::new(project_name, inspect_git(project.root()))
}

/// Keep absent and inaccessible repositories distinct without failing a native build.
fn inspect_git(root: &std::path::Path) -> GitProvenance {
    let repository = match Repository::discover(root) {
        Ok(repository) => repository,
        Err(RepositoryError::NotFound { .. }) => return GitProvenance::Absent,
        Err(_) => return GitProvenance::Unavailable,
    };
    let commit = match repository.head() {
        Ok(Head::Unborn { .. }) => None,
        Ok(head) => head.id().map(|id| id.to_string()),
        Err(_) => return GitProvenance::Unavailable,
    };
    let dirty = match repository.status() {
        Ok(status) => status.is_dirty(),
        Err(_) => return GitProvenance::Unavailable,
    };
    GitProvenance::Available { commit, dirty }
}
