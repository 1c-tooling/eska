//! Safe project-scoped commits without exposing Git staging in the public UX.

use std::{fs, io, path::PathBuf};

use gix::{ObjectId, bstr::ByteSlice};

use super::Project;
use crate::vcs::{
    command::{Error as CommandError, Executor},
    repository::{Error as RepositoryError, Head, Repository},
    status::{Change, PathStatus},
};

/// Result of saving one project `ChangeSet`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveResult {
    pub commit: ObjectId,
    pub files: usize,
}

/// Errors that leave the worktree unchanged and restore prior staging when possible.
#[derive(Debug)]
pub enum SaveError {
    Repository(RepositoryError),
    ProjectOutsideRepository {
        project: PathBuf,
        repository: PathBuf,
    },
    DetachedHead,
    NoChanges,
    Conflicts {
        files: usize,
    },
    EmptyMessage,
    IndexSnapshot {
        path: PathBuf,
        source: io::Error,
    },
    Command(CommandError),
    IndexRestore {
        path: PathBuf,
        source: io::Error,
        original: Box<Self>,
    },
    CommitNotCreated,
}

/// Save every current change below the project root in one commit.
///
/// Existing staged changes outside the project are excluded and preserved. If staging or commit
/// creation fails, the original index is restored byte-for-byte. Concurrent index mutation and
/// abrupt process termination are outside this rollback guarantee.
///
/// # Errors
/// Returns a structured error for invalid repository state, conflicts, empty changes, index I/O
/// or a failed Git staging/commit operation.
pub fn execute(project: &Project, message: Option<&str>) -> Result<SaveResult, SaveError> {
    execute_with_message(project, SaveMessage::Explicit(message))
}

/// Save every current project change after opening Git's editor with a generated draft.
///
/// # Errors
/// Returns the same structured preflight, staging, editor, commit and rollback failures as
/// [`execute`]. An empty generated draft is rejected before repository mutation.
pub fn execute_with_draft(project: &Project, draft: &str) -> Result<SaveResult, SaveError> {
    execute_with_message(project, SaveMessage::Draft(draft))
}

#[derive(Clone, Copy)]
enum SaveMessage<'a> {
    Explicit(Option<&'a str>),
    Draft(&'a str),
}

impl SaveMessage<'_> {
    /// Return whether the selected message source is empty before invoking Git.
    fn is_empty(&self) -> bool {
        match self {
            Self::Explicit(message) => message.is_some_and(|message| message.trim().is_empty()),
            Self::Draft(draft) => draft.trim().is_empty(),
        }
    }
}

/// Execute the shared project-scoped staging, commit and rollback transaction.
fn execute_with_message(
    project: &Project,
    message: SaveMessage<'_>,
) -> Result<SaveResult, SaveError> {
    if message.is_empty() {
        return Err(SaveError::EmptyMessage);
    }

    let repository = Repository::discover(project.root()).map_err(SaveError::Repository)?;
    ensure_project_in_repository(project, &repository)?;
    if matches!(
        repository.head().map_err(SaveError::Repository)?,
        Head::Detached { .. }
    ) {
        return Err(SaveError::DetachedHead);
    }

    let status = repository.status().map_err(SaveError::Repository)?;
    let changes: Vec<_> = status
        .entries
        .iter()
        .filter(|entry| belongs_to_project(&repository, project, entry))
        .collect();
    if changes.is_empty() {
        return Err(SaveError::NoChanges);
    }
    let conflicts = changes
        .iter()
        .filter(|entry| {
            entry.index == Some(Change::Conflict) || entry.worktree == Some(Change::Conflict)
        })
        .count();
    if conflicts > 0 {
        return Err(SaveError::Conflicts { files: conflicts });
    }

    let snapshot = IndexSnapshot::capture(repository.index_path())?;
    let executor = Executor::new(project.root());
    if let Err(error) = executor.stage_all().map_err(SaveError::Command) {
        return snapshot.restore_after(error);
    }
    let commit = match message {
        SaveMessage::Explicit(message) => executor.commit_only(message),
        SaveMessage::Draft(draft) => executor.commit_only_with_draft(repository.git_dir(), draft),
    };
    if let Err(error) = commit.map_err(SaveError::Command) {
        return snapshot.restore_after(error);
    }

    let repository = Repository::discover(project.root()).map_err(SaveError::Repository)?;
    let commit = repository
        .head()
        .map_err(SaveError::Repository)?
        .id()
        .ok_or(SaveError::CommitNotCreated)?;
    Ok(SaveResult {
        commit,
        files: changes.len(),
    })
}

fn ensure_project_in_repository(
    project: &Project,
    repository: &Repository,
) -> Result<(), SaveError> {
    if project.root().starts_with(repository.work_dir()) {
        Ok(())
    } else {
        Err(SaveError::ProjectOutsideRepository {
            project: project.root().to_owned(),
            repository: repository.work_dir().to_owned(),
        })
    }
}

fn belongs_to_project(repository: &Repository, project: &Project, entry: &PathStatus) -> bool {
    project.root() == repository.work_dir()
        || repository
            .work_dir()
            .join(gix::path::from_bstr(entry.path.as_bstr()).as_ref())
            .starts_with(project.root())
}

struct IndexSnapshot {
    path: PathBuf,
    contents: Option<Vec<u8>>,
}

impl IndexSnapshot {
    fn capture(path: PathBuf) -> Result<Self, SaveError> {
        let contents = match fs::read(&path) {
            Ok(contents) => Some(contents),
            Err(source) if source.kind() == io::ErrorKind::NotFound => None,
            Err(source) => return Err(SaveError::IndexSnapshot { path, source }),
        };
        Ok(Self { path, contents })
    }

    fn restore_after<T>(self, original: SaveError) -> Result<T, SaveError> {
        match self.restore() {
            Ok(()) => Err(original),
            Err(source) => Err(SaveError::IndexRestore {
                path: self.path,
                source,
                original: Box::new(original),
            }),
        }
    }

    fn restore(&self) -> Result<(), io::Error> {
        self.contents.as_ref().map_or_else(
            || match fs::remove_file(&self.path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error),
            },
            |contents| fs::write(&self.path, contents),
        )
    }
}
