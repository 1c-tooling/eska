//! Owner-local metadata snapshots; opening a tree never reads modules, resources or a baseline.

mod projection;
mod semantic;

#[cfg(test)]
mod measurement;
#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    ops::Range,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use crate::project_core::MetadataKind;

use super::{
    Error, FileRole, MetadataNode, NativeDocument, NativeUuid, ObjectSource, PropertyName,
    PropertyValue, SourceManifest, SourcePath,
    composition::{identities, reference_path, references, source_path},
    files, yaml,
};

pub use semantic::{MetadataSnapshot, NativeMetadata};

/// One declaration projected directly from typed YAML, with coordinates in its owner snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataDeclaration {
    pub kind: MetadataKind,
    pub uuid: NativeUuid,
    pub name: String,
    pub parent: Option<NativeUuid>,
    pub native_path: SourcePath,
    /// UTF-8 byte range of the metadata object mapping in the exact owner text.
    pub range: Range<usize>,
    /// UTF-8 byte range of the Name scalar, including its YAML quoting or block syntax.
    pub name_range: Range<usize>,
}

/// Coordinates and facts belong to these exact bytes, not to a generated XML or a path timestamp.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnerSnapshot {
    pub path: SourcePath,
    pub source_sha256: String,
    pub manifest_sha256: String,
    pub source: ObjectSource,
    pub declarations: Vec<MetadataDeclaration>,
}

/// A declared owner has one main descriptor and the complete native identities of its family.
struct OwnerSpec {
    main: SourcePath,
    kind: MetadataKind,
    name: Option<String>,
    parent: Option<NativeUuid>,
    descriptors: BTreeMap<SourcePath, NativeUuid>,
}

/// An explicit manifest/configuration snapshot, with other owners read only when requested.
///
/// Callers reopen after a manifest or configuration change. Individual owner reads always use
/// current bytes, and in-memory overlays share exactly the same parser. This view neither grants
/// edit permissions nor replaces the complete composition and payload checks at publication.
pub struct SourceReader {
    root: PathBuf,
    manifest: SourceManifest,
    manifest_sha256: String,
    configuration: OwnerSnapshot,
    owners: BTreeMap<SourcePath, OwnerSpec>,
}

impl SourceReader {
    /// Read only the file map and configuration; missing unrelated payloads do not block a view.
    ///
    /// # Errors
    /// Returns unsupported schema, unsafe path, root composition or native identity failures.
    pub fn open(root: &Path) -> Result<Self, Error> {
        require_directory(root)?;
        let root = fs::canonicalize(root).map_err(|error| files::io(&error))?;
        let text = read_text(&root, &SourcePath::parse(files::MANIFEST)?)?;
        let manifest: SourceManifest =
            serde_json::from_str(&text).map_err(|_| Error::root("source_schema_invalid"))?;
        manifest.validate()?;
        let manifest_sha256 = files::hex(&Sha256::digest(text.as_bytes()));
        let mut groups = descriptor_groups(&manifest)?;
        let root_path = SourcePath::parse("configuration.yaml")?;
        let root_spec = OwnerSpec {
            main: SourcePath::parse("Configuration.xml")?,
            kind: MetadataKind::Configuration,
            name: None,
            parent: None,
            descriptors: groups
                .remove(&root_path)
                .ok_or_else(|| Error::root("source_descriptor_missing"))?,
        };
        let configuration = snapshot(
            &root_path,
            &read_text(&root, &root_path)?,
            &manifest_sha256,
            &root_spec,
        )?;
        let parent = &configuration.source.document.descriptor.object.uuid;
        let mut owners = BTreeMap::new();
        for (kind, name) in references(&configuration.source.document.descriptor.object)? {
            let main = reference_path("", kind, &name)?;
            let path = source_path(&main)?;
            let descriptors = groups
                .remove(&path)
                .ok_or_else(|| Error::root("source_descriptor_missing").in_field(main.as_str()))?;
            if !descriptors.contains_key(&main) {
                return Err(Error::root("source_identity_mismatch").in_field(path.as_str()));
            }
            owners.insert(
                path,
                OwnerSpec {
                    main,
                    kind,
                    name: Some(name),
                    parent: Some(parent.clone()),
                    descriptors,
                },
            );
        }
        if !groups.is_empty() {
            return Err(Error::root("source_descriptor_undeclared"));
        }
        owners.insert(root_path, root_spec);
        Ok(Self {
            root,
            manifest,
            manifest_sha256,
            configuration,
            owners,
        })
    }

    /// Return the root facts read at opening; reopen when this file or the manifest changes.
    #[must_use]
    pub const fn configuration(&self) -> &OwnerSnapshot {
        &self.configuration
    }

    /// Return the original file map without touching the mapped modules or resources.
    #[must_use]
    pub const fn manifest(&self) -> &SourceManifest {
        &self.manifest
    }

    /// Enumerate declared owners, including the configuration, without opening their files.
    pub fn owner_paths(&self) -> impl Iterator<Item = &SourcePath> {
        self.owners.keys()
    }

    /// Read current bytes of exactly one declared owner, rejecting filesystem aliases.
    ///
    /// # Errors
    /// Returns missing/unsafe file, strict YAML, identity or owner-local composition failures.
    pub fn read_owner(&self, path: &SourcePath) -> Result<OwnerSnapshot, Error> {
        self.spec(path)?;
        self.parse_owner(path, &read_text(&self.root, path)?)
    }

    /// Project unsaved editor text with the same schema and coordinates as an on-disk read.
    ///
    /// # Errors
    /// Returns strict YAML or identity/composition failures; never falls back to earlier bytes.
    pub fn parse_owner(&self, path: &SourcePath, text: &str) -> Result<OwnerSnapshot, Error> {
        let result = snapshot(path, text, &self.manifest_sha256, self.spec(path)?)?;
        if result.source.document.descriptor.object.kind == MetadataKind::Configuration
            && references(&result.source.document.descriptor.object)?
                != references(&self.configuration.source.document.descriptor.object)?
        {
            return Err(Error::root("source_declaration_mismatch").in_field(path.as_str()));
        }
        Ok(result)
    }

    /// Read one YAML family as the same logical descriptors used by the XML consumer.
    ///
    /// # Errors
    /// Returns source, schema, identity or semantic-projection failures without generating XML.
    pub fn read_metadata(
        &self,
        path: &SourcePath,
        mode: crate::project_core::metadata_parser::PropertiesMode,
    ) -> Result<MetadataSnapshot, Error> {
        self.spec(path)?;
        self.parse_metadata(path, &read_text(&self.root, path)?, mode)
    }

    /// Project unsaved YAML into the shared model with exact coordinates in this text.
    ///
    /// # Errors
    /// Returns the same failures as an on-disk metadata read, without fallback or writes.
    pub fn parse_metadata(
        &self,
        path: &SourcePath,
        text: &str,
        mode: crate::project_core::metadata_parser::PropertiesMode,
    ) -> Result<MetadataSnapshot, Error> {
        let result = (|| {
            let (value, tree) = yaml::decode_with_tree(text)?;
            let source = ObjectSource::from_value(value)?;
            let parents = validate_owner(&source, self.spec(path)?)?;
            if source.document.descriptor.object.kind == MetadataKind::Configuration
                && references(&source.document.descriptor.object)?
                    != references(&self.configuration.source.document.descriptor.object)?
            {
                return Err(Error::root("source_declaration_mismatch"));
            }
            let documents = semantic::documents(&source, &tree, &parents, mode)?;
            Ok(MetadataSnapshot {
                path: path.clone(),
                source_sha256: files::hex(&Sha256::digest(text.as_bytes())),
                manifest_sha256: self.manifest_sha256.clone(),
                documents,
            })
        })();
        result.map_err(|error: Error| error.in_field(path.as_str()))
    }

    /// Unknown files cannot become metadata merely by matching a directory naming convention.
    fn spec(&self, path: &SourcePath) -> Result<&OwnerSpec, Error> {
        self.owners
            .get(path)
            .ok_or_else(|| Error::root("source_descriptor_undeclared").in_field(path.as_str()))
    }
}

/// Index only descriptor identities; the manifest does not activate undeclared owners.
fn descriptor_groups(
    manifest: &SourceManifest,
) -> Result<BTreeMap<SourcePath, BTreeMap<SourcePath, NativeUuid>>, Error> {
    let mut groups: BTreeMap<SourcePath, BTreeMap<SourcePath, NativeUuid>> = BTreeMap::new();
    let mut uuids = BTreeSet::new();
    for file in &manifest.files {
        if let FileRole::Descriptor { source, uuid } = &file.role {
            if !uuids.insert(uuid.key()) {
                return Err(Error::root("native_uuid_duplicate").in_field(file.path.as_str()));
            }
            groups
                .entry(source.clone())
                .or_default()
                .insert(file.path.clone(), uuid.clone());
        }
    }
    Ok(groups)
}

/// Parse once and tie semantic declarations to coordinates and an exact source fingerprint.
fn snapshot(
    path: &SourcePath,
    text: &str,
    manifest_sha256: &str,
    spec: &OwnerSpec,
) -> Result<OwnerSnapshot, Error> {
    let result = (|| {
        let (value, tree) = yaml::decode_with_tree(text)?;
        let source = ObjectSource::from_value(value)?;
        let parents = validate_owner(&source, spec)?;
        let declarations = projection::declarations(&source, &tree, &parents)?;
        Ok(OwnerSnapshot {
            path: path.clone(),
            source_sha256: files::hex(&Sha256::digest(text.as_bytes())),
            manifest_sha256: manifest_sha256.into(),
            source,
            declarations,
        })
    })();
    result.map_err(|error: Error| error.in_field(path.as_str()))
}

/// Check this entire owner family without opening any of its siblings or payload files.
fn validate_owner(
    source: &ObjectSource,
    spec: &OwnerSpec,
) -> Result<BTreeMap<SourcePath, Option<NativeUuid>>, Error> {
    let parts: BTreeMap<_, _> = std::iter::once(&source.document)
        .chain(&source.nested)
        .map(|part| (&part.path, part))
        .collect();
    if source.document.path != spec.main || parts.len() != spec.descriptors.len() {
        return Err(Error::root("source_identity_mismatch"));
    }
    for (path, part) in &parts {
        if spec
            .descriptors
            .get(*path)
            .is_none_or(|uuid| uuid.key() != part.descriptor.object.uuid.key())
        {
            return Err(Error::root("source_identity_mismatch").in_field(path.as_str()));
        }
    }
    check_declaration(&source.document, spec.kind, spec.name.as_deref())?;
    let mut parents = BTreeMap::new();
    let mut uuids = BTreeSet::new();
    let mut pending = vec![(&source.document, spec.parent.clone())];
    while let Some((part, parent)) = pending.pop() {
        if parents.insert(part.path.clone(), parent).is_some() {
            return Err(Error::root("source_descriptor_duplicate"));
        }
        let node = &part.descriptor.object;
        identities(node, &mut uuids)?;
        let children = references(node)?;
        if node.kind == MetadataKind::Configuration {
            // Other top-level owners are indexed at opening, not parsed as part of the root.
            continue;
        }
        let base = part
            .path
            .as_str()
            .strip_suffix(".xml")
            .ok_or_else(|| Error::root("source_path_invalid"))?;
        for (kind, name) in children {
            let path = reference_path(base, kind, &name)?;
            let child = parts
                .get(&path)
                .ok_or_else(|| Error::root("source_descriptor_missing").in_field(path.as_str()))?;
            check_declaration(child, kind, Some(&name))?;
            pending.push((child, Some(node.uuid.clone())));
        }
    }
    if parents.len() != parts.len() {
        return Err(Error::root("source_descriptor_undeclared"));
    }
    Ok(parents)
}

/// Apply the same typed name/kind rules to root declarations and nested references.
fn check_declaration(
    document: &NativeDocument,
    kind: MetadataKind,
    name: Option<&str>,
) -> Result<(), Error> {
    let node = &document.descriptor.object;
    if node.kind != kind || name.is_some_and(|name| node_name(node) != Ok(name)) {
        return Err(Error::root("source_declaration_mismatch").in_field(document.path.as_str()));
    }
    Ok(())
}

/// Only the verified Name property identifies an object; opaque fragments are not consulted.
fn node_name(node: &MetadataNode) -> Result<&str, Error> {
    match node.property(PropertyName::Name) {
        Some(PropertyValue::Text(name)) => Ok(name),
        _ => Err(Error::root("source_name_missing")),
    }
}

/// Reject a source-root alias before canonicalization and on every later owner read.
fn require_directory(root: &Path) -> Result<(), Error> {
    if fs::symlink_metadata(root)
        .map_err(|error| files::io(&error))?
        .file_type()
        .is_dir()
    {
        Ok(())
    } else {
        Err(Error::root("source_directory_required"))
    }
}

/// Check every selected component without inventorying or following unrelated filesystem entries.
fn read_text(root: &Path, path: &SourcePath) -> Result<String, Error> {
    require_directory(root)?;
    let mut selected = root.to_owned();
    let mut components = path.as_path().components().peekable();
    while let Some(component) = components.next() {
        selected.push(component);
        let kind = fs::symlink_metadata(&selected)
            .map_err(|error| files::io(&error).in_field(path.as_str()))?
            .file_type();
        if !(if components.peek().is_some() {
            kind.is_dir()
        } else {
            kind.is_file()
        }) {
            return Err(Error::root("source_entry_unsupported").in_field(path.as_str()));
        }
    }
    files::read_text(&selected).map_err(|error| error.in_field(path.as_str()))
}
