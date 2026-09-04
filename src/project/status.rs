//! Locale-independent project status assembled from configuration and read-only Git data.

use std::path::{Path, PathBuf};

use gix::bstr::{BString, ByteSlice};

use super::{Project, ProjectType};
use crate::vcs::{
    repository::{Divergence, Error as RepositoryError, Head, Repository},
    status::{Change, PathStatus},
    workflow::{PolicyError, WorkflowPreset},
};

/// A point-in-time project status. Lock support remains explicit until T21 implements it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectStatus {
    pub root: PathBuf,
    pub project_type: ProjectType,
    pub workflow: WorkflowPreset,
    pub task: Option<String>,
    pub head: HeadState,
    pub branch: Option<BString>,
    pub base_branch: String,
    pub changes: ChangeSummary,
    pub synchronization: Option<Divergence>,
    pub locks: LockSummary,
    pub readiness: Readiness,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadState {
    Attached,
    Detached,
    Unborn,
}

/// Counts index and worktree states. A path changed in both may occur in two categories.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChangeSummary {
    pub files: usize,
    pub added: usize,
    pub modified: usize,
    pub deleted: usize,
    pub type_changed: usize,
    pub untracked: usize,
    pub intent_to_add: usize,
    pub conflicts: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LockSummary {
    pub available: bool,
    pub count: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Readiness {
    pub save: bool,
    pub publish: bool,
}

#[derive(Debug)]
pub enum StatusError {
    WorkflowNotConfigured,
    Policy(PolicyError),
    Repository(RepositoryError),
    ProjectOutsideRepository {
        project: PathBuf,
        repository: PathBuf,
    },
}

/// Inspect one discovered project without changing its files, index, refs or remotes.
///
/// # Errors
/// Returns a structured error when workflow policy or repository data cannot be read.
pub fn inspect(project: &Project) -> Result<ProjectStatus, StatusError> {
    let settings = project
        .configuration()
        .workflow_settings()
        .ok_or(StatusError::WorkflowNotConfigured)?;
    let policy = settings.resolve(None).map_err(StatusError::Policy)?;
    let repository = Repository::discover(project.root()).map_err(StatusError::Repository)?;
    if !project.root().starts_with(repository.work_dir()) {
        return Err(StatusError::ProjectOutsideRepository {
            project: project.root().to_owned(),
            repository: repository.work_dir().to_owned(),
        });
    }

    let head = repository.head().map_err(StatusError::Repository)?;
    let (head_state, branch) = head_state(&head);
    let task = branch
        .as_ref()
        .and_then(|name| name.to_str().ok())
        .and_then(|name| policy.task_id(name))
        .map(str::to_owned);
    let changes = repository.status().map_err(StatusError::Repository)?;
    let changes = summarize(
        changes
            .entries
            .iter()
            .filter(|entry| belongs_to_project(&repository, project.root(), entry)),
    );
    let synchronization = repository
        .divergence(&policy.remote_base_reference())
        .map_err(StatusError::Repository)?;
    let readiness = Readiness {
        save: changes.files > 0 && changes.conflicts == 0,
        publish: changes.files == 0
            && task.is_some()
            && synchronization.is_some_and(|state| state.behind == 0 && state.ahead > 0),
    };

    Ok(ProjectStatus {
        root: project.root().to_owned(),
        project_type: project.configuration().project_type(),
        workflow: settings.preset(),
        task,
        head: head_state,
        branch,
        base_branch: policy.base_branch().to_owned(),
        changes,
        synchronization,
        locks: LockSummary {
            available: false,
            count: None,
        },
        readiness,
    })
}

fn head_state(head: &Head) -> (HeadState, Option<BString>) {
    match head {
        Head::Attached { reference, .. } => (
            HeadState::Attached,
            Some(
                reference
                    .as_slice()
                    .strip_prefix(b"refs/heads/")
                    .unwrap_or(reference.as_slice())
                    .into(),
            ),
        ),
        Head::Detached { .. } => (HeadState::Detached, None),
        Head::Unborn { reference } => (
            HeadState::Unborn,
            Some(
                reference
                    .as_slice()
                    .strip_prefix(b"refs/heads/")
                    .unwrap_or(reference.as_slice())
                    .into(),
            ),
        ),
    }
}

fn belongs_to_project(repository: &Repository, root: &Path, entry: &PathStatus) -> bool {
    root == repository.work_dir()
        || repository
            .work_dir()
            .join(gix::path::from_bstr(entry.path.as_bstr()).as_ref())
            .starts_with(root)
}

fn summarize<'a>(entries: impl Iterator<Item = &'a PathStatus>) -> ChangeSummary {
    let mut summary = ChangeSummary::default();
    for entry in entries {
        summary.files += 1;
        for change in [entry.index, entry.worktree].into_iter().flatten() {
            match change {
                Change::Added => summary.added += 1,
                Change::Modified => summary.modified += 1,
                Change::Deleted => summary.deleted += 1,
                Change::TypeChanged => summary.type_changed += 1,
                Change::Untracked => summary.untracked += 1,
                Change::IntentToAdd => summary.intent_to_add += 1,
                Change::Conflict => summary.conflicts += 1,
            }
        }
    }
    summary
}
