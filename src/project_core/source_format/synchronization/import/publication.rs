//! Import candidates are reviewable new source trees; project replacement needs separate journaling.

use std::{fs, path::Path};

use super::{Error, Inventory, SelectedFile, SourceImportPlan, SourcePath, files, storage};
use crate::project_core::source_format::LayoutPublication;

impl SourceImportPlan {
    /// Publish the validated candidate without modifying working source, ancestry or accepted bytes.
    ///
    /// # Errors
    /// Refuses conflicts, stale inputs, overlaps, unsafe paths and nonidentical existing output.
    pub fn publish_source(&self, destination: &Path) -> Result<LayoutPublication, Error> {
        self.publish_source_cancellable(destination, &|| false)
    }

    /// Cancellation removes only this operation's unfinished new candidate, never an input tree.
    ///
    /// # Errors
    /// Returns publication validation failures or explicit caller cancellation.
    pub fn publish_source_cancellable(
        &self,
        destination: &Path,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<LayoutPublication, Error> {
        if !self.conflicts.is_empty() {
            return Err(Error::root("source_import_conflicts"));
        }
        self.verify_cancellable(cancelled)?;
        let destination = storage::destination(&self.working.root, destination)?;
        for input in [&self.ancestor, &self.incoming, &self.approved] {
            if destination.starts_with(&input.root) || input.root.starts_with(&destination) {
                return Err(Error::root("source_tree_overlap"));
            }
        }
        match fs::symlink_metadata(&destination) {
            Ok(metadata) if metadata.file_type().is_dir() => {
                if Inventory::read_cancellable(&destination, cancelled)? != self.output {
                    return Err(Error::root("source_import_output_mismatch"));
                }
                self.verify_cancellable(cancelled)?;
                return Ok(LayoutPublication::Unchanged);
            }
            Ok(_) => return Err(Error::root("source_destination_exists")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(files::io(&error)),
        }
        fs::create_dir(&destination).map_err(|error| files::io(&error))?;
        if let Err(error) = self.write_source(&destination, cancelled) {
            fs::remove_dir_all(&destination).map_err(|_| Error::root("source_cleanup_failed"))?;
            return Err(error);
        }
        Ok(LayoutPublication::Created)
    }

    /// Publish the unchanged reverse map last, after copying each selected blob without replacement.
    fn write_source(&self, destination: &Path, cancelled: &dyn Fn() -> bool) -> Result<(), Error> {
        for directory in &self.output.directories {
            files::check_cancelled(cancelled)?;
            fs::create_dir(destination.join(directory.as_path()))
                .map_err(|error| files::io(&error))?;
        }
        let marker = SourcePath::parse(files::MANIFEST)?;
        let ordered = self
            .selected
            .iter()
            .filter(|(path, _)| **path != marker)
            .chain(self.selected.get_key_value(&marker));
        for (path, choice) in ordered {
            let target = destination.join(path.as_path());
            match choice {
                SelectedFile::Working(entry) => {
                    files::copy_to_cancellable(&self.working.root, &target, entry, cancelled)?;
                }
                SelectedFile::Incoming(entry) => {
                    let mut stored = entry.clone();
                    stored.path = storage::blob_path(entry)?;
                    files::copy_to_cancellable(&self.incoming.root, &target, &stored, cancelled)?;
                }
            }
        }
        self.verify_cancellable(cancelled)?;
        if Inventory::read_cancellable(destination, cancelled)? != self.output {
            return Err(Error::root("source_input_changed"));
        }
        Ok(())
    }
}
