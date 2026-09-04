//! Locale-independent execution of the workflow plan for starting a task.

use std::path::PathBuf;

use gix::{ObjectId, bstr::ByteSlice};

use super::Project;
use crate::vcs::{
    command::{Error as CommandError, Executor},
    repository::{Error as RepositoryError, Head, ReferenceTarget, Repository},
    workflow::PolicyError,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartResult {
    pub task: String,
    pub branch: String,
    pub base_branch: String,
    pub base_updated: bool,
}

#[derive(Debug)]
pub enum StartError {
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
    DetachedHead,
    UnbornHead,
    BaseBranchMissing {
        branch: String,
    },
    RemoteBaseMissing {
        reference: String,
    },
    TaskBranchExists {
        branch: String,
    },
    BaseDiverged {
        branch: String,
        remote_reference: String,
    },
    Command(CommandError),
}

/// Fetch and fast-forward the policy base, then create and activate a task branch.
///
/// Task registration is represented by the active branch, which status resolves through policy.
/// Preflight covers the entire worktree because switching branches can affect files outside a
/// nested project root.
///
/// # Errors
/// Returns a structured error before worktree mutation when policy or repository preflight fails.
pub fn execute(project: &Project, task: &str) -> Result<StartResult, StartError> {
    let settings = project
        .configuration()
        .workflow_settings()
        .ok_or(StartError::WorkflowNotConfigured)?;
    let policy = settings.resolve(None).map_err(StartError::Policy)?;
    let plan = policy.plan(task).map_err(StartError::Policy)?;
    let repository = Repository::discover(project.root()).map_err(StartError::Repository)?;
    ensure_project_in_repository(project, &repository)?;
    let on_base = ensure_attached_head(&repository, &plan.base_branch)?;

    let status = repository.status().map_err(StartError::Repository)?;
    if status.is_dirty() {
        return Err(StartError::DirtyWorkspace {
            files: status.entries.len(),
        });
    }

    let base_reference = format!("refs/heads/{}", plan.base_branch);
    let task_reference = format!("refs/heads/{}", plan.working_branch);
    if reference_id(&repository, &base_reference)?.is_none() {
        return Err(StartError::BaseBranchMissing {
            branch: plan.base_branch,
        });
    }
    if reference_id(&repository, &task_reference)?.is_some() {
        return Err(StartError::TaskBranchExists {
            branch: plan.working_branch,
        });
    }

    let executor = Executor::new(repository.work_dir());
    executor
        .fetch(policy.remote())
        .map_err(StartError::Command)?;

    // Fetch changes refs and the object database, so reopen before reading the result.
    let repository = Repository::discover(project.root()).map_err(StartError::Repository)?;
    let base_id = reference_id(&repository, &base_reference)?.ok_or_else(|| {
        StartError::BaseBranchMissing {
            branch: plan.base_branch.clone(),
        }
    })?;
    let remote_id = reference_id(&repository, &plan.sync_reference)?;
    let Some(remote_id) = remote_id else {
        return Err(StartError::RemoteBaseMissing {
            reference: plan.sync_reference,
        });
    };
    let executor = Executor::new(repository.work_dir());
    let base_updated = if base_id == remote_id {
        false
    } else if executor
        .is_ancestor(&base_reference, &plan.sync_reference)
        .map_err(StartError::Command)?
    {
        if on_base {
            executor
                .fast_forward_current(&plan.sync_reference)
                .map_err(StartError::Command)?;
        } else {
            executor
                .fast_forward_inactive(&plan.base_branch, &plan.sync_reference)
                .map_err(StartError::Command)?;
        }
        true
    } else if executor
        .is_ancestor(&plan.sync_reference, &base_reference)
        .map_err(StartError::Command)?
    {
        false
    } else {
        return Err(StartError::BaseDiverged {
            branch: plan.base_branch,
            remote_reference: plan.sync_reference,
        });
    };

    executor
        .switch_new_branch(&plan.working_branch, &base_reference)
        .map_err(StartError::Command)?;

    Ok(StartResult {
        task: task.to_owned(),
        branch: plan.working_branch,
        base_branch: plan.base_branch,
        base_updated,
    })
}

fn ensure_project_in_repository(
    project: &Project,
    repository: &Repository,
) -> Result<(), StartError> {
    if project.root().starts_with(repository.work_dir()) {
        Ok(())
    } else {
        Err(StartError::ProjectOutsideRepository {
            project: project.root().to_owned(),
            repository: repository.work_dir().to_owned(),
        })
    }
}

fn ensure_attached_head(repository: &Repository, base_branch: &str) -> Result<bool, StartError> {
    match repository.head().map_err(StartError::Repository)? {
        Head::Attached { reference, .. } => {
            Ok(reference.as_bstr() == format!("refs/heads/{base_branch}").as_bytes())
        }
        Head::Detached { .. } => Err(StartError::DetachedHead),
        Head::Unborn { .. } => Err(StartError::UnbornHead),
    }
}

fn reference_id(repository: &Repository, name: &str) -> Result<Option<ObjectId>, StartError> {
    Ok(repository
        .references()
        .map_err(StartError::Repository)?
        .into_iter()
        .find(|reference| reference.name.as_bstr() == name.as_bytes())
        .and_then(|reference| match reference.target {
            ReferenceTarget::Object(id) => Some(id),
            ReferenceTarget::Symbolic(_) => None,
        }))
}
