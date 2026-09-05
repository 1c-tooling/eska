//! Locale-independent completion of the active workflow task.

use std::path::PathBuf;

use gix::{ObjectId, bstr::ByteSlice};

use super::Project;
use crate::vcs::{
    command::{Error as CommandError, Executor},
    network,
    repository::{Error as RepositoryError, Head, ReferenceTarget, Remote, Repository},
    workflow::{FinishRequirement, PolicyError},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinishResult {
    pub task: String,
    pub task_branch: String,
    pub base_branch: String,
    pub base_updated: bool,
    pub remote: Option<String>,
    pub branch_deleted: bool,
}

#[derive(Debug)]
pub enum FinishError {
    WorkflowNotConfigured,
    Policy(PolicyError),
    Repository(RepositoryError),
    ProjectOutsideRepository {
        project: PathBuf,
        repository: PathBuf,
    },
    OperationInProgress,
    DirtyWorkspace {
        files: usize,
    },
    DetachedHead,
    UnbornHead,
    NotTaskBranch,
    BaseBranchMissing {
        branch: String,
    },
    RemoteBaseMissing {
        remote: String,
        url: String,
        reference: String,
    },
    RemoteRequired {
        remote: String,
    },
    RequirementReferenceMissing {
        reference: String,
    },
    NotPublished {
        reference: String,
    },
    NotIntegrated {
        reference: String,
    },
    BaseDiverged {
        branch: String,
        remote_reference: String,
    },
    Fetch {
        remote: String,
        url: String,
        reason: String,
    },
    Ancestry(RepositoryError),
    UpdateBase(RepositoryError),
    Switch(CommandError),
    DeleteBranch(RepositoryError),
}

/// Verify the active task against current policy refs and complete its local lifecycle.
///
/// Preflight covers the complete worktree. A configured remote is fetched and the policy base is
/// updated only by fast-forward. The operation never publishes, merges or deletes a remote branch.
///
/// # Errors
/// Returns a structured error before cleanup when repository state or policy requirements do not
/// permit completion. A branch deletion error can occur after the worktree has switched to base.
pub fn execute(project: &Project) -> Result<FinishResult, FinishError> {
    let settings = project
        .configuration()
        .workflow_settings()
        .ok_or(FinishError::WorkflowNotConfigured)?;
    let policy = settings.resolve(None).map_err(FinishError::Policy)?;
    let repository = Repository::discover(project.root()).map_err(FinishError::Repository)?;
    ensure_project_in_repository(project, &repository)?;
    if repository.has_in_progress_operation() {
        return Err(FinishError::OperationInProgress);
    }
    let status = repository.status().map_err(FinishError::Repository)?;
    if status.is_dirty() {
        return Err(FinishError::DirtyWorkspace {
            files: status.entries.len(),
        });
    }

    let (task_branch, task_tip) = active_branch(&repository)?;
    let task = policy
        .task_id(&task_branch)
        .ok_or(FinishError::NotTaskBranch)?
        .to_owned();
    let plan = policy.plan(&task).map_err(FinishError::Policy)?;
    let base_reference = format!("refs/heads/{}", plan.base_branch);
    let base_id = reference_id(&repository, &base_reference)?.ok_or_else(|| {
        FinishError::BaseBranchMissing {
            branch: plan.base_branch.clone(),
        }
    })?;

    let remote = repository
        .remote(policy.remote())
        .map_err(FinishError::Repository)?;
    let (repository, base_updated) = if let Some(remote) = &remote {
        update_base_from_remote(project, &repository, remote, &plan, base_id)?
    } else {
        (repository, false)
    };

    verify_finish_requirement(
        &repository,
        remote.as_ref(),
        policy.remote(),
        &plan,
        task_tip,
    )?;

    Executor::new(repository.work_dir())
        .switch_existing_branch(&plan.base_branch)
        .map_err(FinishError::Switch)?;
    if plan.delete_local_branch {
        let repository = Repository::discover(project.root()).map_err(FinishError::DeleteBranch)?;
        repository
            .delete_inactive_reference(&format!("refs/heads/{task_branch}"), task_tip)
            .map_err(FinishError::DeleteBranch)?;
    }

    Ok(FinishResult {
        task,
        task_branch,
        base_branch: plan.base_branch,
        base_updated,
        remote: remote.map(|remote| remote.name().to_owned()),
        branch_deleted: plan.delete_local_branch,
    })
}

fn update_base_from_remote(
    project: &Project,
    repository: &Repository,
    remote: &Remote,
    plan: &crate::vcs::workflow::TaskPlan,
    base_id: ObjectId,
) -> Result<(Repository, bool), FinishError> {
    if let Err(error) = network::fetch(repository, remote.name()) {
        return Err(fetch_error(remote, &error));
    }
    let repository = Repository::discover(project.root()).map_err(FinishError::Repository)?;
    let remote_id = reference_id(&repository, &plan.sync_reference)?.ok_or_else(|| {
        FinishError::RemoteBaseMissing {
            remote: remote.name().to_owned(),
            url: remote.url().to_owned(),
            reference: plan.sync_reference.clone(),
        }
    })?;
    let updated = if base_id == remote_id {
        false
    } else if repository
        .is_ancestor(base_id, remote_id)
        .map_err(FinishError::Ancestry)?
    {
        repository
            .update_inactive_reference(
                &format!("refs/heads/{}", plan.base_branch),
                base_id,
                remote_id,
            )
            .map_err(FinishError::UpdateBase)?;
        true
    } else if repository
        .is_ancestor(remote_id, base_id)
        .map_err(FinishError::Ancestry)?
    {
        false
    } else {
        return Err(FinishError::BaseDiverged {
            branch: plan.base_branch.clone(),
            remote_reference: plan.sync_reference.clone(),
        });
    };
    Ok((repository, updated))
}

fn verify_finish_requirement(
    repository: &Repository,
    remote: Option<&Remote>,
    remote_name: &str,
    plan: &crate::vcs::workflow::TaskPlan,
    task_tip: ObjectId,
) -> Result<(), FinishError> {
    match plan.finish {
        FinishRequirement::Published => {
            let remote = remote.ok_or_else(|| FinishError::RemoteRequired {
                remote: remote_name.to_owned(),
            })?;
            let reference = format!("refs/remotes/{}/{}", remote.name(), plan.working_branch);
            let Some(target) = reference_id(repository, &reference)? else {
                return Err(FinishError::NotPublished { reference });
            };
            let published = repository
                .is_ancestor(task_tip, target)
                .map_err(FinishError::Ancestry)?;
            if published {
                Ok(())
            } else {
                Err(FinishError::NotPublished { reference })
            }
        }
        FinishRequirement::Integrated => {
            let reference = remote.map_or_else(
                || format!("refs/heads/{}", plan.integration_target),
                |remote| format!("refs/remotes/{}/{}", remote.name(), plan.integration_target),
            );
            let target = reference_id(repository, &reference)?.ok_or_else(|| {
                FinishError::RequirementReferenceMissing {
                    reference: reference.clone(),
                }
            })?;
            if repository
                .is_ancestor(task_tip, target)
                .map_err(FinishError::Ancestry)?
            {
                Ok(())
            } else {
                Err(FinishError::NotIntegrated { reference })
            }
        }
    }
}

fn active_branch(repository: &Repository) -> Result<(String, ObjectId), FinishError> {
    match repository.head().map_err(FinishError::Repository)? {
        Head::Attached { reference, id } => {
            let branch = reference
                .as_bstr()
                .strip_prefix(b"refs/heads/")
                .and_then(|name| name.to_str().ok())
                .ok_or(FinishError::NotTaskBranch)?;
            Ok((branch.to_owned(), id))
        }
        Head::Detached { .. } => Err(FinishError::DetachedHead),
        Head::Unborn { .. } => Err(FinishError::UnbornHead),
    }
}

fn ensure_project_in_repository(
    project: &Project,
    repository: &Repository,
) -> Result<(), FinishError> {
    if project.root().starts_with(repository.work_dir()) {
        Ok(())
    } else {
        Err(FinishError::ProjectOutsideRepository {
            project: project.root().to_owned(),
            repository: repository.work_dir().to_owned(),
        })
    }
}

fn reference_id(repository: &Repository, name: &str) -> Result<Option<ObjectId>, FinishError> {
    Ok(repository
        .references()
        .map_err(FinishError::Repository)?
        .into_iter()
        .find(|reference| reference.name.as_bstr() == name.as_bytes())
        .and_then(|reference| match reference.target {
            ReferenceTarget::Object(id) => Some(id),
            ReferenceTarget::Symbolic(_) => None,
        }))
}

fn fetch_error(remote: &Remote, error: &network::FetchError) -> FinishError {
    FinishError::Fetch {
        remote: remote.name().to_owned(),
        url: remote.url().to_owned(),
        reason: remote.sanitize_diagnostic(error.to_string().as_bytes()),
    }
}
