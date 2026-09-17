//! Repository-wide, byte-exact shelves with durable payloads and guarded restoration.

use super::{
    command::Executor,
    repository::{Head, Repository},
    status::{Change, PathStatus},
};
use gix::{ObjectId, bstr::ByteSlice};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions, TryLockError},
    io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

mod files;

#[derive(Debug)]
pub enum Error {
    Repository(super::repository::Error),
    Command(super::command::Error),
    Io(io::Error),
    InvalidShelf,
    Locked,
    Detached,
    Unborn,
    InProgress,
    UnsupportedIndex,
    Empty,
    Exists,
    Missing,
    WrongBranch,
    MovedBranch,
    Dirty,
    Collision(PathBuf),
    Incomplete,
}

impl From<io::Error> for Error {
    /// Retain filesystem failures as structured errors outside presentation.
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<super::repository::Error> for Error {
    /// Retain repository failures without parsing human diagnostics.
    fn from(error: super::repository::Error) -> Self {
        Self::Repository(error)
    }
}
impl From<super::command::Error> for Error {
    /// Retain the isolated Git process failure.
    fn from(error: super::command::Error) -> Self {
        Self::Command(error)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Shelf {
    pub id: String,
    pub branch: Vec<u8>,
    pub base: String,
    pub created_at: u64,
    pub files: Vec<ShelfPath>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ShelfPath {
    pub path: Vec<u8>,
    pub index: Option<String>,
    pub worktree: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Stored {
    version: u8,
    shelf: Shelf,
    images: Vec<files::Image>,
    index: files::Image,
}

/// One repository-wide mutation lock shared by all linked worktrees.
pub struct Session<'a> {
    repository: &'a Repository,
    _lock: File,
}

/// Enumerate fully captured shelves without creating directories or lock files.
///
/// # Errors
/// Returns an error for inaccessible or malformed shelf data.
pub fn list(repository: &Repository) -> Result<Vec<Shelf>, Error> {
    let root = shelf_root(repository);
    if fs::symlink_metadata(&root).is_ok_and(|m| !m.is_dir() || m.file_type().is_symlink()) {
        return Err(Error::InvalidShelf);
    }
    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut shelves = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let Some(id) = name.to_str() else {
            return Err(Error::InvalidShelf);
        };
        if !entry.file_type()?.is_dir() {
            return Err(Error::InvalidShelf);
        }
        if entry.path().join("metadata.json").is_file() {
            shelves.push(load(repository, id)?.shelf);
        }
    }
    shelves.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(shelves)
}

/// Prepare a capture preview for the entire repository without mutating its index.
///
/// # Errors
/// Rejects ambiguous repository state, an existing shelf, or unsupported file kinds.
pub fn plan(repository: &Repository) -> Result<Shelf, Error> {
    let (branch, base) = preflight(repository)?;
    if list(repository)?.iter().any(|shelf| shelf.branch == branch) {
        return Err(Error::Exists);
    }
    let status = repository.status()?;
    if status.entries.is_empty() {
        return Err(Error::Empty);
    }
    for entry in &status.entries {
        files::inspect(&files::work_path(repository.work_dir(), &entry.path)?)?;
    }
    Ok(Shelf {
        id: String::new(),
        branch,
        base,
        created_at: 0,
        files: status.entries.iter().map(shelf_path).collect(),
    })
}

/// Validate an entire restoration preview without creating a journal or lock.
///
/// # Errors
/// Rejects branch mismatches, corrupt payloads and conflicting destination state.
pub fn restore_plan(repository: &Repository, id: Option<&str>) -> Result<Shelf, Error> {
    let saved = select_restore(repository, id)?;
    let stored = load(repository, &saved.id)?;
    let directory = shelf_directory(repository, &saved.id)?;
    validate_payloads(&stored, &directory)?;
    if !matches_snapshot(repository, &stored)? {
        if !directory.join("ready").is_file() {
            return Err(Error::Incomplete);
        }
        restore_preflight(repository, &stored, &directory, false)?;
    }
    Ok(saved)
}

/// Select the current branch's shelf or validate an explicitly requested stable ID.
///
/// # Errors
/// Rejects a different branch or base commit before any restoration.
fn select_restore(repository: &Repository, id: Option<&str>) -> Result<Shelf, Error> {
    let (branch, base) = preflight(repository)?;
    let shelf = list(repository)?
        .into_iter()
        .find(|shelf| id.map_or(shelf.branch == branch, |id| shelf.id == id))
        .ok_or(Error::Missing)?;
    if shelf.branch != branch {
        return Err(Error::WrongBranch);
    }
    if shelf.base != base {
        return Err(Error::MovedBranch);
    }
    Ok(shelf)
}

impl<'a> Session<'a> {
    /// Exclusively serialize eska shelf operations without waiting on a busy repository.
    ///
    /// # Errors
    /// Returns a lock or filesystem error before changing branch state.
    pub fn acquire(repository: &'a Repository) -> Result<Self, Error> {
        let path = repository.inner.common_dir().join("eska-shelves.lock");
        if fs::symlink_metadata(&path).is_ok_and(|m| !m.is_file() || m.file_type().is_symlink()) {
            return Err(Error::InvalidShelf);
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        lock.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => Error::Locked,
            TryLockError::Error(error) => Error::Io(error),
        })?;
        Ok(Self {
            repository,
            _lock: lock,
        })
    }

    /// Save exact bytes and staged objects before cleaning only the captured paths.
    ///
    /// # Errors
    /// Failed cleanup retains the durable shelf; the caller can inspect or restore it.
    pub fn capture(&self) -> Result<Shelf, Error> {
        let mut saved = plan(self.repository)?;
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::InvalidShelf)?;
        saved.id = format!("{:x}-{:x}", time.as_nanos(), std::process::id());
        saved.created_at = time.as_secs();
        let directory = shelf_directory(self.repository, &saved.id)?;
        fs::create_dir_all(shelf_root(self.repository))?;
        fs::create_dir(&directory)?;
        let index = files::capture(&self.repository.index_path(), &directory.join("index"))?;
        let mut images = Vec::with_capacity(saved.files.len());
        for (number, entry) in saved.files.iter().enumerate() {
            let path = files::work_path(self.repository.work_dir(), &entry.path)?;
            images.push(files::capture(&path, &directory.join(number.to_string()))?);
        }
        self.protect_index(&saved.id)?;
        let stored = Stored {
            version: 1,
            shelf: saved.clone(),
            images,
            index,
        };
        files::write_new(
            &directory.join("metadata.json"),
            &serde_json::to_vec(&stored).map_err(|_| Error::InvalidShelf)?,
        )?;
        // From this point every captured byte and staged blob has a durable owner.
        if let Err(error) = self.clean(&stored, &directory) {
            // A synchronous cleanup failure can roll back this operation's own writes.
            // If rollback also fails, every original payload remains in the shelf.
            if self.restore_saved(&stored, &directory).is_err() {
                return Err(Error::Incomplete);
            }
            return Err(error);
        }
        files::write_new(&directory.join("ready"), b"1\n")?;
        Ok(saved)
    }

    /// Restore only a matching branch/base, and consume a shelf after byte verification.
    ///
    /// # Errors
    /// Conflicts and failed writes preserve the shelf and its original payloads.
    pub fn restore(&self, id: Option<&str>) -> Result<Shelf, Error> {
        let saved = select_restore(self.repository, id)?;
        let stored = load(self.repository, &saved.id)?;
        let directory = shelf_directory(self.repository, &saved.id)?;
        validate_payloads(&stored, &directory)?;
        if !matches_snapshot(self.repository, &stored)? {
            if !directory.join("ready").is_file() {
                return Err(Error::Incomplete);
            }
            restore_preflight(self.repository, &stored, &directory, true)?;
            self.restore_saved(&stored, &directory)?;
        }
        self.consume(&stored.shelf.id)?;
        Ok(saved)
    }

    /// Install original bytes and verify the entire snapshot before considering it restored.
    fn restore_saved(&self, stored: &Stored, directory: &Path) -> Result<(), Error> {
        for (number, (entry, image)) in stored.shelf.files.iter().zip(&stored.images).enumerate() {
            let path = files::work_path(self.repository.work_dir(), &entry.path)?;
            files::restore(&path, image, &directory.join(number.to_string()))?;
        }
        self.restore_index(stored, directory)?;
        if !matches_snapshot(self.repository, stored)? {
            return Err(Error::Incomplete);
        }
        Ok(())
    }

    /// Pin the serialized index tree with gix so garbage collection cannot remove staged blobs.
    fn protect_index(&self, id: &str) -> Result<(), Error> {
        let tree = Executor::new(self.repository.work_dir()).shelf_index_tree()?;
        let tree = ObjectId::from_hex(&tree).map_err(|_| Error::InvalidShelf)?;
        let mut repository = self.repository.inner.clone();
        repository
            .committer_or_set_generic_fallback()
            .map_err(|_| Error::InvalidShelf)?;
        repository
            .reference(
                format!("refs/eska/shelves/{id}"),
                tree,
                gix::refs::transaction::PreviousValue::MustNotExist,
                "eska: preserve shelf index",
            )
            .map_err(|_| Error::InvalidShelf)?;
        Ok(())
    }

    /// Restore tracked paths to HEAD and delete only captured untracked files.
    fn clean(&self, stored: &Stored, directory: &Path) -> Result<(), Error> {
        let mut paths = Vec::new();
        for entry in &stored.shelf.files {
            if entry.index.is_some() || entry.worktree.as_deref() != Some("untracked") {
                paths.extend_from_slice(&entry.path);
                paths.push(0);
            }
        }
        if !paths.is_empty() {
            let pathspec = directory.join("paths");
            files::write_new(&pathspec, &paths)?;
            Executor::new(self.repository.work_dir())
                .clean_shelved_paths(&stored.shelf.base, &pathspec)?;
        }
        for (entry, image) in stored.shelf.files.iter().zip(&stored.images) {
            if entry.index.is_none() && entry.worktree.as_deref() == Some("untracked") {
                let path = files::work_path(self.repository.work_dir(), &entry.path)?;
                if files::inspect(&path)? != *image {
                    return Err(Error::Collision(path));
                }
                fs::remove_file(path)?;
            }
        }
        let refreshed = Repository::discover(self.repository.work_dir())?;
        if refreshed.status()?.is_dirty() {
            return Err(Error::Dirty);
        }
        Ok(())
    }

    /// Replace index bytes through Git's standard lock path only after worktree restoration.
    fn restore_index(&self, stored: &Stored, directory: &Path) -> Result<(), Error> {
        let index = self.repository.index_path();
        let lock = index.with_extension("lock");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock)
            .map_err(|error| {
                if error.kind() == io::ErrorKind::AlreadyExists {
                    Error::Locked
                } else {
                    error.into()
                }
            })?;
        let result = (|| {
            let mut input = File::open(directory.join("index"))?;
            io::copy(&mut input, &mut file)?;
            stored.index.apply_mode(&lock)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&lock, &index)?;
            if files::inspect(&index)? != stored.index {
                return Err(Error::Incomplete);
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(lock);
        }
        result
    }

    /// Remove the shelf only after verification; its private reference protects staged objects.
    fn consume(&self, id: &str) -> Result<(), Error> {
        let directory = shelf_directory(self.repository, id)?;
        fs::remove_dir_all(directory)?;
        if let Ok(reference) = self
            .repository
            .inner
            .find_reference(format!("refs/eska/shelves/{id}").as_str())
        {
            reference.delete().map_err(|_| Error::InvalidShelf)?;
        }
        Ok(())
    }
}

/// Reject states whose complete contents cannot be captured by this backend.
pub(crate) fn preflight(repository: &Repository) -> Result<(Vec<u8>, String), Error> {
    if repository.has_in_progress_operation() {
        return Err(Error::InProgress);
    }
    let (reference, id) = match repository.head()? {
        Head::Attached { reference, id } => (reference, id),
        Head::Detached { .. } => return Err(Error::Detached),
        Head::Unborn { .. } => return Err(Error::Unborn),
    };
    if repository.index_path().with_extension("lock").exists() {
        return Err(Error::Locked);
    }
    if !repository.index_path().is_file() {
        return Err(Error::UnsupportedIndex);
    }
    // Decode without resolving the link extension: gix's repository index accessor
    // dissolves split indexes and hides the dependency on sharedindex files.
    let (index, _) = gix::index::State::from_bytes(
        &fs::read(repository.index_path())?,
        SystemTime::now().into(),
        repository.inner.object_hash(),
        gix::index::decode::Options::default(),
    )
    .map_err(|_| Error::UnsupportedIndex)?;
    let flags = gix::index::entry::Flags::SKIP_WORKTREE
        | gix::index::entry::Flags::ASSUME_VALID
        | gix::index::entry::Flags::INTENT_TO_ADD;
    if index.is_sparse()
        || index.link().is_some()
        || index.entries().iter().any(|entry| {
            entry.mode.is_submodule()
                || entry.flags.intersects(flags)
                || entry.stage() != gix::index::entry::Stage::Unconflicted
        })
    {
        return Err(Error::UnsupportedIndex);
    }
    Ok((reference.to_vec(), id.to_string()))
}

/// Read and validate immutable metadata before trusting any persisted path or payload count.
fn load(repository: &Repository, id: &str) -> Result<Stored, Error> {
    let directory = shelf_directory(repository, id)?;
    let stored: Stored = serde_json::from_slice(&fs::read(directory.join("metadata.json"))?)
        .map_err(|_| Error::InvalidShelf)?;
    if stored.version != 1
        || stored.shelf.id != id
        || stored.images.len() != stored.shelf.files.len()
    {
        return Err(Error::InvalidShelf);
    }
    let mut paths = std::collections::HashSet::new();
    for entry in &stored.shelf.files {
        files::relative_path(&entry.path)?;
        if !paths.insert(&entry.path) {
            return Err(Error::InvalidShelf);
        }
    }
    Ok(stored)
}

/// Validate all snapshots before any restoring write is attempted.
fn validate_payloads(stored: &Stored, directory: &Path) -> Result<(), Error> {
    files::validate(&stored.index, &directory.join("index"))?;
    for (number, image) in stored.images.iter().enumerate() {
        files::validate(image, &directory.join(number.to_string()))?;
    }
    Ok(())
}

/// Keep shelf storage shared by the repository's main and linked worktrees.
fn shelf_root(repository: &Repository) -> PathBuf {
    repository.inner.common_dir().join("eska-shelves")
}

/// Validate user-supplied IDs and reject symlink-redirection of private storage.
fn shelf_directory(repository: &Repository, id: &str) -> Result<PathBuf, Error> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
    {
        return Err(Error::InvalidShelf);
    }
    let root = shelf_root(repository);
    let directory = root.join(id);
    for path in [&root, &directory] {
        if fs::symlink_metadata(path)
            .is_ok_and(|metadata| !metadata.is_dir() || metadata.file_type().is_symlink())
        {
            return Err(Error::InvalidShelf);
        }
    }
    Ok(directory)
}

/// Preserve staged and worktree changes independently in previews and stored metadata.
fn shelf_path(entry: &PathStatus) -> ShelfPath {
    ShelfPath {
        path: entry.path.to_vec(),
        index: entry.index.map(|v| change_name(v).to_owned()),
        worktree: entry.worktree.map(|v| change_name(v).to_owned()),
    }
}

/// Map file states to stable, non-localized names.
const fn change_name(change: Change) -> &'static str {
    match change {
        Change::Added => "added",
        Change::Modified => "modified",
        Change::Deleted => "deleted",
        Change::TypeChanged => "type_changed",
        Change::Untracked => "untracked",
        Change::IntentToAdd => "intent_to_add",
        Change::Conflict => "conflict",
    }
}

/// Journal clean destination images so interrupted restoration can be retried safely.
fn restore_preflight(
    repository: &Repository,
    stored: &Stored,
    directory: &Path,
    write: bool,
) -> Result<(), Error> {
    let journal = directory.join("restore.json");
    let current = Repository::discover(repository.work_dir())?;
    if journal.is_file() {
        let baseline: Vec<files::Image> =
            serde_json::from_slice(&fs::read(&journal)?).map_err(|_| Error::InvalidShelf)?;
        if baseline.len() != stored.images.len() {
            return Err(Error::InvalidShelf);
        }
        for ((entry, original), clean) in
            stored.shelf.files.iter().zip(&stored.images).zip(&baseline)
        {
            let path = files::work_path(repository.work_dir(), &entry.path)?;
            let image = files::inspect(&path)?;
            if image != *original && image != *clean {
                return Err(Error::Collision(path));
            }
        }
        if current.status()?.entries.iter().any(|entry| {
            !stored
                .shelf
                .files
                .iter()
                .any(|saved| saved.path == entry.path.as_bytes())
        }) {
            return Err(Error::Dirty);
        }
    } else {
        if current.status()?.is_dirty() {
            return Err(Error::Dirty);
        }
        let mut baseline = Vec::new();
        for entry in &stored.shelf.files {
            let path = files::work_path(repository.work_dir(), &entry.path)?;
            if repository
                .inner
                .head_tree()
                .map_err(|_| Error::InvalidShelf)?
                .lookup_entry_by_path(gix::path::from_bstr(entry.path.as_bstr()).as_ref())
                .map_err(|_| Error::InvalidShelf)?
                .is_none()
                && fs::symlink_metadata(&path).is_ok()
            {
                return Err(Error::Collision(path));
            }
            baseline.push(files::inspect(&path)?);
        }
        if !write {
            return Ok(());
        }
        let baseline_index = directory.join("restore-index");
        let index = fs::read(repository.index_path())?;
        if baseline_index.exists() {
            // A crash between the two journal files must allow a safe retry.
            if fs::read(&baseline_index)? != index {
                return Err(Error::Dirty);
            }
        } else {
            files::write_new(&baseline_index, &index)?;
        }
        files::write_new(
            &journal,
            &serde_json::to_vec(&baseline).map_err(|_| Error::InvalidShelf)?,
        )?;
    }
    let index = fs::read(repository.index_path())?;
    if index != fs::read(directory.join("restore-index"))?
        && index != fs::read(directory.join("index"))?
    {
        return Err(Error::Dirty);
    }
    Ok(())
}

/// Check the captured raw index and all affected file bytes before consuming storage.
fn matches_snapshot(repository: &Repository, stored: &Stored) -> Result<bool, Error> {
    if files::inspect(&repository.index_path())? != stored.index {
        return Ok(false);
    }
    for (entry, image) in stored.shelf.files.iter().zip(&stored.images) {
        if files::inspect(&files::work_path(repository.work_dir(), &entry.path)?)? != *image {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Verify a target shelf before a switch captures the current branch's state.
pub(crate) fn validate_saved(repository: &Repository, id: &str) -> Result<(), Error> {
    let stored = load(repository, id)?;
    let directory = shelf_directory(repository, id)?;
    if !directory.join("ready").is_file() {
        return Err(Error::Incomplete);
    }
    validate_payloads(&stored, &directory)
}
