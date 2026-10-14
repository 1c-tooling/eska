//! Exclusive project publication and recovery from independently resumable storage plans.

use std::{
    fs,
    path::{Path, PathBuf},
};

use super::super::{
    Error, LayoutPublication, SourcePath, files, layout::migration::publication as storage,
};
use super::{
    PROJECT_LAYOUT_RECEIPT, ProjectLayoutPlan, Receipt, ancillary, portable, publish_new,
    read_optional,
};

impl ProjectLayoutPlan {
    /// Publish or resume only this exact project plan, keeping the original and CI settings intact.
    ///
    /// Both nested outputs are checked before either is written. Interrupted output retains its
    /// matching receipt and is unavailable to ESKA consumers until project publication completes.
    /// No process-wide or power-loss transaction across the three markers is claimed.
    ///
    /// # Errors
    /// Returns changed input, foreign/edited output, unsafe filesystem entries or I/O failures.
    pub fn publish(&self) -> Result<LayoutPublication, Error> {
        self.recheck()?;
        let status = self.check_output()?;
        if status == LayoutPublication::Unchanged {
            return Ok(status);
        }
        if status == LayoutPublication::Created {
            fs::create_dir(&self.destination).map_err(|error| files::io(&error))?;
            let text = serde_json::to_string(&self.receipt)
                .map_err(|_| Error::root("source_schema_invalid"))?;
            publish_new(&self.destination, PROJECT_LAYOUT_RECEIPT, &text)?;
        }
        for path in self.storage_paths() {
            if let Some(parent) = path.as_path().parent() {
                fs::create_dir_all(self.destination.join(parent))
                    .map_err(|error| files::io(&error))?;
            }
        }
        storage::preflight(self.source.publication_view(), &self.source_destination())?;
        storage::preflight(self.base.publication_view(), &self.base_destination())?;
        if let Some(history) = &self.history {
            storage::preflight(
                history.view(),
                &self.destination.join(history.path.as_path()),
            )?;
        }
        self.source.publish(&self.source_destination())?;
        self.base.publish(&self.base_destination())?;
        if let Some(history) = &self.history {
            storage::publish(
                history.view(),
                &self.destination.join(history.path.as_path()),
            )?;
        }
        self.recheck()?;
        for (path, text) in &self.ancillary {
            if path.as_str() != "eska.toml"
                && let Some(text) = text
            {
                self.publish_ancillary(path, text)?;
            }
        }
        self.recheck()?;
        let manifest = SourcePath::parse("eska.toml")?;
        let text = self
            .ancillary
            .get(&manifest)
            .and_then(Option::as_deref)
            .ok_or_else(|| Error::root("source_file_missing"))?;
        self.publish_ancillary(&manifest, text)?;
        self.check_output()?;
        self.recheck()?;
        fs::remove_file(self.destination.join(PROJECT_LAYOUT_RECEIPT))
            .map_err(|error| files::io(&error))?;
        if self.check_output()? != LayoutPublication::Unchanged {
            return Err(Error::root("source_layout_destination_mismatch"));
        }
        Ok(status)
    }

    /// A root receipt never grants permission to overwrite an existing entry.
    pub(super) fn check_output(&self) -> Result<LayoutPublication, Error> {
        match fs::symlink_metadata(&self.destination) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(LayoutPublication::Created);
            }
            Err(error) => return Err(files::io(&error)),
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => return Err(Error::root("source_destination_exists")),
        }
        let pending = read_optional(&self.destination.join(PROJECT_LAYOUT_RECEIPT))?;
        let status = if let Some(text) = pending {
            let stored: Receipt = serde_json::from_str(&text)
                .map_err(|_| Error::root("source_layout_receipt_invalid"))?;
            if stored != self.receipt {
                return Err(Error::root("source_layout_receipt_invalid"));
            }
            LayoutPublication::Resumed
        } else {
            LayoutPublication::Unchanged
        };
        self.check_entries(Path::new(""))?;
        let outputs = [
            (self.source.publication_view(), self.source_destination()),
            (self.base.publication_view(), self.base_destination()),
        ];
        let history = self.history.as_ref().map(|history| {
            (
                history.view(),
                self.destination.join(history.path.as_path()),
            )
        });
        for (view, destination) in outputs.into_iter().chain(history) {
            if destination.parent().is_some_and(Path::is_dir) {
                let (_, nested) = storage::preflight(view, &destination)?;
                if status == LayoutPublication::Unchanged && nested != LayoutPublication::Unchanged
                {
                    return Err(Error::root("source_destination_exists"));
                }
            } else if status == LayoutPublication::Unchanged {
                return Err(Error::root("source_destination_exists"));
            }
        }
        if status == LayoutPublication::Unchanged && ancillary(&self.destination)? != self.ancillary
        {
            return Err(Error::root("source_destination_exists"));
        }
        Ok(status)
    }

    /// Inspect only migration-owned paths; foreign Git state and project files refuse resumption.
    fn check_entries(&self, relative: &Path) -> Result<(), Error> {
        for entry in
            fs::read_dir(self.destination.join(relative)).map_err(|error| files::io(&error))?
        {
            let entry = entry.map_err(|error| files::io(&error))?;
            let path = relative.join(entry.file_name());
            let kind = entry.file_type().map_err(|error| files::io(&error))?;
            if kind.is_dir() && self.storage_paths().any(|target| target.as_path() == path) {
                continue;
            }
            if kind.is_dir()
                && self
                    .storage_paths()
                    .any(|target| target.as_path().starts_with(&path))
            {
                self.check_entries(&path)?;
            } else if kind.is_file() && path == Path::new(PROJECT_LAYOUT_RECEIPT) {
                // The receipt was parsed and matched before inspecting individual entries.
            } else if kind.is_file()
                && self
                    .ancillary
                    .get(&portable(&path)?)
                    .is_some_and(Option::is_some)
            {
                if read_optional(&entry.path())? != self.ancillary[&portable(&path)?] {
                    return Err(Error::root("source_layout_destination_mismatch"));
                }
            } else {
                return Err(Error::root("source_layout_destination_mismatch"));
            }
        }
        Ok(())
    }

    /// Publication is exclusive; a previous exact file is a no-op.
    fn publish_ancillary(&self, path: &SourcePath, text: &str) -> Result<(), Error> {
        match read_optional(&self.destination.join(path.as_path()))? {
            None => publish_new(&self.destination, path.as_str(), text)?,
            Some(current) if current == text => {}
            Some(_) => return Err(Error::root("source_layout_destination_mismatch")),
        }
        Ok(())
    }

    /// Native paths in project settings remain unchanged in the new root.
    pub(super) fn source_destination(&self) -> PathBuf {
        self.destination.join(self.source_path.as_path())
    }

    /// Authority always uses the configured independent base directory.
    pub(super) fn base_destination(&self) -> PathBuf {
        self.destination.join(self.base_path.as_path())
    }

    /// Only declared storage roots and their parents belong to the resumable project copy.
    fn storage_paths(&self) -> impl Iterator<Item = &SourcePath> {
        [&self.source_path, &self.base_path]
            .into_iter()
            .chain(self.history.as_ref().map(|history| &history.path))
    }
}
