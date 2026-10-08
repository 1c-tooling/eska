//! Acceptance-only native export; protected project builds are introduced after baseline policy.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use crate::project::metadata_model::MetadataKind;

use super::{
    Error, FileEntry, FileRole, NativeDocument, ObjectSource, PropertyName, PropertyValue,
    SourceManifest, SourcePath,
    composition::{identities, reference_path, references},
    files::{self, Inventory},
};

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

struct Package {
    manifest: SourceManifest,
    snapshot: Inventory,
    documents: BTreeMap<SourcePath, NativeDocument>,
}

impl Package {
    /// Read only declared owner files; extra filesystem files cannot disappear during export.
    fn read(root: &Path) -> Result<Self, Error> {
        let snapshot = Inventory::read(root)?;
        let manifest: SourceManifest =
            serde_json::from_str(&files::read_text(&root.join(files::MANIFEST))?)
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
        let entries: BTreeMap<_, _> = manifest
            .files
            .iter()
            .map(|file| (file.path.as_str(), file))
            .collect();
        for owner in &owners {
            let object = ObjectSource::from_yaml(&files::read_text(&root.join(owner.as_path()))?)
                .map_err(|error| error.in_field(owner.as_str()))?;
            for part in std::iter::once(object.document).chain(object.nested) {
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
        }
        validate_sources(&manifest, &snapshot, &owners, &documents)?;
        validate_composition(&documents)?;
        Ok(Self {
            manifest,
            snapshot,
            documents,
        })
    }

    /// Generate all native descriptors before claiming an export directory.
    fn native_xml(&self) -> Result<BTreeMap<SourcePath, String>, Error> {
        self.documents
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
            .collect()
    }

    /// Predict exact native bytes and directory composition, including preserved empty directories.
    fn output_inventory(&self, xml: &BTreeMap<SourcePath, String>) -> Result<Inventory, Error> {
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
                    .get(&file.path)
                    .ok_or_else(|| Error::root("source_file_missing"))?
                    .clone()
            };
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
                files::copy(source, destination, entry)?;
            }
        }
        self.recheck(source)?;
        if Inventory::read(destination)? != *expected {
            return Err(Error::root("source_export_changed"));
        }
        Ok(())
    }

    /// A changed YAML, payload, manifest or file composition cannot bless a stale export.
    fn recheck(&self, root: &Path) -> Result<(), Error> {
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
    for file in &manifest.files {
        match &file.role {
            FileRole::Preserved => {
                paths.insert(file.path.clone());
            }
            FileRole::Descriptor { .. } if !documents.contains_key(&file.path) => {
                return Err(Error::root("source_descriptor_missing").in_field(file.path.as_str()));
            }
            FileRole::Descriptor { .. } => {}
        }
    }
    if snapshot.files.keys().collect::<BTreeSet<_>>() != paths.iter().collect() {
        return Err(Error::root("source_file_map_mismatch"));
    }
    let mut directories: BTreeSet<_> = manifest.directories.iter().cloned().collect();
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
fn validate_composition(documents: &BTreeMap<SourcePath, NativeDocument>) -> Result<(), Error> {
    let root = SourcePath::parse("Configuration.xml")?;
    let mut pending = vec![(root, MetadataKind::Configuration, None)];
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
    Ok(())
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
