//! Locale-independent local commit history with conservative task attribution.

use std::{collections::BTreeSet, path::PathBuf};

use gix::{ObjectId, bstr::ByteSlice};

use super::Project;
use crate::vcs::{
    repository::{Commit, Error as RepositoryError, ReferenceTarget, Repository},
    workflow::{PolicyError, WorkflowPolicy},
};

/// A local commit and its unambiguous workflow task, when one can be proven.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub commit: Commit,
    pub task: Option<String>,
}

#[derive(Debug)]
pub enum HistoryError {
    Policy(PolicyError),
    Repository(RepositoryError),
    ProjectOutsideRepository {
        project: PathBuf,
        repository: PathBuf,
    },
}

/// Read commits reachable from HEAD without fetching or changing repository state.
///
/// Task attribution is deliberately conservative: a commit is attributed only when it is
/// outside the workflow base and reachable from exactly one matching local task branch.
///
/// # Errors
/// Returns a structured error when configured workflow policy or repository data cannot be read.
pub fn inspect(project: &Project, limit: usize) -> Result<Vec<HistoryEntry>, HistoryError> {
    let repository = Repository::discover(project.root()).map_err(HistoryError::Repository)?;
    if !project.root().starts_with(repository.work_dir()) {
        return Err(HistoryError::ProjectOutsideRepository {
            project: project.root().to_owned(),
            repository: repository.work_dir().to_owned(),
        });
    }

    let commits = repository
        .history(limit)
        .map_err(HistoryError::Repository)?;
    let Some(settings) = project.configuration().workflow_settings() else {
        return Ok(without_tasks(commits));
    };
    let policy = settings.resolve(None).map_err(HistoryError::Policy)?;
    let references = repository.references().map_err(HistoryError::Repository)?;
    let base_name = format!("refs/heads/{}", policy.base_branch());
    let Some(base) = direct_target(&references, &base_name) else {
        return Ok(without_tasks(commits));
    };
    let task_tips = task_tips(&references, &policy);

    commits
        .into_iter()
        .map(|commit| {
            let task = task_for_commit(&repository, commit.id, base, &task_tips)?;
            Ok(HistoryEntry { commit, task })
        })
        .collect()
}

fn without_tasks(commits: Vec<Commit>) -> Vec<HistoryEntry> {
    commits
        .into_iter()
        .map(|commit| HistoryEntry { commit, task: None })
        .collect()
}

fn direct_target(references: &[crate::vcs::repository::Reference], name: &str) -> Option<ObjectId> {
    references
        .iter()
        .find_map(|reference| {
            (reference.name.as_bstr() == name.as_bytes()).then_some(&reference.target)
        })
        .and_then(|target| match target {
            ReferenceTarget::Object(id) => Some(*id),
            ReferenceTarget::Symbolic(_) => None,
        })
}

fn task_tips(
    references: &[crate::vcs::repository::Reference],
    policy: &WorkflowPolicy,
) -> Vec<(String, ObjectId)> {
    references
        .iter()
        .filter_map(|reference| {
            let branch = reference.name.as_slice().strip_prefix(b"refs/heads/")?;
            let branch = branch.to_str().ok()?;
            let task = policy.task_id(branch)?;
            let ReferenceTarget::Object(id) = reference.target else {
                return None;
            };
            Some((task.to_owned(), id))
        })
        .collect()
}

fn task_for_commit(
    repository: &Repository,
    commit: ObjectId,
    base: ObjectId,
    task_tips: &[(String, ObjectId)],
) -> Result<Option<String>, HistoryError> {
    if repository
        .is_ancestor(commit, base)
        .map_err(HistoryError::Repository)?
    {
        return Ok(None);
    }

    let mut tasks = BTreeSet::new();
    for (task, tip) in task_tips {
        if repository
            .is_ancestor(commit, *tip)
            .map_err(HistoryError::Repository)?
        {
            tasks.insert(task.clone());
            if tasks.len() > 1 {
                return Ok(None);
            }
        }
    }
    Ok(tasks.into_iter().next())
}
