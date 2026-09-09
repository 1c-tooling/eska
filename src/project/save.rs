//! Safe project-scoped commits without exposing Git staging in the public UX.

use std::{fs, io, path::PathBuf};

use gix::{
    ObjectId,
    bstr::{BStr, BString, ByteSlice},
};

use super::{Project, Workspace};
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

/// Exact read-only snapshot of files that one save would include.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavePlan {
    files: Vec<SaveFile>,
}

impl SavePlan {
    #[must_use]
    pub fn files(&self) -> &[SaveFile] {
        &self.files
    }
}

/// One scope-relative path and its staged and unstaged states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveFile {
    path: BString,
    index: Option<Change>,
    worktree: Option<Change>,
}

impl SaveFile {
    #[must_use]
    pub fn path(&self) -> &BStr {
        self.path.as_bstr()
    }

    #[must_use]
    pub const fn index(&self) -> Option<Change> {
        self.index
    }

    #[must_use]
    pub const fn worktree(&self) -> Option<Change> {
        self.worktree
    }
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

/// Prepare an exact project save snapshot without changing repository state.
///
/// # Errors
/// Returns the same repository, scope, HEAD, empty-scope and conflict preflight
/// failures as [`execute`].
pub fn plan(project: &Project) -> Result<SavePlan, SaveError> {
    prepare_scope(project.root()).map(|prepared| prepared.plan)
}

/// Prepare an exact workspace save snapshot without changing repository state.
///
/// # Errors
/// Returns the same preflight failures as [`execute_workspace`].
pub fn plan_workspace(workspace: &Workspace) -> Result<SavePlan, SaveError> {
    prepare_scope(workspace.root()).map(|prepared| prepared.plan)
}

/// Validate a supplied commit message before preparing or executing a save.
///
/// # Errors
/// Returns [`SaveError::EmptyMessage`] when the message contains only whitespace.
pub fn validate_message(message: &str) -> Result<(), SaveError> {
    if message.trim().is_empty() {
        Err(SaveError::EmptyMessage)
    } else {
        Ok(())
    }
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
    execute_scope(project.root(), SaveMessage::Explicit(message))
}

/// Save every current project change after opening Git's editor with a generated draft.
///
/// # Errors
/// Returns the same structured preflight, staging, editor, commit and rollback failures as
/// [`execute`]. An empty generated draft is rejected before repository mutation.
pub fn execute_with_draft(project: &Project, draft: &str) -> Result<SaveResult, SaveError> {
    execute_scope(project.root(), SaveMessage::Draft(draft))
}

/// Save every current change inside a workspace while preserving repository siblings.
///
/// # Errors
/// Returns the same structured preflight, staging, commit and rollback failures as [`execute`].
pub fn execute_workspace(
    workspace: &Workspace,
    message: Option<&str>,
) -> Result<SaveResult, SaveError> {
    execute_scope(workspace.root(), SaveMessage::Explicit(message))
}

/// Save every current workspace change after opening Git's editor with a generated draft.
///
/// # Errors
/// Returns the same structured failures as [`execute_workspace`].
pub fn execute_workspace_with_draft(
    workspace: &Workspace,
    draft: &str,
) -> Result<SaveResult, SaveError> {
    execute_scope(workspace.root(), SaveMessage::Draft(draft))
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
            Self::Explicit(None) => false,
            Self::Explicit(Some(message)) | Self::Draft(message) => {
                validate_message(message).is_err()
            }
        }
    }
}

/// Execute the shared project-scoped staging, commit and rollback transaction.
fn execute_scope(
    root: &std::path::Path,
    message: SaveMessage<'_>,
) -> Result<SaveResult, SaveError> {
    if message.is_empty() {
        return Err(SaveError::EmptyMessage);
    }

    let PreparedSave { repository, plan } = prepare_scope(root)?;
    let files = plan.files.len();

    let snapshot = IndexSnapshot::capture(repository.index_path())?;
    let executor = Executor::new(root);
    if let Err(error) = executor.stage_all().map_err(SaveError::Command) {
        return snapshot.restore_after(error);
    }
    let commit = match message {
        SaveMessage::Explicit(message) => executor.commit_only(message),
        SaveMessage::Draft(draft) => executor.commit_only_with_draft(draft),
    };
    if let Err(error) = commit.map_err(SaveError::Command) {
        return snapshot.restore_after(error);
    }

    let repository = Repository::discover(root).map_err(SaveError::Repository)?;
    let commit = repository
        .head()
        .map_err(SaveError::Repository)?
        .id()
        .ok_or(SaveError::CommitNotCreated)?;
    Ok(SaveResult { commit, files })
}

struct PreparedSave {
    repository: Repository,
    plan: SavePlan,
}

/// Read and validate every repository input that determines the save scope.
fn prepare_scope(root: &std::path::Path) -> Result<PreparedSave, SaveError> {
    let repository = Repository::discover(root).map_err(SaveError::Repository)?;
    ensure_scope_in_repository(root, &repository)?;
    if matches!(
        repository.head().map_err(SaveError::Repository)?,
        Head::Detached { .. }
    ) {
        return Err(SaveError::DetachedHead);
    }

    let status = repository.status().map_err(SaveError::Repository)?;
    let files = status
        .entries
        .iter()
        .filter_map(|entry| save_file(&repository, root, entry))
        .collect::<Vec<_>>();
    if files.is_empty() {
        return Err(SaveError::NoChanges);
    }
    let conflicts = files
        .iter()
        .filter(|file| {
            file.index == Some(Change::Conflict) || file.worktree == Some(Change::Conflict)
        })
        .count();
    if conflicts > 0 {
        return Err(SaveError::Conflicts { files: conflicts });
    }

    Ok(PreparedSave {
        repository,
        plan: SavePlan { files },
    })
}

fn ensure_scope_in_repository(
    root: &std::path::Path,
    repository: &Repository,
) -> Result<(), SaveError> {
    if root.starts_with(repository.work_dir()) {
        Ok(())
    } else {
        Err(SaveError::ProjectOutsideRepository {
            project: root.to_owned(),
            repository: repository.work_dir().to_owned(),
        })
    }
}

fn save_file(
    repository: &Repository,
    root: &std::path::Path,
    entry: &PathStatus,
) -> Option<SaveFile> {
    let absolute = repository
        .work_dir()
        .join(gix::path::from_bstr(entry.path.as_bstr()).as_ref());
    let relative = absolute.strip_prefix(root).ok()?;
    let path =
        gix::path::to_unix_separators_on_windows(gix::path::into_bstr(relative)).into_owned();
    Some(SaveFile {
        path,
        index: entry.index,
        worktree: entry.worktree,
    })
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
