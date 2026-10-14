//! Exact plain edits use pinned ownership and the shared writer without changing resource links.

use std::path::Path;

use crate::project_core::{
    publish,
    source_format::{AcceptedBase, AcceptedRef, Error, Operation, SourcePath, files, validation},
};

use super::{SourceEditPlan, edit_error, permission, safe_root};

#[cfg(test)]
mod tests;

/// An unsaved preview binds exact replacement text to accepted ownership but cannot publish it.
#[derive(Debug)]
pub struct QueryEditPreview {
    path: SourcePath,
    baseline: AcceptedRef,
    updated: String,
    unchanged: bool,
}

impl QueryEditPreview {
    /// Return only a plain path declared by the independently accepted source map.
    #[must_use]
    pub const fn path(&self) -> &SourcePath {
        &self.path
    }

    /// The caller verifies this pin and its document revision before applying editor changes.
    #[must_use]
    pub const fn baseline(&self) -> &AcceptedRef {
        &self.baseline
    }

    /// Replacement bytes retain query spelling, comments and line endings exactly.
    #[must_use]
    pub fn updated(&self) -> &str {
        &self.updated
    }

    /// Byte equality needs no write and never normalizes query syntax or whitespace.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.unchanged
    }
}

/// Prepare a single declared plain file without modifying its owner or native reference.
///
/// The full export still validates actual working links and native scope independently.
/// No query syntax or consumer semantics are claimed by this storage-level edit.
///
/// # Errors
/// Returns undeclared/restricted ownership, invalid XML characters, unsafe path or I/O failures.
pub fn prepare_query_edit(
    root: &Path,
    base: &AcceptedBase,
    path: &SourcePath,
    updated: &str,
) -> Result<SourceEditPlan, Error> {
    let uuid = base
        .query_owner(path)
        .ok_or_else(|| Error::root("source_query_undeclared").in_field(path.as_str()))?;
    permission(base, uuid, Operation::Resource)?;
    let root = safe_root(root, path)?;
    let original = files::read_text(&root.join(path.as_path()))?;
    let preview = prepare_query_edit_text(base, path, &original, updated)?;
    publish::publish_snapshot(&root, path.as_path(), &original, &original).map_err(edit_error)?;
    Ok(SourceEditPlan {
        root,
        path: preview.path,
        uuid: uuid.clone(),
        operation: Operation::Resource,
        baseline: preview.baseline,
        original: Some(original),
        updated: preview.updated,
        source_binding: base.source_binding(),
    })
}

/// Prepare exact editor text without accessing mutable source files or granting write authority.
///
/// The caller checks document revision/hash and an independently verified base. Invalid current
/// XML characters may be repaired by a valid replacement; only the resulting value is validated.
///
/// # Errors
/// Returns undeclared/restricted ownership or characters that cannot round-trip through XML.
pub fn prepare_query_edit_text(
    base: &AcceptedBase,
    path: &SourcePath,
    original: &str,
    updated: &str,
) -> Result<QueryEditPreview, Error> {
    let uuid = base
        .query_owner(path)
        .ok_or_else(|| Error::root("source_query_undeclared").in_field(path.as_str()))?;
    permission(base, uuid, Operation::Resource)?;
    if updated.len() as u64 > files::MAX_TEXT_BYTES {
        return Err(Error::root("source_document_too_large").in_field(path.as_str()));
    }
    validation::xml_text(updated).map_err(|error| error.in_field(path.as_str()))?;
    Ok(QueryEditPreview {
        path: path.clone(),
        baseline: base.reference().clone(),
        updated: updated.into(),
        unchanged: original == updated,
    })
}
