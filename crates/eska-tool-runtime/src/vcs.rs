//! Small typed calls to the VCS owner's private protocol, without a repository implementation.

use crate::{Component, Error, query};
use std::path::Path;

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
