//! A prepared input keeps its private proof; a mutable export report is not that proof.

use std::{fs, path::Path};

use super::{Exclusion, ProtectedExport, SourceMap, generation::Binding};
use crate::project_core::source_format::{
    Error, FileEntry,
    files::{self, Inventory},
};

/// Native bytes produced by one verified protected export, with an immutable input identity.
///
/// Construction belongs to protected materialization. A consumer must validate the actual
/// copied snapshot before invoking the platform; a path or a serialized report is insufficient.
#[derive(Debug)]
pub struct PreparedSource {
    pub(super) report: ProtectedExport,
    pub(super) binding: Binding,
    pub(super) native: Inventory,
    pub(super) source_map: SourceMap,
}

impl PreparedSource {
    /// Return origins derived from the same selected bytes as this prepared generation.
    #[must_use]
    pub const fn source_map(&self) -> &SourceMap {
        &self.source_map
    }

    /// Return the completed generation's native directory, never a mutable fallback directory.
    #[must_use]
    pub fn xml(&self) -> &Path {
        &self.report.xml
    }

    /// Return the identity that binds policy, working input and generated native bytes.
    #[must_use]
    pub fn generation(&self) -> &str {
        &self.report.generation
    }

    /// Return the exact native file-tree revision expected in a build snapshot.
    #[must_use]
    pub fn native_revision(&self) -> &str {
        &self.native.revision
    }

    /// Return the source-tree revision captured before protected selection.
    #[must_use]
    pub fn input_revision(&self) -> &str {
        &self.report.input_revision
    }

    /// Return the caller-pinned accepted-base identity used to authorize export.
    #[must_use]
    pub fn baseline(&self) -> &str {
        &self.report.baseline
    }

    /// Describe every native payload's expected bytes without rereading the generation.
    pub fn files(&self) -> impl Iterator<Item = &FileEntry> {
        self.native.files.values()
    }

    /// Return restored or excluded working changes for the caller's presentation layer.
    #[must_use]
    pub fn diagnostics(&self) -> &[Exclusion] {
        &self.report.diagnostics
    }

    /// Return presentation data whose subsequent modification cannot alter the private proof.
    #[must_use]
    pub fn report(&self) -> ProtectedExport {
        self.report.clone()
    }

    /// Recheck the completed marker and every native byte before opening build resources.
    ///
    /// # Errors
    /// Returns a structured failure for aliases, a changed marker or a changed native tree.
    pub fn validate(&self) -> Result<(), Error> {
        let root = self
            .xml()
            .parent()
            .ok_or_else(|| Error::root("source_generation_corrupt"))?;
        require_directories(self.xml())?;
        let path = root.join("generation.json");
        if !fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_file()) {
            return Err(Error::root("source_generation_corrupt"));
        }
        let binding: Binding = serde_json::from_str(&files::read_text(&path)?)
            .map_err(|_| Error::root("source_generation_corrupt"))?;
        if binding != self.binding || binding.id()? != self.generation() {
            return Err(Error::root("source_generation_corrupt"));
        }
        self.verify_tree(self.xml(), "source_generation_corrupt")
    }

    /// Verify the exact copied tree that will be passed to the platform.
    ///
    /// # Errors
    /// Returns a structured failure for missing, additional, changed or aliased snapshot bytes.
    pub fn validate_snapshot(&self, root: &Path) -> Result<(), Error> {
        require_directories(root)?;
        self.verify_tree(root, "source_snapshot_changed")
    }

    /// Compare complete inventories, including empty directories and orphan payloads.
    fn verify_tree(&self, root: &Path, code: &'static str) -> Result<(), Error> {
        if Inventory::read(root)? != self.native {
            return Err(Error::root(code));
        }
        Ok(())
    }
}

/// Reject ancestor aliases as well as a link replacing the root itself.
fn require_directories(root: &Path) -> Result<(), Error> {
    if !root.is_absolute() {
        return Err(Error::root("source_path_invalid"));
    }
    for ancestor in root.ancestors() {
        if !fs::symlink_metadata(ancestor)
            .map_err(|error| files::io(&error))?
            .file_type()
            .is_dir()
        {
            return Err(Error::root("source_entry_unsupported"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
