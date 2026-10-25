//! Atomic checkpoints use an OS-released store lock and compare revisions across IDE windows.

use std::{
    fs::{self, File, OpenOptions, TryLockError},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use super::{Error, MAX_FILE_BYTES, MAX_RECORDS, MAX_RETAINED_BYTES, Record, is_id};
use crate::project::metadata_edit::snapshot;

const LOCK_FILE: &str = ".eska-query-recovery.lock";
const MAX_ENTRIES: usize = 1024;

/// The revision identifies the exact acknowledged file, including older clients' JSON field order.
#[derive(Clone, Debug)]
pub struct Stored {
    pub record: Record,
    pub revision: String,
}

/// Damaged records remain on disk and never become automatic source-edit instructions.
#[derive(Debug, Default)]
pub struct Contents {
    pub records: Vec<Stored>,
    pub damaged: Vec<String>,
    pub limited: bool,
}

/// The caller supplies private storage; this type never falls back to a project source directory.
#[derive(Debug)]
pub struct Store {
    directory: PathBuf,
}

impl Store {
    /// Reject lexical aliases before any directory creation or checkpoint access.
    ///
    /// # Errors
    /// Rejects relative paths and explicit traversal components.
    pub fn new(directory: PathBuf) -> Result<Self, Error> {
        if !directory.is_absolute()
            || directory
                .components()
                .any(|part| matches!(part, Component::CurDir | Component::ParentDir))
        {
            return Err(Error::UnsafePath);
        }
        Ok(Self { directory })
    }

    /// Load a bounded collection without creating storage or modifying any existing record.
    ///
    /// # Errors
    /// Rejects aliased directories, IO failures and an excessive directory inventory.
    pub fn load(&self) -> Result<Contents, Error> {
        if !checked_directory(&self.directory, false)? {
            return Ok(Contents::default());
        }
        let mut names = Vec::new();
        for entry in fs::read_dir(&self.directory)? {
            if names.len() == MAX_ENTRIES {
                return Err(Error::ResourceLimit);
            }
            names.push(entry?.file_name());
        }
        names.sort();
        let mut contents = Contents::default();
        let mut retained = 0;
        let mut scanned = 0;
        for name in names {
            let Some(id) = name
                .to_str()
                .and_then(|name| name.strip_suffix(".json"))
                .filter(|id| is_id(id))
            else {
                continue;
            };
            match self.load_record(id, &mut scanned) {
                Ok(stored)
                    if contents.records.len() < MAX_RECORDS
                        && retained + stored.record.retained_bytes() <= MAX_RETAINED_BYTES =>
                {
                    retained += stored.record.retained_bytes();
                    contents.records.push(stored);
                }
                Ok(_) | Err(Error::ResourceLimit) => {
                    contents.limited = true;
                    contents.damaged.push(id.to_owned());
                }
                Err(_) => contents.damaged.push(id.to_owned()),
            }
        }
        Ok(contents)
    }

    /// Publish one acknowledged checkpoint after current quota and revision checks under the shared lock.
    ///
    /// # Errors
    /// Rejects concurrent storage ownership, a stale revision, quota exhaustion or IO failure.
    pub fn save(&self, record: Record, expected: Option<&str>) -> Result<Stored, Error> {
        checked_directory(&self.directory, true)?;
        let _lock = self.lock()?;
        let destination = self.path(record.id());
        compare_revision(&destination, expected)?;
        let current = self.load()?;
        if current.limited {
            return Err(Error::ResourceLimit);
        }
        let previous = current
            .records
            .iter()
            .find(|stored| stored.record.id() == record.id());
        let used: usize = current
            .records
            .iter()
            .map(|stored| stored.record.retained_bytes())
            .sum();
        if (previous.is_none() && current.records.len() >= MAX_RECORDS)
            || used - previous.map_or(0, |stored| stored.record.retained_bytes())
                + record.retained_bytes()
                > MAX_RETAINED_BYTES
        {
            return Err(Error::ResourceLimit);
        }
        let bytes = record.encode()?;
        let revision = bytes_snapshot(&bytes)?;
        if expected != Some(revision.as_str()) {
            let (temporary, mut file) = temporary(&self.directory)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            compare_revision(&destination, expected)?;
            fs::rename(&temporary.0, &destination)?;
            sync_directory(&self.directory)?;
        }
        Ok(Stored { record, revision })
    }

    /// Remove only the caller's acknowledged revision; another window's checkpoint is preserved.
    ///
    /// # Errors
    /// Rejects invalid IDs, stale revisions, aliased files and IO failures.
    pub fn remove(&self, id: &str, expected: &str) -> Result<bool, Error> {
        if !is_id(id) {
            return Err(Error::InvalidRecord);
        }
        if !checked_directory(&self.directory, false)? {
            return Ok(false);
        }
        let _lock = self.lock()?;
        let destination = self.path(id);
        if missing(&destination)? {
            return Ok(false);
        }
        compare_revision(&destination, Some(expected))?;
        fs::remove_file(destination)?;
        sync_directory(&self.directory)?;
        Ok(true)
    }

    /// A source plan must notice another window's checkpoint before it reaches the native editor.
    ///
    /// # Errors
    /// Rejects an invalid ID, an aliased path or any changed/missing acknowledged file.
    pub fn verify(&self, id: &str, expected: &str) -> Result<(), Error> {
        if !is_id(id) {
            return Err(Error::InvalidRecord);
        }
        if !checked_directory(&self.directory, false)? {
            return Err(Error::Conflict);
        }
        compare_revision(&self.path(id), Some(expected))
    }

    /// Only validated canonical IDs can be joined to the private store path.
    fn path(&self, id: &str) -> PathBuf {
        self.directory.join(format!("{id}.json"))
    }

    /// A persistent lock inode is never removed; process exit releases its OS lock automatically.
    fn lock(&self) -> Result<File, Error> {
        let path = self.directory.join(LOCK_FILE);
        if !missing(&path)? && !fs::symlink_metadata(&path)?.file_type().is_file() {
            return Err(Error::UnsafePath);
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        private_mode(&mut options);
        let file = options.open(path)?;
        match file.try_lock() {
            Ok(()) => Ok(file),
            Err(TryLockError::WouldBlock) => Err(Error::Busy),
            Err(TryLockError::Error(error)) => Err(error.into()),
        }
    }

    /// Bound both individual reads and the total encoded data scanned in one operation.
    fn load_record(&self, id: &str, scanned: &mut usize) -> Result<Stored, Error> {
        let bytes = read_record_limit(&self.path(id), MAX_FILE_BYTES - *scanned)?;
        *scanned = scanned.saturating_add(bytes.len());
        if *scanned > MAX_FILE_BYTES {
            return Err(Error::ResourceLimit);
        }
        let record = Record::decode(&bytes)?;
        if record.id() != id {
            return Err(Error::InvalidRecord);
        }
        Ok(Stored {
            record,
            revision: bytes_snapshot(&bytes)?,
        })
    }
}

/// Check every existing path component before creating missing private directories.
fn checked_directory(path: &Path, create: bool) -> Result<bool, Error> {
    let mut current = PathBuf::new();
    for part in path.components() {
        current.push(part.as_os_str());
        if missing(&current)? {
            if !create {
                return Ok(false);
            }
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&current) {
                Ok(()) => (),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(error) => return Err(error.into()),
            }
        }
        if !fs::symlink_metadata(&current)?.file_type().is_dir() {
            return Err(Error::UnsafePath);
        }
    }
    Ok(true)
}

/// A missing file is distinct from a dangling symlink or an inaccessible path.
fn missing(path: &Path) -> Result<bool, Error> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error.into()),
    }
}

/// Metadata and a bounded handle read reject aliases and files grown during the operation.
fn read_record(path: &Path) -> Result<Vec<u8>, Error> {
    read_record_limit(path, MAX_FILE_BYTES)
}

/// An exhausted scan budget rejects another record before allocating or reading its body.
fn read_record_limit(path: &Path, limit: usize) -> Result<Vec<u8>, Error> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(Error::UnsafePath);
    }
    if metadata.len() > limit as u64 {
        return Err(Error::ResourceLimit);
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(Error::ResourceLimit);
    }
    Ok(bytes)
}

/// Compare exact persisted bytes, including records written by the previous IDE-owned store.
fn compare_revision(path: &Path, expected: Option<&str>) -> Result<(), Error> {
    if missing(path)? {
        return if expected.is_none() {
            Ok(())
        } else {
            Err(Error::Conflict)
        };
    }
    let expected = expected.ok_or(Error::Conflict)?;
    if bytes_snapshot(&read_record(path)?)? != expected {
        return Err(Error::Conflict);
    }
    Ok(())
}

/// Checkpoints are UTF-8 JSON even though their three texts are stored as base64.
fn bytes_snapshot(bytes: &[u8]) -> Result<String, Error> {
    std::str::from_utf8(bytes)
        .map(snapshot)
        .map_err(|_| Error::InvalidRecord)
}

/// Temporary cleanup owns only the exclusively created staging file.
struct Temporary(PathBuf);

impl Drop for Temporary {
    /// Destination records and another operation's staging files are never removed on failure.
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Exclusive creation leaves earlier complete checkpoints intact on collision or failure.
fn temporary(directory: &Path) -> Result<(Temporary, File), Error> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Error::UnsafePath)?
        .as_nanos();
    for attempt in 0..32 {
        let path = directory.join(format!(
            ".eska-query-{}-{stamp}-{attempt}.tmp",
            std::process::id()
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        private_mode(&mut options);
        match options.open(&path) {
            Ok(file) => return Ok((Temporary(path), file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(error.into()),
        }
    }
    Err(Error::Busy)
}

/// New private records do not inherit a permissive process umask on Unix.
fn private_mode(options: &mut OpenOptions) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    #[cfg(not(unix))]
    let _ = options;
}

/// Sync the containing directory after rename/removal where directory handles support it.
fn sync_directory(path: &Path) -> Result<(), Error> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
