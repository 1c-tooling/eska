//! Repository-wide task switching with branch-bound capture and restoration.

use super::{Project, discovery::DiscoveryContext};
use crate::vcs::{
    command::{Error as CommandError, Executor},
    repository::{Error as RepositoryError, ReferenceTarget, Repository},
    shelves::{self, Session, Shelf},
    workflow::{PolicyError, WorkflowSettings},
};
use gix::bstr::ByteSlice;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchTarget<'a> {
    Task(&'a str),
    Base,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchResult {
    pub task: Option<String>,
    pub branch: String,
    pub saved: Option<Shelf>,
    pub restored: Option<Shelf>,
    pub dry_run: bool,
}

#[derive(Debug)]
pub enum SwitchError {
    WorkflowNotConfigured,
    Policy(PolicyError),
    Repository(RepositoryError),
    ProjectOutsideRepository {
        project: PathBuf,
        repository: PathBuf,
    },
    TaskBranchMissing {
        branch: String,
    },
    BaseBranchMissing {
        branch: String,
    },
    TargetCheckedOut {
        branch: String,
    },
    Shelf(shelves::Error),
    Command(CommandError),
}

/// Activate an existing task/base branch while preserving both branches' pending changes.
///
/// # Errors
/// Rejects invalid policy, conflicting shelves and unsafe Git states before switching.
pub fn execute(project: &Project, target: SwitchTarget<'_>) -> Result<SwitchResult, SwitchError> {
    execute_scope(
        project.root(),
        project.configuration().workflow_settings(),
        target,
        false,
    )
}

/// Execute or preview a repository-wide switch using the validated workspace policy.
///
/// # Errors
/// Returns structured policy, shelf and repository errors without locale-dependent text.
pub fn execute_context(
    context: &DiscoveryContext,
    target: SwitchTarget<'_>,
    dry_run: bool,
) -> Result<SwitchResult, SwitchError> {
    let (root, settings) = match context {
        DiscoveryContext::Standalone(project) => {
            (project.root(), project.configuration().workflow_settings())
        }
        DiscoveryContext::Workspace { workspace, .. } => {
            (workspace.root(), workspace.workflow_settings())
        }
    };
    execute_scope(root, settings, target, dry_run)
}

/// Resolve the requested branch before acquiring a mutation lock or creating a shelf.
fn execute_scope(
    root: &Path,
    settings: Option<&WorkflowSettings>,
    target: SwitchTarget<'_>,
    dry_run: bool,
) -> Result<SwitchResult, SwitchError> {
    let policy = settings
        .ok_or(SwitchError::WorkflowNotConfigured)?
        .resolve(None)
        .map_err(SwitchError::Policy)?;
    let (task, branch) = match target {
        SwitchTarget::Task(task) => (
            Some(task.to_owned()),
            policy
                .plan(task)
                .map_err(SwitchError::Policy)?
                .working_branch,
        ),
        SwitchTarget::Base => (None, policy.base_branch().to_owned()),
    };
    let repository = Repository::discover(root).map_err(SwitchError::Repository)?;
    if !root.starts_with(repository.work_dir()) {
        return Err(SwitchError::ProjectOutsideRepository {
            project: root.to_owned(),
            repository: repository.work_dir().to_owned(),
        });
    }
    let session = if dry_run {
        None
    } else {
        Some(Session::acquire(&repository).map_err(SwitchError::Shelf)?)
    };
    let mut result = prepare(&repository, task, branch, dry_run)?;
    if let Some(session) = session {
        apply(&repository, &session, &mut result)?;
    }
    Ok(result)
}

/// Preview the complete branch transition, including the current and target shelves.
fn prepare(
    repository: &Repository,
    task: Option<String>,
    branch: String,
    dry_run: bool,
) -> Result<SwitchResult, SwitchError> {
    let (current, _) = shelves::preflight(repository).map_err(SwitchError::Shelf)?;
    let target_ref = format!("refs/heads/{branch}");
    let target_id = repository
        .references()
        .map_err(SwitchError::Repository)?
        .into_iter()
        .find_map(|candidate| {
            if candidate.name.as_bstr() == target_ref.as_bytes()
                && let ReferenceTarget::Object(id) = candidate.target
            {
                return Some(id);
            }
            None
        })
        .ok_or_else(|| {
            if task.is_some() {
                SwitchError::TaskBranchMissing {
                    branch: branch.clone(),
                }
            } else {
                SwitchError::BaseBranchMissing {
                    branch: branch.clone(),
                }
            }
        })?;
    let same_branch = current == target_ref.as_bytes();
    if !same_branch
        && repository
            .reference_is_checked_out(&target_ref)
            .map_err(SwitchError::Repository)?
    {
        return Err(SwitchError::TargetCheckedOut { branch });
    }
    let restored = shelves::list(repository)
        .map_err(SwitchError::Shelf)?
        .into_iter()
        .find(|saved| saved.branch == target_ref.as_bytes());
    if let Some(saved) = &restored {
        if saved.base != target_id {
            return Err(SwitchError::Shelf(shelves::Error::MovedBranch));
        }
        shelves::validate_saved(repository, &saved.id).map_err(SwitchError::Shelf)?;
        if same_branch {
            shelves::restore_plan(repository, Some(&saved.id)).map_err(SwitchError::Shelf)?;
        }
    }
    let saved = if !same_branch
        && repository
            .status()
            .map_err(SwitchError::Repository)?
            .is_dirty()
    {
        Some(shelves::plan(repository).map_err(SwitchError::Shelf)?)
    } else {
        None
    };
    Ok(SwitchResult {
        task,
        branch,
        saved,
        restored,
        dry_run,
    })
}

/// Capture before switching, then restore only the target branch's matching shelf.
fn apply(
    repository: &Repository,
    session: &Session<'_>,
    result: &mut SwitchResult,
) -> Result<(), SwitchError> {
    if result.saved.is_some() {
        result.saved = Some(session.capture().map_err(SwitchError::Shelf)?);
    }
    let (current, _) = shelves::preflight(repository).map_err(SwitchError::Shelf)?;
    if current != format!("refs/heads/{}", result.branch).as_bytes()
        && let Err(error) =
            Executor::new(repository.work_dir()).switch_existing_branch(&result.branch)
    {
        // A failed checkout may leave the original branch active. Restore only when its
        // own shelf still matches; never apply it to a branch activated by a failing hook.
        if let Some(saved) = &result.saved {
            let _ = session.restore(Some(&saved.id));
        }
        return Err(SwitchError::Command(error));
    }
    if let Some(saved) = &result.restored {
        result.restored = Some(
            session
                .restore(Some(&saved.id))
                .map_err(SwitchError::Shelf)?,
        );
    }
    Ok(())
}
