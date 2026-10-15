//! Cooperative project writers retain one stable lock inode across saves, imports and recovery.

use std::{
    fs::{self, File, OpenOptions, TryLockError},
    path::Path,
};

use super::{Error, files};

#[cfg(test)]
mod tests;

/// Ownership is unique: no cloned file handle can silently extend this publication interval.
#[derive(Debug)]
pub(super) struct Guard {
    file: File,
}

impl Drop for Guard {
    /// Explicit unlock ends the interval even if an unrelated fork inherited the descriptor.
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

/// Hold this guard until all guarded checks and the final publication have finished.
///
/// Dropping the guard releases the OS lock, but must never unlink its stable path: another
/// process may already have opened that inode. External writers remain outside this protocol.
pub(super) fn acquire(project: &Path) -> Result<Guard, Error> {
    checked_directory(project)?;
    let service = project.join(".eska");
    match fs::create_dir(&service) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(files::io(&error)),
    }
    checked_directory(&service)?;
    let path = service.join("source-write.lock");
    match fs::symlink_metadata(&path) {
        Ok(_) => checked_file(&path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(files::io(&error)),
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|error| files::io(&error))?;
    if !file
        .metadata()
        .map_err(|error| files::io(&error))?
        .is_file()
    {
        return Err(Error::root("source_edit_unsafe_path"));
    }
    checked_directory(project)?;
    checked_directory(&service)?;
    checked_file(&path)?;
    file.try_lock().map_err(|error| match error {
        TryLockError::WouldBlock => Error::root("source_write_busy"),
        TryLockError::Error(error) => files::io(&error),
    })?;
    let guard = Guard { file };
    // Cooperating writers never replace the path; recheck aliases after acquiring its lock.
    checked_directory(project)?;
    checked_directory(&service)?;
    checked_file(&path)?;
    Ok(guard)
}

/// Canonical equality refuses aliases at every project/service component before file creation.
fn checked_directory(path: &Path) -> Result<(), Error> {
    if !fs::symlink_metadata(path)
        .map_err(|error| files::io(&error))?
        .file_type()
        .is_dir()
        || path.canonicalize().map_err(|error| files::io(&error))? != path
    {
        return Err(Error::root("source_edit_unsafe_path"));
    }
    Ok(())
}

/// A lock path is a regular non-aliased file, never a directory, link or special device.
fn checked_file(path: &Path) -> Result<(), Error> {
    if !fs::symlink_metadata(path)
        .map_err(|error| files::io(&error))?
        .file_type()
        .is_file()
        || path.canonicalize().map_err(|error| files::io(&error))? != path
    {
        return Err(Error::root("source_edit_unsafe_path"));
    }
    Ok(())
}
