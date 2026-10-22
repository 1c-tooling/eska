//! Small typed calls to the VCS owner's private protocol, without a repository implementation.

use crate::{Component, Error, query};
use std::{ffi::OsString, path::Path};

/// Recheck only selected source paths using the same preflighted executable throughout a transaction.
///
/// # Errors
/// Returns cancellation, repository failures and invalid counts without claiming a clean source.
pub fn source_changes(
    binary: &Path,
    root: &Path,
    empty: bool,
    files: usize,
) -> Result<usize, Error> {
    let mut options = vec![OsString::from("--files"), OsString::from(files.to_string())];
    if empty {
        options.push(OsString::from("--empty-scaffold"));
    }
    query(Component::Vcs, binary, "source_changes", root, &options)?
        .get("local_changes")
        .and_then(serde_json::Value::as_u64)
        .and_then(|count| usize::try_from(count).ok())
        .ok_or(Error::InvalidResponse)
}

/// Inspect the nearest on-disk repository before the caller starts a file transaction.
///
/// # Errors
/// Preserves component, malformed-response and repository failures instead of claiming absence.
pub fn exists(binary: &Path, root: &Path) -> Result<bool, Error> {
    query(Component::Vcs, binary, "repository_exists", root, &[])?
        .get("exists")
        .and_then(serde_json::Value::as_bool)
        .ok_or(Error::InvalidResponse)
}

/// Initialize Git inside a newly claimed project; the caller owns rollback of that directory.
///
/// # Errors
/// Rejects existing metadata, component failures and incomplete initialization responses.
pub fn initialize_new(binary: &Path, root: &Path) -> Result<(), Error> {
    initialized(binary, root, "initialize")
}

/// Populate only the caller's exclusively reserved empty `.git` directory.
///
/// # Errors
/// Rejects nonempty metadata and returns component/initialization failures without fallback.
pub fn initialize_reserved(binary: &Path, git: &Path) -> Result<(), Error> {
    initialized(binary, git, "initialize_reserved")
}

/// Require an affirmative complete response before accepting Git initialization as successful.
fn initialized(binary: &Path, root: &Path, operation: &str) -> Result<(), Error> {
    let data = query(Component::Vcs, binary, operation, root, &[])?;
    if data.get("initialized").and_then(serde_json::Value::as_bool) != Some(true) {
        return Err(Error::InvalidResponse);
    }
    Ok(())
}
