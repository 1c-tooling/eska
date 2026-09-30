//! Same-directory replacement, with a final comparison against the expected source bytes.

use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use super::{EditError, EditPlan};

/// A temporary file belongs to this write alone, including on every failure path.
struct Temporary(PathBuf);

impl Drop for Temporary {
    /// Never remove the destination or another operation's temporary file.
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

impl EditPlan {
    /// Publish after rechecking containment, source bytes and file permissions.
    ///
    /// The final compare detects prior external writes, but is not an OS-level
    /// compare-and-swap against an unrelated writer racing the following rename.
    ///
    /// # Errors
    /// Returns a conflict, unsafe path, read-only source or filesystem error. No
    /// delete-then-rename fallback is used when replacement is unavailable.
    pub fn publish(&self, root: &Path, relative: &Path) -> Result<(), EditError> {
        let path = checked_path(root, relative)?;
        let permissions = fs::metadata(&path).map_err(EditError::Io)?.permissions();
        if permissions.readonly() {
            return Err(EditError::ReadOnly);
        }
        compare(&path, &self.original)?;
        if self.is_empty() {
            return Ok(());
        }
        let parent = path.parent().ok_or(EditError::UnsafePath)?;
        let (temporary, mut file) = temporary(parent)?;
        file.write_all(self.updated.as_bytes())
            .map_err(EditError::Io)?;
        file.set_permissions(permissions.clone())
            .map_err(EditError::Io)?;
        file.sync_all().map_err(EditError::Io)?;
        drop(file);
        if checked_path(root, relative)? != path {
            return Err(EditError::Conflict);
        }
        compare(&path, &self.original)?;
        if fs::metadata(&path).map_err(EditError::Io)?.permissions() != permissions {
            return Err(EditError::Conflict);
        }
        fs::rename(&temporary.0, &path).map_err(EditError::Io)
    }
}

/// Reject aliases so an edit cannot replace a symlink or traverse outside the configured source.
fn checked_path(root: &Path, relative: &Path) -> Result<PathBuf, EditError> {
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(EditError::UnsafePath);
    }
    let mut path = root.canonicalize().map_err(EditError::Io)?;
    for component in relative.components() {
        path.push(component);
        if fs::symlink_metadata(&path)
            .map_err(EditError::Io)?
            .file_type()
            .is_symlink()
        {
            return Err(EditError::UnsafePath);
        }
    }
    if !fs::metadata(&path).map_err(EditError::Io)?.is_file() {
        return Err(EditError::UnsafePath);
    }
    Ok(path)
}

/// Bound rereads even if an external process grows the file between metadata and open.
fn compare(path: &Path, expected: &str) -> Result<(), EditError> {
    let mut actual = Vec::new();
    fs::File::open(path)
        .map_err(EditError::Io)?
        .take(expected.len() as u64 + 1)
        .read_to_end(&mut actual)
        .map_err(EditError::Io)?;
    if actual != expected.as_bytes() {
        return Err(EditError::Conflict);
    }
    Ok(())
}

/// Use exclusive creation; collisions never overwrite another operation's staging file.
fn temporary(parent: &Path) -> Result<(Temporary, fs::File), EditError> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| EditError::UnsafePath)?
        .as_nanos();
    for attempt in 0..32 {
        let path = parent.join(format!(
            ".eska-properties-{}-{stamp}-{attempt}.tmp",
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((Temporary(path), file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(EditError::Io(error)),
        }
    }
    Err(EditError::Io(std::io::Error::from(
        std::io::ErrorKind::AlreadyExists,
    )))
}
