//! A separately retained digest and exact tree fingerprints constrain every recovery move.

use super::{
    Digest, Error, ImportAncestor, Inventory, LayoutPublication, PROJECT_SOURCE_IMPORT_RECEIPT,
    Path, PathBuf, ProjectSourceImportRef, RECEIPT, Receipt, STATE, Sha256, SourceImportRecovery,
    State, checked_directory, files, fs, publication::present, read_optional, transaction,
    verify_settings,
};
use crate::project_core::{publish, source_format::project_migration::publish_new};

/// The three recoverable positions keep the previous tree reachable through every directory move.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Position {
    Prepared,
    BackedUp,
    Installed,
}

/// A loaded journal is verified against the caller's digest, not its own marker or directory name.
pub(super) struct Journal {
    root: PathBuf,
    directory: PathBuf,
    reference: ProjectSourceImportRef,
    receipt: Receipt,
}

/// Resolve a guarded import without needing discovery or the original incoming staging process.
///
/// The exact old/selected source, persisted ancestors, approved pin and settings are checked first.
/// Changed, missing or foreign data blocks both completion and rollback without writing anything.
///
/// # Errors
/// Returns an untrusted/incomplete journal, altered source/settings/support, unsafe paths or I/O.
pub fn recover_project_source_import(
    root: &Path,
    expected: &ProjectSourceImportRef,
    action: SourceImportRecovery,
    cancelled: &dyn Fn() -> bool,
) -> Result<LayoutPublication, Error> {
    files::check_cancelled(cancelled)?;
    Journal::open(root, expected, cancelled)?.resolve(action, cancelled)
}

impl Journal {
    /// Reconstruct only a canonical journal whose immutable receipt matches an independent digest.
    pub(super) fn open(
        root: &Path,
        expected: &ProjectSourceImportRef,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Self, Error> {
        ProjectSourceImportRef::from_sha256(&expected.sha256)?;
        checked_directory(root)?;
        let directory = transaction(root, expected);
        checked_directory(&directory)?;
        let text = read_optional(&directory.join(RECEIPT))?
            .ok_or_else(|| Error::root("source_import_receipt_invalid"))?;
        if files::hex(&Sha256::digest(text.as_bytes())) != expected.sha256 {
            return Err(Error::root("source_import_receipt_invalid"));
        }
        let receipt: Receipt = serde_json::from_str(&text)
            .map_err(|_| Error::root("source_import_receipt_invalid"))?;
        if receipt.schema_version != 1 {
            return Err(Error::root("source_import_receipt_invalid"));
        }
        let journal = Self {
            root: root.into(),
            directory,
            reference: expected.clone(),
            receipt,
        };
        journal.check_trees(cancelled)?;
        journal.guarded()?;
        Ok(journal)
    }

    /// The guard is exclusive and may name only this independently selected transaction.
    pub(super) fn guarded(&self) -> Result<bool, Error> {
        let Some(text) = read_optional(&self.root.join(PROJECT_SOURCE_IMPORT_RECEIPT))? else {
            return Ok(false);
        };
        let reference: ProjectSourceImportRef = serde_json::from_str(&text)
            .map_err(|_| Error::root("source_import_receipt_invalid"))?;
        if reference != self.reference {
            return Err(Error::root("source_import_receipt_invalid"));
        }
        Ok(true)
    }

    /// Completion keeps the old source; rollback puts selected bytes back into candidate staging.
    pub(super) fn resolve(
        &self,
        action: SourceImportRecovery,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<LayoutPublication, Error> {
        self.check_trees(cancelled)?;
        if !self.guarded()? {
            return match action {
                SourceImportRecovery::Complete if self.is_complete(cancelled)? => {
                    Ok(LayoutPublication::Unchanged)
                }
                SourceImportRecovery::RollBack
                    if self.position(cancelled)? == Position::Prepared
                        && read_optional(&self.root.join(STATE))?
                            == self.receipt.previous_state =>
                {
                    Ok(LayoutPublication::Unchanged)
                }
                _ => Err(Error::root("source_import_recovery_required")),
            };
        }
        files::check_cancelled(cancelled)?;
        match action {
            SourceImportRecovery::Complete => self.complete(cancelled)?,
            SourceImportRecovery::RollBack => self.roll_back(cancelled)?,
        }
        self.check_trees(cancelled)?;
        files::check_cancelled(cancelled)?;
        if !self.guarded()? {
            return Err(Error::root("source_import_receipt_invalid"));
        }
        fs::remove_file(self.root.join(PROJECT_SOURCE_IMPORT_RECEIPT))
            .map_err(|error| files::io(&error))?;
        Ok(LayoutPublication::Resumed)
    }

    /// An absent guard is success only with both the selected source and the exact new state pointer.
    pub(super) fn is_complete(&self, cancelled: &dyn Fn() -> bool) -> Result<bool, Error> {
        Ok(!self.guarded()?
            && self.position(cancelled)? == Position::Installed
            && read_optional(&self.root.join(STATE))?.as_deref()
                == Some(self.next_state()?.as_str()))
    }

    /// Retained history remains exact; arbitrary extra journal entries never authorize recovery.
    pub(super) fn check_trees(&self, cancelled: &dyn Fn() -> bool) -> Result<(), Error> {
        verify_settings(&self.root, &self.receipt, cancelled)?;
        checked_directory(&self.directory)?;
        let text = read_optional(&self.directory.join(RECEIPT))?
            .ok_or_else(|| Error::root("source_import_receipt_invalid"))?;
        if files::hex(&Sha256::digest(text.as_bytes())) != self.reference.sha256 {
            return Err(Error::root("source_import_receipt_invalid"));
        }
        for entry in fs::read_dir(&self.directory).map_err(|error| files::io(&error))? {
            files::check_cancelled(cancelled)?;
            let entry = entry.map_err(|error| files::io(&error))?;
            let name = entry.file_name();
            if ![
                RECEIPT,
                "ancestor",
                "incoming",
                "candidate",
                "previous-source",
            ]
            .iter()
            .any(|allowed| name == std::ffi::OsStr::new(allowed))
            {
                return Err(Error::root("source_import_output_mismatch"));
            }
        }
        ImportAncestor::open_cancellable(
            &self.directory.join("ancestor"),
            &self.receipt.ancestor,
            cancelled,
        )?;
        ImportAncestor::open_cancellable(
            &self.directory.join("incoming"),
            &self.receipt.incoming,
            cancelled,
        )?;
        self.position(cancelled)?;
        let state = read_optional(&self.root.join(STATE))?;
        if state != self.receipt.previous_state
            && state.as_deref() != Some(self.next_state()?.as_str())
        {
            return Err(Error::root("source_import_state_invalid"));
        }
        Ok(())
    }

    /// Source is either untouched, moved to backup, or replaced by the fully staged candidate.
    fn position(&self, cancelled: &dyn Fn() -> bool) -> Result<Position, Error> {
        let source = revision(&self.root.join(self.receipt.source.as_path()), cancelled)?;
        let candidate = revision(&self.directory.join("candidate"), cancelled)?;
        let backup = revision(&self.directory.join("previous-source"), cancelled)?;
        let old = Some(self.receipt.working_revision.as_str());
        let output = Some(self.receipt.output_revision.as_str());
        match (source.as_deref(), candidate.as_deref(), backup.as_deref()) {
            (source, candidate, None) if source == old && candidate == output => {
                Ok(Position::Prepared)
            }
            (None, candidate, backup) if candidate == output && backup == old => {
                Ok(Position::BackedUp)
            }
            (source, None, backup) if source == output && backup == old => Ok(Position::Installed),
            _ => Err(Error::root("source_import_output_mismatch")),
        }
    }

    /// Both directory moves retain old bytes, and state advances only after selected source verifies.
    fn complete(&self, cancelled: &dyn Fn() -> bool) -> Result<(), Error> {
        if self.position(cancelled)? == Position::Prepared {
            self.move_tree(
                &self.root.join(self.receipt.source.as_path()),
                &self.directory.join("previous-source"),
                cancelled,
            )?;
        }
        if self.position(cancelled)? == Position::BackedUp {
            self.move_tree(
                &self.directory.join("candidate"),
                &self.root.join(self.receipt.source.as_path()),
                cancelled,
            )?;
        }
        self.check_trees(cancelled)?;
        files::check_cancelled(cancelled)?;
        self.set_state(
            self.receipt.previous_state.as_deref(),
            Some(&self.next_state()?),
        )
    }

    /// A changed installed source cannot be silently exchanged for the old backup during rollback.
    fn roll_back(&self, cancelled: &dyn Fn() -> bool) -> Result<(), Error> {
        if self.position(cancelled)? == Position::Installed {
            self.move_tree(
                &self.root.join(self.receipt.source.as_path()),
                &self.directory.join("candidate"),
                cancelled,
            )?;
        }
        if self.position(cancelled)? == Position::BackedUp {
            self.move_tree(
                &self.directory.join("previous-source"),
                &self.root.join(self.receipt.source.as_path()),
                cancelled,
            )?;
        }
        self.check_trees(cancelled)?;
        files::check_cancelled(cancelled)?;
        self.set_state(
            Some(&self.next_state()?),
            self.receipt.previous_state.as_deref(),
        )
    }

    /// Cooperative writers honor the guard; filesystem aliases and preexisting destinations refuse.
    fn move_tree(
        &self,
        source: &Path,
        destination: &Path,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), Error> {
        self.check_trees(cancelled)?;
        files::check_cancelled(cancelled)?;
        checked_directory(source)?;
        checked_directory(
            destination
                .parent()
                .ok_or_else(|| Error::root("source_path_invalid"))?,
        )?;
        if present(destination)? || !self.guarded()? {
            return Err(Error::root("source_import_output_mismatch"));
        }
        fs::rename(source, destination).map_err(|error| files::io(&error))
    }

    /// One exact pointer selects raw incoming ancestry, even when the merged tree retains local edits.
    fn next_state(&self) -> Result<String, Error> {
        serde_json::to_string(&State {
            schema_version: 1,
            source: self.receipt.source.clone(),
            transaction: self.reference.clone(),
            ancestor: self.receipt.incoming.clone(),
        })
        .map_err(|_| Error::root("source_schema_invalid"))
    }

    /// Expected-byte publication makes an unexpected synchronization pointer a recovery conflict.
    fn set_state(&self, previous: Option<&str>, next: Option<&str>) -> Result<(), Error> {
        let current = read_optional(&self.root.join(STATE))?;
        if current.as_deref() == next {
            return Ok(());
        }
        if current.as_deref() != previous {
            return Err(Error::root("source_import_state_invalid"));
        }
        match (previous, next) {
            (Some(old), Some(new)) => {
                publish::publish_snapshot(&self.root, Path::new(STATE), old, new)
                    .map_err(|_| Error::root("source_input_changed"))
            }
            (None, Some(new)) => publish_new(&self.root, STATE, new),
            (Some(_), None) => {
                fs::remove_file(self.root.join(STATE)).map_err(|error| files::io(&error))
            }
            (None, None) => Ok(()),
        }
    }
}

/// Optional trees are hashed only through canonical directories, never through an alias.
fn revision(path: &Path, cancelled: &dyn Fn() -> bool) -> Result<Option<String>, Error> {
    if !present(path)? {
        checked_directory(
            path.parent()
                .ok_or_else(|| Error::root("source_path_invalid"))?,
        )?;
        return Ok(None);
    }
    checked_directory(path)?;
    Inventory::read_cancellable(path, cancelled).map(|inventory| Some(inventory.revision))
}
