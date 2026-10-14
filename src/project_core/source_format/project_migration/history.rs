//! A layout no-op retains raw ancestry; a representation change cannot invent new common history.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use super::validate_distinct_paths;
use crate::project_core::source_format::{
    AcceptedBase, Error, PROJECT_SOURCE_IMPORT_DIRECTORY, SourcePath,
    files::{self, Inventory},
    layout::migration::publication,
    synchronization::verified_project_import_history,
};

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub(super) struct RawHistory {
    pub path: SourcePath,
    root: PathBuf,
    inventory: Inventory,
    text: BTreeMap<SourcePath, String>,
}

impl RawHistory {
    /// Absence stays absent; changing source or base representation with history is an explicit refusal.
    pub(super) fn prepare(
        root: &Path,
        source: &SourcePath,
        base: &SourcePath,
        approved: &AcceptedBase,
        layout_changed: bool,
    ) -> Result<Option<Self>, Error> {
        let path = SourcePath::parse(PROJECT_SOURCE_IMPORT_DIRECTORY)?;
        let directory = root.join(path.as_path());
        match fs::symlink_metadata(&directory) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(files::io(&error)),
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => return Err(Error::root("source_entry_unsupported")),
        }
        validate_distinct_paths(source, &path)?;
        validate_distinct_paths(base, &path)?;
        let inventory = verified_project_import_history(root, source, approved)?;
        if layout_changed {
            return Err(Error::root("source_migration_ancestry_unsupported"));
        }
        Ok(Some(Self {
            path,
            root: directory,
            inventory,
            text: BTreeMap::new(),
        }))
    }

    /// The receipt binds all state, stores, backups and journals by their exact bytes and hierarchy.
    pub(super) fn revision(&self) -> &str {
        &self.inventory.revision
    }

    /// Existing publication primitives stream the unchanged archive and publish state.json last.
    pub(super) fn view(&self) -> publication::View<'_> {
        publication::View {
            root: &self.root,
            input: &self.inventory,
            output: &self.inventory,
            text: &self.text,
            ready: "state.json",
            renamed: None,
        }
    }
}
