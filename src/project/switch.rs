//! Locale-independent switching between existing workflow branches.

use std::path::PathBuf;

use gix::bstr::ByteSlice;

use super::Project;
use crate::vcs::{
    command::{Error as CommandError, Executor},
    repository::{Error as RepositoryError, ReferenceTarget, Repository},
    workflow::PolicyError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchTarget<'a> {
    Task(&'a str),
    Base,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchResult {
    pub task: Option<String>,
    pub branch: String,
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
    DirtyWorkspace {
        files: usize,
    },
    TaskBranchMissing {
        branch: String,
    },
    BaseBranchMissing {
        branch: String,
    },
    Command(CommandError),
}

/// Activate an existing task branch or return temporarily to the workflow base.
///
/// The active branch remains the only task state. Preflight covers the entire repository
/// worktree and the operation never fetches or creates a branch.
///
/// # Errors
/// Returns a structured error before worktree mutation when policy, repository, cleanliness or
/// target-reference validation fails, or when Git cannot switch the checked-out worktree.
pub fn execute(project: &Project, target: SwitchTarget<'_>) -> Result<SwitchResult, SwitchError> {
    let settings = project
        .configuration()
        .workflow_settings()
        .ok_or(SwitchError::WorkflowNotConfigured)?;
    let policy = settings.resolve(None).map_err(SwitchError::Policy)?;
    let (task, branch, missing) = match target {
        SwitchTarget::Task(task) => {
            let plan = policy.plan(task).map_err(SwitchError::Policy)?;
            (
                Some(task.to_owned()),
                plan.working_branch,
                MissingBranch::Task,
            )
        }
        SwitchTarget::Base => (None, policy.base_branch().to_owned(), MissingBranch::Base),
    };

    let repository = Repository::discover(project.root()).map_err(SwitchError::Repository)?;
    ensure_project_in_repository(project, &repository)?;
    let status = repository.status().map_err(SwitchError::Repository)?;
    if status.is_dirty() {
        return Err(SwitchError::DirtyWorkspace {
            files: status.entries.len(),
        });
    }
    if !local_branch_exists(&repository, &branch)? {
        return Err(match missing {
            MissingBranch::Task => SwitchError::TaskBranchMissing { branch },
            MissingBranch::Base => SwitchError::BaseBranchMissing { branch },
        });
    }

    Executor::new(repository.work_dir())
        .switch_existing_branch(&branch)
        .map_err(SwitchError::Command)?;
    Ok(SwitchResult { task, branch })
}

#[derive(Debug, Clone, Copy)]
enum MissingBranch {
    Task,
    Base,
}

fn ensure_project_in_repository(
    project: &Project,
    repository: &Repository,
) -> Result<(), SwitchError> {
    if project.root().starts_with(repository.work_dir()) {
        Ok(())
    } else {
        Err(SwitchError::ProjectOutsideRepository {
            project: project.root().to_owned(),
            repository: repository.work_dir().to_owned(),
        })
    }
}

fn local_branch_exists(repository: &Repository, branch: &str) -> Result<bool, SwitchError> {
    let reference = format!("refs/heads/{branch}");
    Ok(repository
        .references()
        .map_err(SwitchError::Repository)?
        .into_iter()
        .any(|candidate| {
            candidate.name.as_bstr() == reference.as_bytes()
                && matches!(candidate.target, ReferenceTarget::Object(_))
        }))
}
