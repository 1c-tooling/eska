//! A workspace update publishes no member until every conflict and destination is checked.

use super::{
    Error, Path, WORKSPACE_BASE_UPDATE_RECEIPT, WorkspaceBaseUpdatePlan, WorkspaceReceipt,
    ancillary, files,
};
use crate::project_core::source_format::LayoutPublication;
use crate::project_core::source_format::project_migration::{portable, publish_new, read_optional};
use std::fs;

impl WorkspaceBaseUpdatePlan {
    /// Publish or resume the exact reviewed workspace, retaining original inputs and CI pins.
    ///
    /// Every member output is checked before the first member is published. The outer receipt
    /// blocks nested consumers until all member receipts are complete and root settings are ready.
    /// This is process recovery, not a filesystem transaction across members or power-loss safety.
    ///
    /// # Errors
    /// Returns stale inputs, foreign or edited outputs, unsafe filesystem entries or I/O failures.
    pub fn publish(&self) -> Result<LayoutPublication, Error> {
        self.recheck()?;
        if !self.can_write() {
            return Err(Error::root("source_update_conflicts"));
        }
        let status = self.check_output()?;
        if status == LayoutPublication::Unchanged {
            return Ok(status);
        }
        if status == LayoutPublication::Created {
            fs::create_dir(&self.destination).map_err(|error| files::io(&error))?;
            publish_new(
                &self.destination,
                WORKSPACE_BASE_UPDATE_RECEIPT,
                &serde_json::to_string(&self.receipt)
                    .map_err(|_| Error::root("source_schema_invalid"))?,
            )?;
        }
        for member in &self.members {
            if let Some(parent) = member.plan.destination().parent() {
                fs::create_dir_all(parent).map_err(|error| files::io(&error))?;
            }
        }
        self.check_output()?;
        for member in &self.members {
            member
                .plan
                .publish()
                .map_err(|error| error.in_field(member.name.as_str()))?;
        }
        self.recheck()?;
        for (path, text) in &self.ancillary {
            if path.as_str() != "eska.toml"
                && let Some(text) = text
            {
                self.publish_ancillary(path.as_str(), text)?;
            }
        }
        self.recheck()?;
        let manifest = self
            .ancillary
            .get(&super::SourcePath::parse("eska.toml")?)
            .and_then(Option::as_deref)
            .ok_or_else(|| Error::root("source_file_missing"))?;
        self.publish_ancillary("eska.toml", manifest)?;
        self.check_output()?;
        self.recheck()?;
        fs::remove_file(self.destination.join(WORKSPACE_BASE_UPDATE_RECEIPT))
            .map_err(|error| files::io(&error))?;
        if self.check_output()? != LayoutPublication::Unchanged {
            return Err(Error::root("source_update_output_mismatch"));
        }
        Ok(status)
    }

    /// A matching root receipt never permits replacing a foreign member, setting or directory.
    pub(super) fn check_output(&self) -> Result<LayoutPublication, Error> {
        match fs::symlink_metadata(&self.destination) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(LayoutPublication::Created);
            }
            Err(error) => return Err(files::io(&error)),
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => return Err(Error::root("source_destination_exists")),
        }
        let pending = read_optional(&self.destination.join(WORKSPACE_BASE_UPDATE_RECEIPT))?;
        let status = if let Some(text) = pending {
            let stored: WorkspaceReceipt = serde_json::from_str(&text)
                .map_err(|_| Error::root("source_update_receipt_invalid"))?;
            if stored != self.receipt {
                return Err(Error::root("source_update_receipt_invalid"));
            }
            LayoutPublication::Resumed
        } else {
            LayoutPublication::Unchanged
        };
        self.check_entries(Path::new(""))?;
        for member in &self.members {
            let nested = member
                .plan
                .check_output()
                .map_err(|error| error.in_field(member.name.as_str()))?;
            if status == LayoutPublication::Unchanged && nested != LayoutPublication::Unchanged {
                return Err(Error::root("source_destination_exists"));
            }
        }
        if status == LayoutPublication::Unchanged && ancillary(&self.destination)? != self.ancillary
        {
            return Err(Error::root("source_destination_exists"));
        }
        Ok(status)
    }

    /// Walk only declared ancestors; each member's own publisher validates its complete subtree.
    fn check_entries(&self, relative: &Path) -> Result<(), Error> {
        for entry in
            fs::read_dir(self.destination.join(relative)).map_err(|error| files::io(&error))?
        {
            let entry = entry.map_err(|error| files::io(&error))?;
            let path = relative.join(entry.file_name());
            let kind = entry.file_type().map_err(|error| files::io(&error))?;
            if kind.is_dir()
                && self
                    .members
                    .iter()
                    .any(|member| member.path.as_path() == path)
            {
                continue;
            }
            if kind.is_dir()
                && self
                    .members
                    .iter()
                    .any(|member| member.path.as_path().starts_with(&path))
            {
                self.check_entries(&path)?;
            } else if kind.is_file() && path == Path::new(WORKSPACE_BASE_UPDATE_RECEIPT) {
                // The outer receipt has already been parsed and matched.
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

    /// Root settings are exclusively installed after all members, with the root manifest last.
    fn publish_ancillary(&self, path: &str, text: &str) -> Result<(), Error> {
        match read_optional(&self.destination.join(path))? {
            None => publish_new(&self.destination, path, text)?,
            Some(current) if current == text => {}
            Some(_) => return Err(Error::root("source_update_output_mismatch")),
        }
        Ok(())
    }
}
