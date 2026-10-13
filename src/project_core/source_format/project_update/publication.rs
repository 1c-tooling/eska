//! The project receipt binds both trees and prevents discovery of incomplete base updates.

use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{PROJECT_BASE_UPDATE_RECEIPT, ProjectBaseUpdatePlan, Receipt};
use crate::project_core::source_format::{
    Error, LayoutPublication, SourcePath, files,
    project_migration::{ancillary, portable, publish_new, read_optional},
};

impl ProjectBaseUpdatePlan {
    /// Publish or resume this exact project copy without modifying either incoming authority.
    ///
    /// The manifest is published last and an outer receipt keeps the result unavailable until all
    /// source, accepted and ancillary bytes match. This is not a power-loss transaction or CI approval.
    ///
    /// # Errors
    /// Refuses conflicts, stale inputs, changed/foreign output, unsafe paths and filesystem failures.
    pub fn publish(&self) -> Result<LayoutPublication, Error> {
        self.recheck()?;
        if !self.source.conflicts().is_empty() {
            return Err(Error::root("source_update_conflicts"));
        }
        let status = self.check_output()?;
        if status == LayoutPublication::Unchanged {
            return Ok(status);
        }
        if status == LayoutPublication::Created {
            fs::create_dir(&self.destination).map_err(|error| files::io(&error))?;
            let text = serde_json::to_string(&self.receipt)
                .map_err(|_| Error::root("source_schema_invalid"))?;
            publish_new(&self.destination, PROJECT_BASE_UPDATE_RECEIPT, &text)?;
        }
        for path in [&self.source_path, &self.base_path] {
            if let Some(parent) = path.as_path().parent() {
                fs::create_dir_all(self.destination.join(parent))
                    .map_err(|error| files::io(&error))?;
            }
        }
        self.source.check_source(&self.source_destination(), true)?;
        self.source.check_accepted(&self.base_destination())?;
        self.source
            .publish_pending_source(&self.source_destination())?;
        self.source.publish_accepted(&self.base_destination())?;
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
        fs::remove_file(self.destination.join(PROJECT_BASE_UPDATE_RECEIPT))
            .map_err(|error| files::io(&error))?;
        if self.check_output()? != LayoutPublication::Unchanged {
            return Err(Error::root("source_update_output_mismatch"));
        }
        Ok(status)
    }

    /// Neither an existing manifest nor a guessed receipt authorizes overwriting authoring bytes.
    pub(super) fn check_output(&self) -> Result<LayoutPublication, Error> {
        match fs::symlink_metadata(&self.destination) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(LayoutPublication::Created);
            }
            Err(error) => return Err(files::io(&error)),
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => return Err(Error::root("source_destination_exists")),
        }
        let status = if let Some(text) =
            read_optional(&self.destination.join(PROJECT_BASE_UPDATE_RECEIPT))?
        {
            let stored: Receipt = serde_json::from_str(&text)
                .map_err(|_| Error::root("source_update_receipt_invalid"))?;
            if stored != self.receipt {
                return Err(Error::root("source_update_receipt_invalid"));
            }
            LayoutPublication::Resumed
        } else {
            LayoutPublication::Unchanged
        };
        self.check_entries(Path::new(""))?;
        for (is_source, destination) in [
            (true, self.source_destination()),
            (false, self.base_destination()),
        ] {
            if destination.parent().is_some_and(Path::is_dir) {
                let nested = if is_source {
                    self.source
                        .check_source(&destination, status == LayoutPublication::Resumed)?
                } else {
                    self.source.check_accepted(&destination)?
                };
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

    /// Only the two planned trees, exact settings and operation marker may appear in this copy.
    fn check_entries(&self, relative: &Path) -> Result<(), Error> {
        for entry in
            fs::read_dir(self.destination.join(relative)).map_err(|error| files::io(&error))?
        {
            let entry = entry.map_err(|error| files::io(&error))?;
            let path = relative.join(entry.file_name());
            let kind = entry.file_type().map_err(|error| files::io(&error))?;
            if kind.is_dir()
                && [&self.source_path, &self.base_path]
                    .iter()
                    .any(|target| target.as_path() == path)
            {
                continue;
            }
            if kind.is_dir()
                && [&self.source_path, &self.base_path]
                    .iter()
                    .any(|target| target.as_path().starts_with(&path))
            {
                self.check_entries(&path)?;
            } else if kind.is_file() && path == Path::new(PROJECT_BASE_UPDATE_RECEIPT) {
                // The marker was already decoded and matched against this complete plan.
            } else if kind.is_file()
                && self
                    .ancillary
                    .get(&portable(&path)?)
                    .is_some_and(Option::is_some)
            {
                if read_optional(&entry.path())? != self.ancillary[&portable(&path)?] {
                    return Err(Error::root("source_update_output_mismatch"));
                }
            } else {
                return Err(Error::root("source_update_output_mismatch"));
            }
        }
        Ok(())
    }

    /// Ancillary publication remains an exclusive exact-byte write, including BOM and CRLF.
    fn publish_ancillary(&self, path: &SourcePath, text: &str) -> Result<(), Error> {
        match read_optional(&self.destination.join(path.as_path()))? {
            None => publish_new(&self.destination, path.as_str(), text),
            Some(current) if current == text => Ok(()),
            Some(_) => Err(Error::root("source_update_output_mismatch")),
        }
    }

    /// Source settings retain their original relative path in the separately published project.
    fn source_destination(&self) -> PathBuf {
        self.destination.join(self.source_path.as_path())
    }

    /// The new base is installed at the original manifest path with an independently selected pin.
    fn base_destination(&self) -> PathBuf {
        self.destination.join(self.base_path.as_path())
    }
}
