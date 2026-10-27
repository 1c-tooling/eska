//! Migration publishes only after a checked preview and an exclusive byte-exact backup.

use super::{Plan, preview};
use crate::formatting::{Error, protocol::source_sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

/// Apply an unchanged preview, retaining a deterministic backup and original file permissions.
///
/// # Errors
/// Rejects stale/forged plans, symlinks, read-only manifests, foreign backups and I/O failures.
pub fn apply(path: &Path, plan: &Plan) -> Result<PathBuf, Error> {
    let valid = preview(&plan.before)?.is_some_and(|valid| valid == *plan);
    if !valid {
        return Err(Error::new("invalid_migration_plan", "plan"));
    }
    let metadata =
        fs::symlink_metadata(path).map_err(|_| Error::new("io", path.to_string_lossy()))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.permissions().readonly()
    {
        return Err(Error::new("protected_source", path.to_string_lossy()));
    }
    let previous = fs::read(path).map_err(|_| Error::new("io", path.to_string_lossy()))?;
    let parent = path
        .parent()
        .ok_or_else(|| Error::new("invalid_path", path.to_string_lossy()))?;
    let backup = parent.join(format!(
        ".eska-fmt-backup-{}.toml",
        source_sha256(&plan.before)
    ));
    if previous == plan.after.as_bytes() {
        verify_backup(&backup, &plan.before)?;
        return Ok(backup);
    }
    if previous != plan.before.as_bytes() {
        return Err(Error::new("source_conflict", path.to_string_lossy()));
    }
    backup_original(&backup, &plan.before, &metadata.permissions())?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Error::new("io", "clock"))?
        .as_nanos();
    let temporary = parent.join(format!(".eska-fmt-{}-{nonce}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|_| Error::new("io", temporary.to_string_lossy()))?;
    let guard = Temporary(temporary);
    publish(&mut file, &guard.0, path, plan, metadata.permissions())
        .map_err(|_| Error::new("source_write_failed", path.to_string_lossy()))?;
    Ok(backup)
}

/// An existing backup is reusable only when it is an ordinary file containing the exact preview.
fn verify_backup(path: &Path, text: &str) -> Result<(), Error> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| Error::new("backup_conflict", path.to_string_lossy()))?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || fs::read(path).ok().as_deref() != Some(text.as_bytes())
    {
        return Err(Error::new("backup_conflict", path.to_string_lossy()));
    }
    Ok(())
}

/// Backup creation never replaces a user file, even when its name matches a planned transaction.
fn backup_original(path: &Path, text: &str, permissions: &fs::Permissions) -> Result<(), Error> {
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => file
            .set_permissions(permissions.clone())
            .and_then(|()| file.write_all(text.as_bytes()))
            .and_then(|()| file.sync_all())
            .map_err(|_| Error::new("backup_write_failed", path.to_string_lossy())),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => verify_backup(path, text),
        Err(_) => Err(Error::new("backup_write_failed", path.to_string_lossy())),
    }
}

/// Flush the new file, then recheck the original immediately before atomic replacement.
fn publish(
    file: &mut File,
    temporary: &Path,
    path: &Path,
    plan: &Plan,
    permissions: fs::Permissions,
) -> io::Result<()> {
    file.set_permissions(permissions)?;
    file.write_all(plan.after.as_bytes())?;
    file.sync_all()?;
    if fs::read(path)? != plan.before.as_bytes() {
        return Err(io::Error::other("source_conflict"));
    }
    fs::rename(temporary, path)
}

struct Temporary(PathBuf);

impl Drop for Temporary {
    /// Remove only the uniquely created temporary file, never the source or recovery backup.
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
