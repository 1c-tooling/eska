//! Complete staging precedes the project guard; guarded directory moves preserve the old source.

use super::{
    Error, ImportAncestor, Inventory, LayoutPublication, PROJECT_SOURCE_IMPORT_RECEIPT, Path,
    ProjectSourceImportPlan, RECEIPT, SERVICE, SourceImportRecovery, checked_directory, files, fs,
    recovery::Journal, require_project_ready, transaction,
};
use crate::project_core::source_format::project_migration::publish_new;

impl ProjectSourceImportPlan {
    /// Publish into the selected project, retaining old source and both ancestors in a guarded journal.
    ///
    /// No accepted storage, manifest, Git state or build artifact is modified. This protocol recovers
    /// process interruption; directory renames are not a power-loss or noncooperating-writer transaction.
    ///
    /// # Errors
    /// Returns conflicts, stale settings/data, unsafe journals, cancellation or filesystem failures.
    pub fn publish(&self) -> Result<LayoutPublication, Error> {
        self.publish_cancellable(&|| false)
    }

    /// Cancellation before the guard leaves source intact; cancellation after it requires recovery.
    ///
    /// # Errors
    /// Returns publication failures or explicit cancellation, retaining guarded recovery data.
    pub fn publish_cancellable(
        &self,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<LayoutPublication, Error> {
        if !self.source.conflicts().is_empty() {
            return Err(Error::root("source_import_conflicts"));
        }
        files::check_cancelled(cancelled)?;
        if present(&self.root.join(PROJECT_SOURCE_IMPORT_RECEIPT))?
            || present(&transaction(&self.root, &self.reference).join(RECEIPT))?
        {
            let journal = Journal::open(&self.root, &self.reference, cancelled)?;
            if journal.is_complete(cancelled)? {
                return Ok(LayoutPublication::Unchanged);
            }
            if journal.guarded()? {
                return journal.resolve(SourceImportRecovery::Complete, cancelled);
            }
        }
        require_project_ready(&self.root)?;
        self.verify(cancelled)?;
        if self.unchanged_ancestor().is_some() {
            return Ok(LayoutPublication::Unchanged);
        }
        let created = !present(&transaction(&self.root, &self.reference))?;
        let journal = self.stage(cancelled)?;
        self.verify(cancelled)?;
        files::check_cancelled(cancelled)?;
        journal.check_trees(cancelled)?;
        let marker = serde_json::to_string(&self.reference)
            .map_err(|_| Error::root("source_schema_invalid"))?;
        publish_new(&self.root, PROJECT_SOURCE_IMPORT_RECEIPT, &marker)?;
        journal.resolve(SourceImportRecovery::Complete, cancelled)?;
        Ok(if created {
            LayoutPublication::Created
        } else {
            LayoutPublication::Resumed
        })
    }

    /// All bytes used after a crash are copied and verified before a project becomes unavailable.
    pub(super) fn stage(&self, cancelled: &dyn Fn() -> bool) -> Result<Journal, Error> {
        for relative in [".eska", SERVICE] {
            let path = self.root.join(relative);
            if !present(&path)? {
                fs::create_dir(&path).map_err(|error| files::io(&error))?;
            }
            checked_directory(&path)?;
        }
        let directory = transaction(&self.root, &self.reference);
        if present(&directory)? {
            return Journal::open(&self.root, &self.reference, cancelled);
        }
        fs::create_dir(&directory).map_err(|error| files::io(&error))?;
        let result = self.stage_bytes(&directory, cancelled);
        if let Err(error) = result {
            fs::remove_dir_all(&directory).map_err(|_| Error::root("source_cleanup_failed"))?;
            return Err(error);
        }
        Journal::open(&self.root, &self.reference, cancelled)
    }

    /// Incoming ancestry is the incoming snapshot, never the merged source with local edits.
    fn stage_bytes(&self, directory: &Path, cancelled: &dyn Fn() -> bool) -> Result<(), Error> {
        for (name, input, reference) in [
            ("ancestor", &self.source.ancestor, &self.receipt.ancestor),
            ("incoming", &self.source.incoming, &self.receipt.incoming),
        ] {
            let target = directory.join(name);
            fs::create_dir(&target).map_err(|error| files::io(&error))?;
            copy_store(&input.root, &target, &input.inventory, cancelled)?;
            ImportAncestor::open_cancellable(&target, reference, cancelled)?;
        }
        self.source
            .publish_source_cancellable(&directory.join("candidate"), cancelled)?;
        self.verify(cancelled)?;
        publish_new(directory, RECEIPT, &self.text)
    }
}

/// Ancestor storage is copied exactly, including its marker and deduplicated streamed resources.
fn copy_store(
    source: &Path,
    destination: &Path,
    inventory: &Inventory,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), Error> {
    for directory in &inventory.directories {
        files::check_cancelled(cancelled)?;
        fs::create_dir(destination.join(directory.as_path())).map_err(|error| files::io(&error))?;
    }
    for entry in inventory.files.values() {
        files::copy_to_cancellable(
            source,
            &destination.join(entry.path.as_path()),
            entry,
            cancelled,
        )?;
    }
    if Inventory::read_cancellable(source, cancelled)? != *inventory
        || Inventory::read_cancellable(destination, cancelled)? != *inventory
    {
        return Err(Error::root("source_input_changed"));
    }
    Ok(())
}

/// A dangling link or foreign directory is present; absence never means failure to inspect a path.
pub(super) fn present(path: &Path) -> Result<bool, Error> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(files::io(&error)),
    }
}
