//! Exclusive destination trees publish complete source and accepted markers only after rechecks.

use std::fs;

use super::{
    AcceptedBase, BTreeMap, BaseUpdatePlan, Error, Inventory, Path, PathBuf, SelectedFile,
    SourcePath, files,
};
use crate::project_core::source_format::{LayoutPublication, layout::migration::publication};

/// Only a matching outer project receipt permits this private staging filename.
const TEMPORARY: &str = ".eska-base-update-write";

impl BaseUpdatePlan {
    /// Publish a validated merged source tree without rewriting the selected working project.
    ///
    /// # Errors
    /// Refuses conflicts, stale input, overlaps, unsafe paths and any changed/foreign destination.
    pub fn publish_source(&self, destination: &Path) -> Result<LayoutPublication, Error> {
        self.ready()?;
        let destination = self.destination(destination)?;
        match fs::symlink_metadata(&destination) {
            Ok(metadata)
                if metadata.file_type().is_dir()
                    && Inventory::read(&destination)? == self.output =>
            {
                self.recheck()?;
                return Ok(LayoutPublication::Unchanged);
            }
            Ok(_) => return Err(Error::root("source_destination_exists")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(files::io(&error)),
        }
        let incoming = AcceptedBase::open(&self.incoming_root, &self.incoming)?;
        fs::create_dir(&destination).map_err(|error| files::io(&error))?;
        let result = self.write_source(&incoming, &destination);
        if let Err(error) = result {
            fs::remove_dir_all(&destination).map_err(|_| Error::root("source_cleanup_failed"))?;
            return Err(error);
        }
        Ok(LayoutPublication::Created)
    }

    /// Copy only the independently approved incoming store; local source never enters its pin.
    ///
    /// The old store remains intact. Project orchestration must select the new reference explicitly.
    ///
    /// # Errors
    /// Refuses conflicts, stale inputs, overlaps, incomplete foreign output and changed accepted bytes.
    pub fn publish_accepted(&self, destination: &Path) -> Result<LayoutPublication, Error> {
        self.ready()?;
        let destination = self.destination(destination)?;
        let status = publication::publish(
            publication::View {
                root: &self.incoming_root,
                input: &self.incoming_storage,
                output: &self.incoming_storage,
                text: &BTreeMap::new(),
                ready: super::super::BASE_MANIFEST,
                renamed: None,
            },
            &destination,
        )?;
        self.recheck()?;
        Ok(status)
    }

    /// Project orchestration checks both trees before writing either, without approving conflicts.
    pub(in crate::project_core::source_format) fn check_source(
        &self,
        destination: &Path,
        pending: bool,
    ) -> Result<LayoutPublication, Error> {
        self.recheck()?;
        if pending
            && self
                .output
                .files
                .contains_key(&SourcePath::parse(TEMPORARY)?)
        {
            return Err(Error::root("source_path_collision"));
        }
        let destination = self.destination(destination)?;
        match fs::symlink_metadata(&destination) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(LayoutPublication::Created)
            }
            Err(error) => Err(files::io(&error)),
            Ok(metadata) if metadata.file_type().is_dir() => {
                let mut current = Inventory::read(&destination)?;
                if current == self.output {
                    return Ok(LayoutPublication::Unchanged);
                }
                if !pending {
                    return Err(Error::root("source_destination_exists"));
                }
                let temporary = SourcePath::parse(TEMPORARY)?;
                current.files.remove(&temporary);
                if current
                    .files
                    .iter()
                    .any(|(path, entry)| self.output.files.get(path) != Some(entry))
                    || current
                        .directories
                        .iter()
                        .any(|path| !self.output.directories.contains(path))
                {
                    return Err(Error::root("source_update_output_mismatch"));
                }
                Ok(LayoutPublication::Resumed)
            }
            Ok(_) => Err(Error::root("source_destination_exists")),
        }
    }

    /// Accepted storage uses its existing content-addressed recovery protocol and never working text.
    pub(in crate::project_core::source_format) fn check_accepted(
        &self,
        destination: &Path,
    ) -> Result<LayoutPublication, Error> {
        self.recheck()?;
        publication::preflight(
            publication::View {
                root: &self.incoming_root,
                input: &self.incoming_storage,
                output: &self.incoming_storage,
                text: &BTreeMap::new(),
                ready: super::super::BASE_MANIFEST,
                renamed: None,
            },
            destination,
        )
        .map(|(_, status)| status)
    }

    /// Resume exact source files only while the enclosing project is protected by its receipt.
    pub(in crate::project_core::source_format) fn publish_pending_source(
        &self,
        destination: &Path,
    ) -> Result<LayoutPublication, Error> {
        self.ready()?;
        let destination = self.destination(destination)?;
        let status = self.check_source(&destination, true)?;
        if status == LayoutPublication::Unchanged {
            return Ok(status);
        }
        if status == LayoutPublication::Created {
            fs::create_dir(&destination).map_err(|error| files::io(&error))?;
        }
        let incoming = AcceptedBase::open(&self.incoming_root, &self.incoming)?;
        let temporary = destination.join(TEMPORARY);
        if fs::symlink_metadata(&temporary).is_ok() {
            fs::remove_file(&temporary).map_err(|error| files::io(&error))?;
        }
        for path in &self.output.directories {
            fs::create_dir_all(destination.join(path.as_path()))
                .map_err(|error| files::io(&error))?;
        }
        let manifest = SourcePath::parse(files::MANIFEST)?;
        for (path, selected) in self.selected.iter().filter(|(path, _)| **path != manifest) {
            self.stage_source(selected, &incoming, &destination, path)?;
        }
        self.recheck()?;
        let selected = self
            .selected
            .get(&manifest)
            .ok_or_else(|| Error::root("source_descriptor_missing"))?;
        self.stage_source(selected, &incoming, &destination, &manifest)?;
        if Inventory::read(&destination)? != self.output {
            return Err(Error::root("source_update_output_mismatch"));
        }
        self.recheck()?;
        Ok(status)
    }

    /// An interrupted copy can leave only private staging bytes, never a partly installed source.
    fn stage_source(
        &self,
        selected: &SelectedFile,
        incoming: &AcceptedBase,
        destination: &Path,
        path: &SourcePath,
    ) -> Result<(), Error> {
        let target = destination.join(path.as_path());
        if fs::symlink_metadata(&target).is_ok() {
            return Ok(());
        }
        let temporary = destination.join(TEMPORARY);
        self.copy(selected, incoming, &temporary)?;
        fs::hard_link(&temporary, &target).map_err(|error| files::io(&error))?;
        fs::remove_file(temporary).map_err(|error| files::io(&error))
    }

    /// A conflicting preview cannot publish either side of an update.
    fn ready(&self) -> Result<(), Error> {
        if !self.conflicts.is_empty() {
            return Err(Error::root("source_update_conflicts"));
        }
        self.recheck()
    }

    /// Resolve only a real parent directory and reject overlap with every captured input tree.
    fn destination(&self, destination: &Path) -> Result<PathBuf, Error> {
        let name = destination
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| Error::root("source_path_invalid"))?;
        SourcePath::parse(name)?;
        let parent = destination
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let destination = parent
            .canonicalize()
            .map_err(|error| files::io(&error))?
            .join(name);
        for root in [&self.source, &self.previous_root, &self.incoming_root] {
            if destination.starts_with(root) || root.starts_with(&destination) {
                return Err(Error::root("source_baseline_overlap"));
            }
        }
        Ok(destination)
    }

    /// The source manifest is a ready marker and is written after every selected body.
    fn write_source(&self, incoming: &AcceptedBase, destination: &Path) -> Result<(), Error> {
        for path in &self.output.directories {
            fs::create_dir_all(destination.join(path.as_path()))
                .map_err(|error| files::io(&error))?;
        }
        let manifest = SourcePath::parse(files::MANIFEST)?;
        for (path, selected) in self.selected.iter().filter(|(path, _)| **path != manifest) {
            self.copy(selected, incoming, &destination.join(path.as_path()))?;
        }
        self.recheck()?;
        let selected = self
            .selected
            .get(&manifest)
            .ok_or_else(|| Error::root("source_descriptor_missing"))?;
        self.copy(selected, incoming, &destination.join(manifest.as_path()))?;
        if Inventory::read(destination)? != self.output {
            return Err(Error::root("source_update_output_mismatch"));
        }
        self.recheck()
    }

    /// Fingerprinted streaming copies preserve binary resources, encodings and original trivia.
    fn copy(
        &self,
        selected: &SelectedFile,
        incoming: &AcceptedBase,
        destination: &Path,
    ) -> Result<(), Error> {
        match selected {
            SelectedFile::Working(entry) => files::copy_to(&self.source, destination, entry),
            SelectedFile::Incoming(entry) => incoming.copy_file(&entry.path, destination),
        }
    }
}
