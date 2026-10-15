//! Current source bindings retain the original policy and fence editor writes against composition changes.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use super::{AcceptedBase, Error, FileEntry, SourcePath, files};
use crate::project_core::{
    Project,
    metadata_model::ProjectScope,
    source_format::{
        FileRole, ProjectSettingsGuard, SourceManifest, SourceReader, export::Package,
    },
};

/// Only captured authoring descriptors and the reverse map fence a bound editor's structure.
#[derive(Clone, Debug)]
pub(in crate::project_core::source_format) struct SourceBinding {
    root: PathBuf,
    project: Option<ProjectSettingsGuard>,
    files: BTreeMap<SourcePath, FileEntry>,
}

impl AcceptedBase {
    /// Bind verified current composition to the independently pinned policy without accepting source bytes.
    ///
    /// Names, paths, removals and new owners use the same validation as protected preparation.
    /// The returned handle retains the original approved pin and immutable blob authority.
    ///
    /// # Errors
    /// Returns invalid root/map/identity, denied structural changes, changed input or I/O failures.
    pub fn bind_source(self, source: &Path) -> Result<Self, Error> {
        self.bind_source_cancellable(source, &|| false)
    }

    /// Verify current bindings with cooperative cancellation, retaining no partial permission view.
    ///
    /// Descriptor parsing is bounded but an individual parse is not interruptible.
    ///
    /// # Errors
    /// Returns the same binding failures as [`Self::bind_source`] or `source_read_cancelled`.
    pub fn bind_source_cancellable(
        mut self,
        source: &Path,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Self, Error> {
        files::check_cancelled(cancelled)?;
        if self.composition.is_some() || self.source_binding.is_some() {
            self = Self::open_cancellable(&self.root, &self.reference, cancelled)?;
        }
        let source = checked_root(source)?;
        let marker = SourcePath::parse(files::MANIFEST)?;
        let text = read(&source, &marker, cancelled)?;
        let manifest: SourceManifest =
            serde_json::from_str(&text).map_err(|_| Error::root("source_schema_invalid"))?;
        manifest.validate()?;
        let expected = if manifest == self.package.manifest {
            None
        } else {
            let snapshot = files::Inventory::read_cancellable(&source, cancelled)?;
            let candidate =
                Package::from_snapshot(snapshot.clone(), |path| read(&source, path, cancelled))?;
            self = self.with_composition(&candidate, |path| read(&source, path, cancelled))?;
            Some(snapshot)
        };
        let mut entries = BTreeMap::new();
        for path in std::iter::once(&marker).chain(self.package.owners.keys()) {
            let text = read(&source, path, cancelled)?;
            let entry = FileEntry {
                path: path.clone(),
                bytes: text.len() as u64,
                sha256: files::hex(&Sha256::digest(text.as_bytes())),
                role: FileRole::Preserved,
            };
            if expected
                .as_ref()
                .is_some_and(|snapshot| snapshot.files.get(path) != Some(&entry))
            {
                return Err(Error::root("source_input_changed").in_field(path.as_str()));
            }
            entries.insert(path.clone(), entry);
        }
        let binding = SourceBinding {
            root: source,
            project: None,
            files: entries,
        };
        let reader = SourceReader::open(&binding.root)?;
        self.validate_source_root(&reader)?;
        binding.verify(&binding.root, cancelled)?;
        self.source_binding = Some(binding);
        files::check_cancelled(cancelled)?;
        Ok(self)
    }

    /// Saved plans retain their own fence; replacing a caller's handle cannot authorize stale structure.
    pub(in crate::project_core::source_format) fn source_binding(&self) -> Option<SourceBinding> {
        self.source_binding.clone()
    }

    /// A project consumer carries its publication guard into every subsequently prepared saved edit.
    pub(in crate::project_core) fn with_project_guard(
        mut self,
        project: &Project,
        scope: &ProjectScope,
        expected_manifest_sha256: &str,
    ) -> Result<Self, Error> {
        let binding = self
            .source_binding
            .as_mut()
            .ok_or_else(|| Error::root("source_edit_structure_changed"))?;
        let manifest = SourcePath::parse(files::MANIFEST)?;
        if binding.root != project.source()
            || binding
                .files
                .get(&manifest)
                .is_none_or(|entry| entry.sha256 != expected_manifest_sha256)
        {
            return Err(Error::root("source_edit_structure_changed"));
        }
        binding.project = Some(ProjectSettingsGuard::capture(project, scope)?);
        binding.verify(&binding.root, &|| false)?;
        Ok(self)
    }
}

impl SourceBinding {
    /// Only a selected project coordinates publication with its import and recovery writers.
    pub(in crate::project_core::source_format) fn acquire_write_lock(
        &self,
    ) -> Result<Option<super::super::source_write::Guard>, Error> {
        self.project
            .as_ref()
            .map(|guard| super::super::source_write::acquire(guard.root()))
            .transpose()
    }

    /// Descriptor/map changes or a different root invalidate saved editor publication before any write.
    pub(in crate::project_core::source_format) fn verify(
        &self,
        root: &Path,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), Error> {
        if let Some(project) = &self.project {
            project.verify(cancelled)?;
        }
        if checked_root(root)? != self.root {
            return Err(Error::root("source_edit_structure_changed"));
        }
        for (path, entry) in &self.files {
            let text = read(root, path, cancelled)?;
            if text.len() as u64 != entry.bytes
                || files::hex(&Sha256::digest(text.as_bytes())) != entry.sha256
            {
                return Err(Error::root("source_edit_structure_changed").in_field(path.as_str()));
            }
        }
        if let Some(project) = &self.project {
            project.verify(cancelled)?;
        }
        Ok(())
    }
}

/// A source alias cannot choose a different directory under a previously checked editor root.
fn checked_root(source: &Path) -> Result<PathBuf, Error> {
    if !fs::symlink_metadata(source)
        .map_err(|error| files::io(&error))?
        .file_type()
        .is_dir()
    {
        return Err(Error::root("source_edit_unsafe_path"));
    }
    source.canonicalize().map_err(|error| files::io(&error))
}

/// Every directory and final file remains non-aliased before a bounded, cancellable read.
fn read(root: &Path, path: &SourcePath, cancelled: &dyn Fn() -> bool) -> Result<String, Error> {
    let mut current = root.to_owned();
    for part in path.as_path().components() {
        files::check_cancelled(cancelled)?;
        current.push(part);
        let kind = fs::symlink_metadata(&current)
            .map_err(|error| files::io(&error))?
            .file_type();
        let valid = if current == root.join(path.as_path()) {
            kind.is_file()
        } else {
            kind.is_dir()
        };
        if !valid {
            return Err(Error::root("source_edit_unsafe_path").in_field(path.as_str()));
        }
    }
    files::read_text_cancellable(&current, files::MAX_TEXT_BYTES, cancelled)
}
