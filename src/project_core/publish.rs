//! Same-directory replacement, with a final comparison against the expected source bytes.

use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

/// Exact-byte publication failures independent of metadata and presentation.
#[derive(Debug)]
pub enum PublishError {
    Conflict,
    UnsafePath,
    ReadOnly,
    Cancelled,
    Io(io::Error),
}

#[cfg(test)]
mod tests;

/// A temporary file belongs to this write alone, including on every failure path.
struct Temporary(PathBuf);

impl Drop for Temporary {
    /// Never remove the destination or another operation's temporary file.
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Other source formats share the same exact-byte and permission checks as XML edits.
///
/// The final comparison detects prior writes, not a noncooperating writer racing rename.
///
/// # Errors
/// Returns a conflict, unsafe path, read-only source or filesystem failure.
pub fn publish_snapshot(
    root: &Path,
    relative: &Path,
    original: &str,
    updated: &str,
) -> Result<(), PublishError> {
    publish_snapshot_cancellable(root, relative, original, updated, &|| false)
}

/// Cancellation can refuse a prepared replacement only before its final rename.
///
/// No cancellation check follows a successful commit, so a caller receives its actual outcome.
///
/// # Errors
/// Returns publication failures or cancellation without a committed replacement.
pub fn publish_snapshot_cancellable(
    root: &Path,
    relative: &Path,
    original: &str,
    updated: &str,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), PublishError> {
    publish_bytes(root, relative, original, updated, None, cancelled)
}

/// A structural journal owns the temporary file too, making process termination recoverable.
///
/// # Errors
/// Returns the same publication failures as [`publish_snapshot`].
pub fn publish_snapshot_in(
    root: &Path,
    relative: &Path,
    original: &str,
    updated: &str,
    staging: &Path,
) -> Result<(), PublishError> {
    publish_bytes(root, relative, original, updated, Some(staging), &|| false)
}

/// Publish a complete new file without replacing a concurrent writer's destination.
///
/// A same-directory hard link makes the prepared bytes visible at once and refuses an
/// existing path. Filesystems without hard links return an error rather than a partial file.
///
/// # Errors
/// Returns a conflict, unsafe parent or filesystem failure. Parent directories must exist.
pub fn publish_new(root: &Path, relative: &Path, updated: &str) -> Result<(), PublishError> {
    publish_new_cancellable(root, relative, updated, &|| false)
}

/// A new file honors cancellation through the last check before its atomic no-overwrite link.
///
/// # Errors
/// Returns publication failures or cancellation before the new destination becomes visible.
pub fn publish_new_cancellable(
    root: &Path,
    relative: &Path,
    updated: &str,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), PublishError> {
    check_cancelled(cancelled)?;
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        || root.canonicalize().map_err(PublishError::Io)? != root
    {
        return Err(PublishError::UnsafePath);
    }
    let path = root.join(relative);
    let parent = path.parent().ok_or(PublishError::UnsafePath)?;
    if parent.canonicalize().map_err(PublishError::Io)? != parent {
        return Err(PublishError::UnsafePath);
    }
    require_absent(&path)?;
    let (temporary, mut file) = temporary(parent)?;
    file.write_all(updated.as_bytes())
        .map_err(PublishError::Io)?;
    file.sync_all().map_err(PublishError::Io)?;
    drop(file);
    if parent.canonicalize().map_err(PublishError::Io)? != parent {
        return Err(PublishError::UnsafePath);
    }
    check_cancelled(cancelled)?;
    fs::hard_link(&temporary.0, &path).map_err(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists {
            PublishError::Conflict
        } else {
            PublishError::Io(error)
        }
    })
}

/// A dangling symlink or a directory is also a concurrent destination, never free space.
fn require_absent(path: &Path) -> Result<(), PublishError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(PublishError::Conflict),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(PublishError::Io(error)),
    }
}

/// Both scalar and structural writes use the same permission and exact-byte replacement checks.
fn publish_bytes(
    root: &Path,
    relative: &Path,
    original: &str,
    updated: &str,
    staging: Option<&Path>,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), PublishError> {
    check_cancelled(cancelled)?;
    let path = checked_path(root, relative)?;
    let permissions = fs::metadata(&path).map_err(PublishError::Io)?.permissions();
    if permissions.readonly() {
        return Err(PublishError::ReadOnly);
    }
    compare(&path, original)?;
    if original == updated {
        check_cancelled(cancelled)?;
        return Ok(());
    }
    let parent = path.parent().ok_or(PublishError::UnsafePath)?;
    let (temporary, mut file) = if let Some(directory) = staging {
        let path = directory.join("publish.tmp");
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(PublishError::Io)?;
        (Temporary(path), file)
    } else {
        temporary(parent)?
    };
    file.write_all(updated.as_bytes())
        .map_err(PublishError::Io)?;
    file.set_permissions(permissions.clone())
        .map_err(PublishError::Io)?;
    file.sync_all().map_err(PublishError::Io)?;
    drop(file);
    if checked_path(root, relative)? != path {
        return Err(PublishError::Conflict);
    }
    compare(&path, original)?;
    if fs::metadata(&path).map_err(PublishError::Io)?.permissions() != permissions {
        return Err(PublishError::Conflict);
    }
    check_cancelled(cancelled)?;
    fs::rename(&temporary.0, &path).map_err(PublishError::Io)
}

/// Only pre-commit callers consult cancellation; completed publication never calls this again.
fn check_cancelled(cancelled: &dyn Fn() -> bool) -> Result<(), PublishError> {
    if cancelled() {
        return Err(PublishError::Cancelled);
    }
    Ok(())
}

/// Reject aliases so an edit cannot replace a symlink or traverse outside the configured source.
fn checked_path(root: &Path, relative: &Path) -> Result<PathBuf, PublishError> {
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(PublishError::UnsafePath);
    }
    let mut path = root.canonicalize().map_err(PublishError::Io)?;
    if path != root {
        return Err(PublishError::UnsafePath);
    }
    for component in relative.components() {
        path.push(component);
        if fs::symlink_metadata(&path)
            .map_err(PublishError::Io)?
            .file_type()
            .is_symlink()
        {
            return Err(PublishError::UnsafePath);
        }
    }
    if !fs::metadata(&path).map_err(PublishError::Io)?.is_file() {
        return Err(PublishError::UnsafePath);
    }
    Ok(path)
}

/// Bound rereads even if an external process grows the file between metadata and open.
fn compare(path: &Path, expected: &str) -> Result<(), PublishError> {
    let mut actual = Vec::new();
    fs::File::open(path)
        .map_err(PublishError::Io)?
        .take(expected.len() as u64 + 1)
        .read_to_end(&mut actual)
        .map_err(PublishError::Io)?;
    if actual != expected.as_bytes() {
        return Err(PublishError::Conflict);
    }
    Ok(())
}

/// Use exclusive creation; collisions never overwrite another operation's staging file.
fn temporary(parent: &Path) -> Result<(Temporary, fs::File), PublishError> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| PublishError::UnsafePath)?
        .as_nanos();
    for attempt in 0..32 {
        let path = parent.join(format!(
            ".eska-properties-{}-{stamp}-{attempt}.tmp",
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((Temporary(path), file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(PublishError::Io(error)),
        }
    }
    Err(PublishError::Io(std::io::Error::from(
        std::io::ErrorKind::AlreadyExists,
    )))
}
