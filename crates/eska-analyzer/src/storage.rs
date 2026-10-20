//! Exclusively owned staging files and OS locks bound every publication to one workspace root.

use std::{
    fs::{self, File, OpenOptions, TryLockError},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::Error;

/// An open lock is released by the OS when the process exits, including interrupted clients.
pub struct ConfigLock(pub File);

impl ConfigLock {
    /// Acquire a writer lock without waiting behind another editor or preparing a different context.
    pub fn exclusive(root: &Path) -> Result<Self, Error> {
        let lock = Self::open(root)?;
        lock.0.try_lock().map_err(|error| lock_error(root, error))?;
        Ok(lock)
    }

    /// A running client shares a lease only with other readers of the same verified config.
    pub fn shared(root: &Path) -> Result<Self, Error> {
        let lock = Self::open(root)?;
        lock.0
            .try_lock_shared()
            .map_err(|error| lock_error(root, error))?;
        Ok(lock)
    }

    /// Never follow a generated-state directory or lock symlink outside the selected project.
    fn open(root: &Path) -> Result<Self, Error> {
        let directory = root.join(".eska");
        match fs::create_dir(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(source) => {
                return Err(Error::Io {
                    path: directory,
                    source,
                });
            }
        }
        let metadata = fs::symlink_metadata(&directory).map_err(|source| Error::Io {
            path: directory.clone(),
            source,
        })?;
        if metadata.is_symlink() || !metadata.is_dir() {
            return Err(Error::ManagedConflict { path: directory });
        }
        let path = directory.join("analyzer-config.lock");
        if fs::symlink_metadata(&path)
            .is_ok_and(|metadata| metadata.is_symlink() || !metadata.is_file())
        {
            return Err(Error::ManagedConflict { path });
        }
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map(Self)
            .map_err(|source| Error::Io { path, source })
    }
}

/// Keep lock contention distinct from filesystem failures in machine output.
fn lock_error(root: &Path, error: TryLockError) -> Error {
    let path = root.join(".eska/analyzer-config.lock");
    match error {
        TryLockError::WouldBlock => Error::Busy { path },
        TryLockError::Error(source) => Error::Io { path, source },
    }
}

/// Staging never overwrites another operation and cleans up only its exclusively created path.
pub struct Temporary {
    path: PathBuf,
    published: bool,
}

impl Temporary {
    /// Keep a candidate in the final directory, preserving all relative configuration paths.
    pub fn new(root: &Path, contents: &[u8]) -> Result<Self, Error> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for attempt in 0..1024 {
            let path = root.join(format!(
                ".eska-analyzer-{}-{timestamp}-{attempt}.toml",
                std::process::id()
            ));
            let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(source) => return Err(Error::Io { path, source }),
            };
            let temporary = Self {
                path,
                published: false,
            };
            let result = file.write_all(contents).and_then(|()| file.sync_all());
            drop(file);
            result.map_err(|source| Error::Io {
                path: temporary.path.clone(),
                source,
            })?;
            return Ok(temporary);
        }
        Err(Error::Io {
            path: root.into(),
            source: io::Error::new(io::ErrorKind::AlreadyExists, "staging namespace exhausted"),
        })
    }

    /// Expose the candidate path only while its owner remains alive.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Publish a fully flushed candidate by renaming it in the same directory.
    pub fn publish(mut self, target: &Path) -> Result<(), Error> {
        fs::rename(&self.path, target).map_err(|source| Error::Io {
            path: target.into(),
            source,
        })?;
        self.published = true;
        Ok(())
    }
}

impl Drop for Temporary {
    /// Disarm cleanup after publication so a later file at the old name cannot be removed.
    fn drop(&mut self) {
        if !self.published {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// Compare immediately before replacement; manual edits do not participate in our advisory lock.
pub fn unchanged(path: &Path, expected: Option<&[u8]>) -> Result<(), Error> {
    if read_optional(path)?.as_deref() == expected {
        Ok(())
    } else {
        Err(Error::StalePreview)
    }
}

/// Treat an absent path separately from unreadable, non-file or symlink destinations.
pub fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, Error> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.is_symlink() => {
            const LIMIT: u64 = 64 * 1024 * 1024;
            if metadata.len() > LIMIT {
                return Err(Error::InputTooLarge { path: path.into() });
            }
            let mut bytes = Vec::new();
            File::open(path)
                .and_then(|file| file.take(LIMIT + 1).read_to_end(&mut bytes))
                .map_err(|source| Error::Io {
                    path: path.into(),
                    source,
                })?;
            if bytes.len() as u64 > LIMIT {
                return Err(Error::InputTooLarge { path: path.into() });
            }
            Ok(Some(bytes))
        }
        Ok(_) => Err(Error::ManagedConflict { path: path.into() }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::Io {
            path: path.into(),
            source,
        }),
    }
}
