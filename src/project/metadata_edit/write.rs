//! Legacy XML edits use the shared exact-byte publication boundary.

use std::path::Path;

use super::{EditError, EditPlan};
use crate::project_core::publish::{self, PublishError};

/// Preserve legacy domain variants while sharing the single filesystem implementation.
fn error(error: PublishError) -> EditError {
    match error {
        PublishError::Conflict => EditError::Conflict,
        PublishError::UnsafePath => EditError::UnsafePath,
        PublishError::ReadOnly => EditError::ReadOnly,
        PublishError::Io(source) => EditError::Io(source),
    }
}

impl EditPlan {
    /// Recheck expected bytes and permissions before replacing the file atomically.
    ///
    /// # Errors
    /// Returns a conflict, unsafe path, read-only source or filesystem error.
    pub fn publish(&self, root: &Path, relative: &Path) -> Result<(), EditError> {
        publish::publish_snapshot(root, relative, &self.original, &self.updated).map_err(error)
    }
}

/// Structural journals share the same publication checks in their owned staging directory.
pub fn publish_snapshot_in(
    root: &Path,
    relative: &Path,
    original: &str,
    updated: &str,
    staging: &Path,
) -> Result<(), EditError> {
    publish::publish_snapshot_in(root, relative, original, updated, staging).map_err(error)
}
