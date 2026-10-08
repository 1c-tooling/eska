//! Import declared composition into a newly owned source tree; resources keep native paths.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use crate::project::metadata_model::MetadataKind;

use super::{
    Child, Descriptor, Error, FileRole, MetadataNode, NativeDocument, ObjectSource, PROFILE,
    PropertyName, PropertyValue, RULES_VERSION, SCHEMA_VERSION, Section, SourceManifest,
    SourcePath,
    files::{self, Inventory},
};

/// Convert a supported ordinary configuration into a new ESKA source directory.
///
/// The input is read only. No existing destination is overwritten. `source.json` is written
/// last, after a complete byte snapshot has been rechecked, so interrupted trees are not ready.
///
/// # Errors
/// Returns profile, composition, path, identity, changed-input or I/O failures. On an ordinary
/// write failure only the newly claimed output is removed; cleanup failures are reported.
pub fn import_designer(input: &Path, destination: &Path) -> Result<SourceManifest, Error> {
    let inventory = Inventory::read(input)?;
    let input = fs::canonicalize(input).map_err(|error| files::io(&error))?;
    let destination = destination_path(&input, destination)?;
    let mut importer = Importer {
        root: &input,
        inventory: &inventory,
        loaded: BTreeMap::new(),
        uuids: BTreeSet::new(),
    };
    let root_path = SourcePath::parse("Configuration.xml")?;
    let root = importer.load(&root_path, MetadataKind::Configuration, None)?;
    let mut sources = BTreeMap::new();
    let configuration = ObjectSource {
        document: root,
        nested: Vec::new(),
    };
    let references = references(&configuration.document.descriptor.object)?;
    for (kind, name) in references {
        let path = reference_path("", kind, &name)?;
        let document = importer.load(&path, kind, Some(&name))?;
        let mut family = ObjectSource {
            document,
            nested: Vec::new(),
        };
        importer.nested(&mut family)?;
        sources.insert(source_path(&path)?, family);
    }
    sources.insert(SourcePath::parse("configuration.yaml")?, configuration);
    let manifest = importer.manifest(&sources)?;
    manifest.validate()?;
    let yaml: BTreeMap<_, _> = sources
        .iter()
        .map(|(path, source)| Ok((path, source.to_yaml()?)))
        .collect::<Result<_, Error>>()?;
    validate_output(&manifest, &sources)?;
    fs::create_dir(&destination).map_err(|error| files::io(&error))?;
    let result = materialize(&input, &destination, &inventory, &manifest, &yaml);
    if let Err(error) = result {
        fs::remove_dir_all(&destination).map_err(|_| {
            Error::root("source_cleanup_failed")
                .in_field(destination.to_string_lossy().into_owned())
                .in_field(error.code)
        })?;
        return Err(error);
    }
    Ok(manifest)
}

struct Importer<'a> {
    root: &'a Path,
    inventory: &'a Inventory,
    loaded: BTreeMap<SourcePath, String>,
    uuids: BTreeSet<String>,
}

impl Importer<'_> {
    /// Resolve a declared descriptor and require its actual kind/name to match that declaration.
    fn load(
        &mut self,
        path: &SourcePath,
        kind: MetadataKind,
        name: Option<&str>,
    ) -> Result<NativeDocument, Error> {
        if !self.inventory.files.contains_key(path) {
            return Err(Error::root("source_descriptor_missing").in_field(path.as_str()));
        }
        if self.loaded.contains_key(path) {
            return Err(Error::root("source_descriptor_duplicate").in_field(path.as_str()));
        }
        let descriptor = Descriptor::from_xml(&files::read_text(&self.root.join(path.as_path()))?)
            .map_err(|error| error.in_field(path.as_str()))?;
        if descriptor.object.kind != kind
            || name.is_some_and(|name| {
                descriptor.object.property(PropertyName::Name)
                    != Some(&PropertyValue::Text(name.into()))
            })
        {
            return Err(Error::root("source_declaration_mismatch").in_field(path.as_str()));
        }
        identities(&descriptor.object, &mut self.uuids)
            .map_err(|error| error.in_field(path.as_str()))?;
        self.loaded
            .insert(path.clone(), descriptor.object.uuid.key());
        Ok(NativeDocument {
            path: path.clone(),
            descriptor,
        })
    }

    /// Follow only declared external children; nested subsystems stay in the same main owner.
    fn nested(&mut self, source: &mut ObjectSource) -> Result<(), Error> {
        let mut pending = vec![(
            source.document.path.clone(),
            references(&source.document.descriptor.object)?,
        )];
        while let Some((owner, values)) = pending.pop() {
            let base = owner
                .as_str()
                .strip_suffix(".xml")
                .ok_or_else(|| Error::root("source_path_invalid"))?;
            for (kind, name) in values {
                let path = reference_path(base, kind, &name)?;
                let document = self.load(&path, kind, Some(&name))?;
                pending.push((path, references(&document.descriptor.object)?));
                source.nested.push(document);
            }
        }
        Ok(())
    }

    /// Map every native file exactly once; the map never adds undeclared metadata to composition.
    fn manifest(
        &self,
        sources: &BTreeMap<SourcePath, ObjectSource>,
    ) -> Result<SourceManifest, Error> {
        let mut files = self.inventory.files.clone();
        for (source_path, source) in sources {
            for part in std::iter::once(&source.document).chain(&source.nested) {
                let file = files
                    .get_mut(&part.path)
                    .ok_or_else(|| Error::root("source_descriptor_missing"))?;
                file.role = FileRole::Descriptor {
                    source: source_path.clone(),
                    uuid: part.descriptor.object.uuid.clone(),
                };
            }
        }
        Ok(SourceManifest {
            schema_version: SCHEMA_VERSION,
            rules_version: RULES_VERSION,
            profile: PROFILE.into(),
            input_revision: self.inventory.revision.clone(),
            directories: self.inventory.directories.clone(),
            files: files.into_values().collect(),
        })
    }
}

/// Filesystem destinations must be new and outside the selected source tree.
fn destination_path(input: &Path, destination: &Path) -> Result<PathBuf, Error> {
    let name = destination
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| Error::root("source_path_invalid"))?;
    SourcePath::parse(name)?;
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let destination = fs::canonicalize(parent)
        .map_err(|error| files::io(&error))?
        .join(name);
    if destination.starts_with(input) || input.starts_with(&destination) {
        return Err(Error::root("source_tree_overlap"));
    }
    match fs::symlink_metadata(&destination) {
        Ok(_) => Err(Error::root("source_destination_exists")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(destination),
        Err(error) => Err(files::io(&error)),
    }
}

/// Native child names are a single safe component; directory layout follows the kind registry.
fn reference_path(base: &str, kind: MetadataKind, name: &str) -> Result<SourcePath, Error> {
    SourcePath::parse(name)?;
    if name.contains('/') {
        return Err(Error::root("source_path_invalid"));
    }
    let folder = kind
        .collection_folder()
        .or(match kind {
            MetadataKind::Form => Some("Forms"),
            MetadataKind::Template => Some("Templates"),
            MetadataKind::Command => Some("Commands"),
            _ => None,
        })
        .ok_or_else(|| Error::root("source_reference_kind_unsupported"))?;
    SourcePath::parse(&format!(
        "{}{folder}/{name}.xml",
        if base.is_empty() {
            String::new()
        } else {
            format!("{base}/")
        }
    ))
}

/// One YAML replaces a main descriptor while preserving its native relative owner directory.
fn source_path(native: &SourcePath) -> Result<SourcePath, Error> {
    let base = native
        .as_str()
        .strip_suffix(".xml")
        .ok_or_else(|| Error::root("source_path_invalid"))?;
    SourcePath::parse(&format!("{base}/object.yaml"))
}

/// Inline references need a verified layout of their own; the initial profile refuses guessing.
fn references(node: &MetadataNode) -> Result<Vec<(MetadataKind, String)>, Error> {
    let mut values = Vec::new();
    for section in &node.sections {
        if let Section::Children(children) = section {
            for child in children {
                match child {
                    Child::Reference {
                        metadata_kind,
                        name,
                    } => values.push((*metadata_kind, name.clone())),
                    Child::Inline { object } if !references(object)?.is_empty() => {
                        return Err(Error::root("source_reference_owner_unsupported"));
                    }
                    Child::Inline { .. } | Child::Preserved { .. } => {}
                }
            }
        }
    }
    Ok(values)
}

/// Check every active native UUID across main and inline objects, independently of file paths.
fn identities(node: &MetadataNode, values: &mut BTreeSet<String>) -> Result<(), Error> {
    if !values.insert(node.uuid.key()) {
        return Err(Error::root("native_uuid_duplicate"));
    }
    for section in &node.sections {
        if let Section::Children(children) = section {
            for child in children {
                if let Child::Inline { object } = child {
                    identities(object, values)?;
                }
            }
        }
    }
    Ok(())
}

/// Generated source files cannot collide with preserved payloads, including case-only aliases.
fn validate_output(
    manifest: &SourceManifest,
    sources: &BTreeMap<SourcePath, ObjectSource>,
) -> Result<(), Error> {
    let mut paths = BTreeSet::from([files::MANIFEST.to_owned()]);
    for path in sources.keys().map(SourcePath::as_str).chain(
        manifest
            .files
            .iter()
            .filter(|file| matches!(file.role, FileRole::Preserved))
            .map(|file| file.path.as_str()),
    ) {
        if !paths.insert(path.to_lowercase()) {
            return Err(Error::root("source_path_collision").in_field(path));
        }
    }
    for path in &paths {
        for (index, _) in path.match_indices('/') {
            if paths.contains(&path[..index]) {
                return Err(Error::root("source_path_collision").in_field(path.clone()));
            }
        }
    }
    let mut spellings = BTreeMap::new();
    for path in sources.keys().map(SourcePath::as_str).chain(
        manifest
            .files
            .iter()
            .filter(|file| matches!(file.role, FileRole::Preserved))
            .map(|file| file.path.as_str()),
    ) {
        for end in path
            .match_indices('/')
            .map(|(index, _)| index)
            .chain(std::iter::once(path.len()))
        {
            let part = &path[..end];
            if spellings
                .insert(part.to_lowercase(), part)
                .is_some_and(|previous| previous != part)
            {
                return Err(Error::root("source_path_collision").in_field(path));
            }
        }
    }
    Ok(())
}

/// Publish the ready marker only after all output bytes and the complete original snapshot agree.
fn materialize(
    input: &Path,
    destination: &Path,
    inventory: &Inventory,
    manifest: &SourceManifest,
    yaml: &BTreeMap<&SourcePath, String>,
) -> Result<(), Error> {
    for (path, source) in yaml {
        files::write(&destination.join(path.as_path()), source.as_bytes())?;
    }
    for file in &manifest.files {
        if matches!(file.role, FileRole::Preserved) {
            files::copy(input, destination, file)?;
        }
    }
    if Inventory::read(input)? != *inventory {
        return Err(Error::root("source_input_changed"));
    }
    let json =
        serde_json::to_vec_pretty(manifest).map_err(|_| Error::root("source_schema_invalid"))?;
    files::write(&destination.join(files::MANIFEST), &json)
}
