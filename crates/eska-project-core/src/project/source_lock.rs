//! Shared source leases use persistent lock inodes and OS release on process exit.

use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions, TryLockError},
    io,
    path::{Path, PathBuf},
};

/// An owned handle is the write capability; filenames and numeric session IDs never grant it.
#[derive(Debug)]
pub struct SourceLease {
    _files: Vec<File>,
    root: PathBuf,
    names: Vec<String>,
}

impl SourceLease {
    /// Acquire every source in canonical order, releasing the partial set on any failure.
    ///
    /// # Errors
    /// Returns `WouldBlock` for a competing writer and IO errors for unsafe or unavailable lock storage.
    pub fn acquire(root: &Path, paths: &[PathBuf]) -> io::Result<Self> {
        let directory = directory(root, true)?;
        let mut names = paths
            .iter()
            .map(|path| key(path))
            .collect::<io::Result<Vec<_>>>()?;
        names.sort();
        names.dedup();
        let mut files = Vec::new();
        for name in &names {
            let path = directory.join(name);
            let file = open(&path, true)?;
            lock(&file)?;
            files.push(file);
        }
        Ok(Self {
            _files: files,
            root: fs::canonicalize(root)?,
            names,
        })
    }

    /// A checked publisher may bypass only the exact source lease it already owns in this project.
    ///
    /// # Errors
    /// Returns filesystem failures while resolving either supplied identity.
    pub fn owns(&self, root: &Path, path: &Path) -> io::Result<bool> {
        Ok(fs::canonicalize(root)? == self.root && self.names.binary_search(&key(path)?).is_ok())
    }

    /// Check a known lock without creating a directory or persistent file for read-only previews.
    ///
    /// # Errors
    /// Returns `WouldBlock` while a competing query/writer owns this source; unsafe storage fails closed.
    pub fn check(root: &Path, path: &Path) -> io::Result<()> {
        let directory = directory(root, false)?;
        let path = directory.join(key(path)?);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
            Ok(_) => {
                let file = open(&path, false)?;
                lock(&file)
            }
        }
    }
}

/// Only canonical source identities share a lock; different path spellings cannot bypass an owner.
fn key(path: &Path) -> io::Result<String> {
    const HEX: &[u8] = b"0123456789abcdef";
    let canonical = fs::canonicalize(path)?;
    let digest = Sha256::digest(canonical.as_os_str().as_encoded_bytes());
    let hash = digest
        .iter()
        .flat_map(|byte| {
            [
                char::from(HEX[usize::from(byte >> 4)]),
                char::from(HEX[usize::from(byte & 15)]),
            ]
        })
        .collect::<String>();
    Ok(format!("{hash}.lock"))
}

/// Never traverse a foreign symlink when creating project-owned lock storage.
fn directory(root: &Path, create: bool) -> io::Result<PathBuf> {
    let mut current = fs::canonicalize(root)?;
    for name in [".eska", "source-locks"] {
        current.push(name);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.is_dir() && !metadata.is_symlink() => {}
            Ok(_) => return Err(io::Error::from(io::ErrorKind::InvalidInput)),
            Err(error) if error.kind() == io::ErrorKind::NotFound && create => {
                match fs::create_dir(&current) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                        if !fs::symlink_metadata(&current)
                            .is_ok_and(|metadata| metadata.is_dir() && !metadata.is_symlink())
                        {
                            return Err(error);
                        }
                    }
                    Err(error) => return Err(error),
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(current)
}

/// Empty regular lock files are durable coordination state, never disposable temporary paths.
fn open(path: &Path, create: bool) -> io::Result<File> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.is_file() || metadata.is_symlink() || metadata.len() != 0 => {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(create)
        .truncate(false)
        .open(path)
}

/// Standard-library locks are released by close/crash; no stale PID cleanup or process killing is needed.
fn lock(file: &File) -> io::Result<()> {
    file.try_lock().map_err(|error| match error {
        TryLockError::WouldBlock => io::Error::from(io::ErrorKind::WouldBlock),
        TryLockError::Error(error) => error,
    })
}
