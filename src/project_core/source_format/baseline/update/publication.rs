//! Exclusive destination trees publish complete source and accepted markers only after rechecks.

use std::fs;

use super::{
    AcceptedBase, BTreeMap, BaseUpdatePlan, Error, Inventory, Path, PathBuf, SelectedFile,
    SourcePath, files,
};
use crate::project_core::source_format::{LayoutPublication, layout::migration::publication};

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
            .ok_or_else(|| Error::root("source_path_invalid"))?;
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
