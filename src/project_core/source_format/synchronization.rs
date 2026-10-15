//! Confirmed import ancestry stores source bytes without becoming an approved support baseline.

mod import;
mod storage;

pub(in crate::project_core::source_format) use import::ProjectSettingsGuard;
pub(in crate::project_core::source_format) use import::stage_project_import_ancestor;
pub(in crate::project_core::source_format) use import::verified_project_import_history;
pub use import::verify_project_import_history;
pub use import::{
    NativeProjectSourceImport, PROJECT_SOURCE_IMPORT_RECEIPT, ProjectSourceImportPlan,
    ProjectSourceImportRef, SourceImportChange, SourceImportConflict, SourceImportConflictKind,
    SourceImportPlan, SourceImportRecovery, prepare_project_native_source_import,
    prepare_project_source_import, prepare_source_import, project_import_ancestor,
    recover_project_source_import,
};
pub use import::{
    PROJECT_SOURCE_IMPORT_DIRECTORY, initialize_project_import_ancestor,
    open_initial_project_import_ancestor,
};

#[cfg(test)]
mod tests;

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    Error, FileEntry, SourcePath,
    export::Package,
    files::{self, Inventory},
};

const MARKER: &str = "ancestor.json";

/// This reference confirms source ancestry only; it cannot be passed as an accepted-base pin.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportAncestorRef {
    sha256: String,
}

impl ImportAncestorRef {
    /// Parse an independently selected reference instead of trusting a store's own advertised hash.
    ///
    /// # Errors
    /// Returns an invalid reference for anything other than a lowercase SHA-256 digest.
    pub fn from_sha256(sha256: &str) -> Result<Self, Error> {
        if !valid_hash(sha256) {
            return Err(Error::root("source_ancestor_pin_invalid"));
        }
        Ok(Self {
            sha256: sha256.to_owned(),
        })
    }

    /// Return the confirmed source reference without confusing it with a support approval.
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

/// A captured ancestor is immutable source data; native rules remain data inside the snapshot.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    source_revision: String,
    files: Vec<FileEntry>,
    directories: Vec<SourcePath>,
}

/// Verified complete ancestry is separate from editable files and the current support authority.
pub struct ImportAncestor {
    root: PathBuf,
    reference: ImportAncestorRef,
    storage: Inventory,
    pub(super) package: Package,
}

/// Capture an explicitly chosen complete source snapshot and return a candidate ancestry reference.
///
/// The caller must establish that these bytes are the common synchronized ancestor. This function
/// never infers ancestry from the current working tree and never approves supplier rules or freeze.
///
/// # Errors
/// Returns invalid source, unsafe or conflicting output, changed input or storage failures.
pub fn capture_import_ancestor(
    source: &Path,
    destination: &Path,
) -> Result<ImportAncestorRef, Error> {
    let package = Package::read(source)?;
    package.native_xml()?;
    let source = source.canonicalize().map_err(|error| files::io(&error))?;
    let manifest = Manifest {
        schema_version: 1,
        source_revision: package.snapshot.revision.clone(),
        files: package.snapshot.files.values().cloned().collect(),
        directories: package.snapshot.directories.clone(),
    };
    let bytes =
        serde_json::to_vec_pretty(&manifest).map_err(|_| Error::root("source_schema_invalid"))?;
    let reference = ImportAncestorRef {
        sha256: files::hex(&Sha256::digest(&bytes)),
    };
    let destination = storage::destination(&source, destination)?;
    match fs::symlink_metadata(&destination) {
        Ok(metadata) if metadata.file_type().is_dir() => {
            ImportAncestor::open(&destination, &reference)?;
            package.recheck(&source)?;
            return Ok(reference);
        }
        Ok(_) => return Err(Error::root("source_destination_exists")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(files::io(&error)),
    }
    fs::create_dir(&destination).map_err(|error| files::io(&error))?;
    let result = storage::persist(&source, &destination, &package.snapshot, &bytes)
        .and_then(|()| package.recheck(&source))
        .and_then(|()| ImportAncestor::open(&destination, &reference).map(|_| ()));
    if let Err(error) = result {
        fs::remove_dir_all(&destination).map_err(|_| Error::root("source_cleanup_failed"))?;
        return Err(error);
    }
    Ok(reference)
}

impl ImportAncestor {
    /// Verify the independently confirmed marker, exact blob set and complete source projection.
    ///
    /// # Errors
    /// Refuses a wrong pin, malformed snapshot, altered/missing/extra blobs or unsafe entries.
    pub fn open(root: &Path, expected: &ImportAncestorRef) -> Result<Self, Error> {
        Self::open_cancellable(root, expected, &|| false)
    }

    /// A cancelled read never creates a partly verified ancestor usable for an import plan.
    ///
    /// # Errors
    /// Returns the same verification failures as open, or an explicit source-read cancellation.
    pub fn open_cancellable(
        root: &Path,
        expected: &ImportAncestorRef,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Self, Error> {
        if !valid_hash(&expected.sha256) {
            return Err(Error::root("source_ancestor_pin_invalid"));
        }
        let inventory = Inventory::read_cancellable(root, cancelled)?;
        let text =
            files::read_text_cancellable(&root.join(MARKER), files::MAX_TEXT_BYTES, cancelled)?;
        if files::hex(&Sha256::digest(text.as_bytes())) != expected.sha256 {
            return Err(Error::root("source_ancestor_untrusted"));
        }
        let manifest: Manifest =
            serde_json::from_str(&text).map_err(|_| Error::root("source_ancestor_corrupt"))?;
        let snapshot = storage::verify(&manifest, &inventory)?;
        let entries = snapshot.files.iter().collect::<BTreeMap<_, _>>();
        let package = Package::from_snapshot(storage::clone_inventory(&snapshot), |path| {
            files::check_cancelled(cancelled)?;
            let entry = entries
                .get(path)
                .ok_or_else(|| Error::root("source_file_undeclared"))?;
            files::read_text_cancellable(
                &root.join(storage::blob_path(entry)?.as_path()),
                files::MAX_TEXT_BYTES,
                cancelled,
            )
        })?;
        package.native_xml()?;
        if Inventory::read_cancellable(root, cancelled)? != inventory {
            return Err(Error::root("source_input_changed"));
        }
        files::check_cancelled(cancelled)?;
        Ok(Self {
            root: root.canonicalize().map_err(|error| files::io(&error))?,
            reference: expected.clone(),
            storage: inventory,
            package,
        })
    }

    /// Return the confirmed ancestry reference, which grants no protected-export permission.
    #[must_use]
    pub const fn reference(&self) -> &ImportAncestorRef {
        &self.reference
    }

    /// Return the byte-exact source revision used by three-way comparison.
    #[must_use]
    pub fn revision(&self) -> &str {
        &self.package.snapshot.revision
    }

    /// Recheck all persisted bytes before an operation acts on a previously opened ancestor.
    ///
    /// # Errors
    /// Refuses changed, added, removed or unsupported storage entries.
    pub fn verify(&self) -> Result<(), Error> {
        if Inventory::read(&self.root)? != self.storage {
            return Err(Error::root("source_input_changed"));
        }
        Ok(())
    }

    /// Restore source bytes for a selected import; this does not create or replace an accepted base.
    ///
    /// # Errors
    /// Returns changed ancestry, conflicting output, overlap, unsafe paths or I/O failure.
    pub fn restore(&self, destination: &Path) -> Result<(), Error> {
        storage::restore(self, destination)
    }
}

/// References and blob filenames use one canonical spelling across OS and locale.
fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
