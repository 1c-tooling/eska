//! Acceptance-only native export; protected project builds are introduced after baseline policy.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use crate::project_core::MetadataKind;

use super::{
    Error, FileEntry, FileRole, NativeDocument, ObjectSource, PropertyName, PropertyValue,
    SourceManifest, SourcePath,
    composition::{identities, reference_path, references},
    files::{self, Inventory},
};

pub(super) mod queries;

/// Materialize a complete native snapshot for format acceptance, without platform subprocesses.
///
/// This experimental writer has no supplier-policy authority. A public build must first select
/// a protected snapshot. Existing output is accepted only when every path and byte already agree;
/// changed output is never replaced by this operation.
///
/// # Errors
/// Returns source, composition, identity, changed-input or conflicting-output failures.
pub fn export_designer(source: &Path, destination: &Path) -> Result<(), Error> {
    let package = Package::read(source)?;
    let source = fs::canonicalize(source).map_err(|error| files::io(&error))?;
    let destination = destination_path(&source, destination)?;
    let xml = package.native_xml()?;
    let expected = package.output_inventory(&xml)?;
    match fs::symlink_metadata(&destination) {
        Ok(metadata) if metadata.file_type().is_dir() => {
            if Inventory::read(&destination)? != expected {
                return Err(Error::root("source_export_destination_mismatch"));
            }
            package.recheck(&source)?;
            return Ok(());
        }
        Ok(_) => return Err(Error::root("source_destination_exists")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(files::io(&error)),
    }
    fs::create_dir(&destination).map_err(|error| files::io(&error))?;
    if let Err(error) = package.materialize(&source, &destination, &xml, &expected) {
        fs::remove_dir_all(&destination).map_err(|_| {
            Error::root("source_cleanup_failed")
                .in_field(destination.to_string_lossy().into_owned())
                .in_field(error.code)
        })?;
        return Err(error);
    }
    Ok(())
}

pub(super) struct Package {
    pub manifest: SourceManifest,
    pub snapshot: Inventory,
    pub documents: BTreeMap<SourcePath, NativeDocument>,
    pub forms: BTreeMap<SourcePath, super::ManagedFormSource>,
    pub owners: BTreeMap<SourcePath, Vec<SourcePath>>,
    /// Physical payload paths resolve to native identity for policy and publication.
    pub payloads: BTreeMap<SourcePath, SourcePath>,
    /// Native output paths resolve to the selected physical payload without repeated full scans.
    pub payload_sources: BTreeMap<SourcePath, SourcePath>,
    pub query_inputs: queries::QueryInputs,
}

impl Package {
    /// Read only declared owner files; extra filesystem files cannot disappear during export.
    pub(super) fn read(root: &Path) -> Result<Self, Error> {
        let snapshot = Inventory::read(root)?;
        Self::from_snapshot(snapshot, |path| {
            files::read_text(&root.join(path.as_path()))
        })
    }

    /// Share composition validation with pinned bytes stored outside a working source tree.
    pub(super) fn from_snapshot(
        snapshot: Inventory,
        mut read: impl FnMut(&SourcePath) -> Result<String, Error>,
    ) -> Result<Self, Error> {
        let manifest: SourceManifest =
            serde_json::from_str(&read(&SourcePath::parse(files::MANIFEST)?)?)
                .map_err(|_| Error::root("source_schema_invalid"))?;
        manifest.validate()?;
        let owners: BTreeSet<_> = manifest
            .files
            .iter()
            .filter_map(|file| {
                if let FileRole::Descriptor { source, .. } = &file.role {
                    Some(source)
                } else {
                    None
                }
            })
            .collect();
        let mut documents = BTreeMap::new();
        let mut forms = BTreeMap::new();
        let form_paths = manifest
            .files
            .iter()
            .filter_map(|file| {
                if let FileRole::ManagedForm { source, .. } = &file.role {
                    Some((source, &file.path))
                } else {
                    None
                }
            })
            .collect::<BTreeMap<_, _>>();
        let mut owner_documents = BTreeMap::new();
        let entries: BTreeMap<_, _> = manifest
            .files
            .iter()
            .map(|file| (file.path.as_str(), file))
            .collect();
        for owner in &owners {
            let object = ObjectSource::from_yaml(&read(owner)?)
                .map_err(|error| error.in_field(owner.as_str()))?;
            super::layout::forms::validate_source(&object, form_paths.contains_key(owner))
                .map_err(|error| error.in_field(owner.as_str()))?;
            if let Some(form) = object.form {
                let native = form_paths
                    .get(owner)
                    .ok_or_else(|| Error::root("source_form_schema_invalid"))?;
                forms.insert((*native).clone(), form);
            }
            let mut paths = Vec::with_capacity(1 + object.nested.len());
            for part in std::iter::once(object.document).chain(object.nested) {
                paths.push(part.path.clone());
                let entry = entries
                    .get(part.path.as_str())
                    .ok_or_else(|| Error::root("source_descriptor_undeclared"))?;
                if !matches!(&entry.role, FileRole::Descriptor { source, uuid } if source == *owner && uuid.key() == part.descriptor.object.uuid.key())
                {
                    return Err(
                        Error::root("source_identity_mismatch").in_field(part.path.as_str())
                    );
                }
                if documents.insert(part.path.clone(), part).is_some() {
                    return Err(Error::root("source_descriptor_duplicate"));
                }
            }
            owner_documents.insert((*owner).clone(), paths);
        }
        validate_sources(&manifest, &snapshot, &owners, &documents)?;
        validate_composition(&documents)?;
        super::layout::validate_documents(&manifest, &documents)?;
        let query_inputs = queries::load(&manifest, &forms, &documents, &mut read)?;
        let payloads = manifest
            .files
            .iter()
            .filter(|file| file.is_payload())
            .map(|file| (file.source_path().clone(), file.path.clone()))
            .collect();
        let payload_sources = manifest
            .files
            .iter()
            .filter(|file| file.is_payload())
            .map(|file| (file.path.clone(), file.source_path().clone()))
            .collect();
        Ok(Self {
            manifest,
            snapshot,
            documents,
            forms,
            owners: owner_documents,
            payloads,
            payload_sources,
            query_inputs,
        })
    }

    /// Generate all native descriptors before claiming an export directory.
    pub(super) fn native_xml(&self) -> Result<BTreeMap<SourcePath, String>, Error> {
        let mut output = self
            .documents
            .iter()
            .map(|(path, document)| {
                Ok((
                    path.clone(),
                    document
                        .descriptor
                        .to_xml()
                        .map_err(|error| error.in_field(path.as_str()))?,
                ))
            })
            .chain(self.forms.iter().map(|(path, form)| {
                Ok((
                    path.clone(),
                    form.to_xml()
                        .map_err(|error| error.in_field(path.as_str()))?,
                ))
            }))
            .collect::<Result<BTreeMap<_, _>, Error>>()?;
        self.query_inputs.materialize(&self.manifest, &mut output)?;
        Ok(output)
    }

    /// Predict exact native bytes and directory composition, including preserved empty directories.
    pub(super) fn output_inventory(
        &self,
        xml: &BTreeMap<SourcePath, String>,
    ) -> Result<Inventory, Error> {
        let mut files = BTreeMap::new();
        let mut directories: BTreeSet<_> = self.manifest.directories.iter().cloned().collect();
        for file in &self.manifest.files {
            let entry = if let Some(text) = xml.get(&file.path) {
                FileEntry {
                    path: file.path.clone(),
                    bytes: text.len() as u64,
                    sha256: files::hex(&Sha256::digest(text.as_bytes())),
                    role: FileRole::Preserved,
                }
            } else {
                self.snapshot
                    .files
                    .get(file.source_path())
                    .ok_or_else(|| Error::root("source_file_missing"))?
                    .clone()
            };
            let mut entry = entry;
            entry.path.clone_from(&file.path);
            for (end, _) in entry.path.as_str().match_indices('/') {
                directories.insert(SourcePath::parse(&entry.path.as_str()[..end])?);
            }
            files.insert(entry.path.clone(), entry);
        }
        let directories: Vec<_> = directories.into_iter().collect();
        let revision = files::revision(&files, &directories);
        Ok(Inventory {
            files,
            directories,
            revision,
        })
    }

    /// Complete a new output and recheck both snapshots before reporting successful publication.
    fn materialize(
        &self,
        source: &Path,
        destination: &Path,
        xml: &BTreeMap<SourcePath, String>,
        expected: &Inventory,
    ) -> Result<(), Error> {
        for directory in &expected.directories {
            fs::create_dir_all(destination.join(directory.as_path()))
                .map_err(|error| files::io(&error))?;
        }
        for entry in expected.files.values() {
            if let Some(text) = xml.get(&entry.path) {
                files::write(&destination.join(entry.path.as_path()), text.as_bytes())?;
            } else {
                let path = self
                    .payload_sources
                    .get(&entry.path)
                    .ok_or_else(|| Error::root("source_file_missing"))?;
                let input = self
                    .snapshot
                    .files
                    .get(path)
                    .ok_or_else(|| Error::root("source_file_missing"))?;
                files::copy_to(source, &destination.join(entry.path.as_path()), input)?;
            }
        }
        self.recheck(source)?;
        if Inventory::read(destination)? != *expected {
            return Err(Error::root("source_export_changed"));
        }
        Ok(())
    }

    /// A changed YAML, payload, manifest or file composition cannot bless a stale export.
    pub(super) fn recheck(&self, root: &Path) -> Result<(), Error> {
        if Inventory::read(root)? != self.snapshot {
            return Err(Error::root("source_input_changed"));
        }
        Ok(())
    }
}

/// Required physical files and descriptors must correspond exactly to the complete native map.
fn validate_sources(
    manifest: &SourceManifest,
    snapshot: &Inventory,
    owners: &BTreeSet<&SourcePath>,
    documents: &BTreeMap<SourcePath, NativeDocument>,
) -> Result<(), Error> {
    let mut paths = BTreeSet::from([SourcePath::parse(files::MANIFEST)?]);
    paths.extend(owners.iter().map(|path| (*path).clone()));
    paths.extend(
        manifest
            .query_resources
            .iter()
            .flat_map(|resource| &resource.sources)
            .cloned(),
    );
    for file in &manifest.files {
        match &file.role {
            FileRole::Preserved | FileRole::Relocated { .. } => {
                paths.insert(file.source_path().clone());
            }
            FileRole::Descriptor { .. } if !documents.contains_key(&file.path) => {
                return Err(Error::root("source_descriptor_missing").in_field(file.path.as_str()));
            }
            FileRole::Descriptor { .. } | FileRole::ManagedForm { .. } => {}
        }
    }
    if snapshot.files.keys().collect::<BTreeSet<_>>() != paths.iter().collect() {
        return Err(Error::root("source_file_map_mismatch"));
    }
    let mut directories: BTreeSet<_> = super::layout::resources::source_directories(manifest)?
        .into_iter()
        .collect();
    for path in &paths {
        for (end, _) in path.as_str().match_indices('/') {
            directories.insert(SourcePath::parse(&path.as_str()[..end])?);
        }
    }
    if snapshot
        .directories
        .iter()
        .any(|directory| !directories.contains(directory))
    {
        return Err(Error::root("source_file_map_mismatch"));
    }
    Ok(())
}

/// Walk current declared composition, refusing missing, extra or ambiguously identified objects.
pub(super) fn validate_composition(
    documents: &BTreeMap<SourcePath, NativeDocument>,
) -> Result<(), Error> {
    let root = super::root::document_root(documents)?;
    let mut pending = vec![(root.path.clone(), root.descriptor.object.kind, None)];
    let mut visited = BTreeSet::new();
    let mut uuids = BTreeSet::new();
    while let Some((path, kind, name)) = pending.pop() {
        if !visited.insert(path.clone()) {
            return Err(Error::root("source_descriptor_duplicate"));
        }
        let document = documents
            .get(&path)
            .ok_or_else(|| Error::root("source_descriptor_missing").in_field(path.as_str()))?;
        let node = &document.descriptor.object;
        if node.kind != kind
            || name.is_some_and(|name| {
                node.property(PropertyName::Name) != Some(&PropertyValue::Text(name))
            })
        {
            return Err(Error::root("source_declaration_mismatch").in_field(path.as_str()));
        }
        identities(node, &mut uuids)?;
        let base = if kind == MetadataKind::Configuration {
            ""
        } else {
            path.as_str()
                .strip_suffix(".xml")
                .ok_or_else(|| Error::root("source_path_invalid"))?
        };
        for (kind, name) in references(node)? {
            pending.push((reference_path(base, kind, &name)?, kind, Some(name)));
        }
    }
    if visited.len() != documents.len() {
        return Err(Error::root("source_descriptor_undeclared"));
    }
    super::targets::validate_documents(documents)
}

/// Resolve an output next to an existing parent, independently of process-relative input spelling.
fn destination_path(source: &Path, destination: &Path) -> Result<PathBuf, Error> {
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::root("source_path_invalid"))?;
    SourcePath::parse(name)?;
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let target = fs::canonicalize(parent)
        .map_err(|error| files::io(&error))?
        .join(name);
    if target.starts_with(source) || source.starts_with(&target) {
        return Err(Error::root("source_tree_overlap"));
    }
    Ok(target)
}
