//! Import declared composition into a new source tree; payloads retain exact native identity.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use crate::project_core::MetadataKind;

use super::{
    Descriptor, Error, FileRole, NativeDocument, ObjectSource, PROFILE, PropertyName,
    PropertyValue, RULES_VERSION, SCHEMA_VERSION, SourceManifest, SourcePath,
    composition::{identities, reference_path, references, source_path},
    files::{self, Inventory},
    layout::resources::{self, Layout},
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
    import(input, destination, Layout::Family)
}

/// Import independently saved descriptors into separate authoring files with reverse-map v2.
///
/// Existing family projects are not changed; both layouts use the same native validation rules.
/// Resource bytes and native paths remain unchanged in this first separated-layout profile.
///
/// # Errors
/// Returns the same profile, composition, destination and changed-input failures as family import.
pub fn import_designer_separated(
    input: &Path,
    destination: &Path,
) -> Result<SourceManifest, Error> {
    import(input, destination, Layout::Descriptors)
}

/// Import separate descriptors and explicit payload paths without the universal `Ext` directory.
///
/// Opaque bytes and the internal paths of resource sets are preserved. Existing projects and the
/// default import remain unchanged; the caller explicitly selects this reverse-map v3 layout.
///
/// # Errors
/// Returns unsupported native input, composition, collisions, destination or changed-input failures.
pub fn import_designer_resources(
    input: &Path,
    destination: &Path,
) -> Result<SourceManifest, Error> {
    import(input, destination, Layout::Resources)
}

/// Import named typed properties with native details separated, using explicit resource paths.
///
/// This opt-in descriptor schema does not alter existing projects or the default import layout.
///
/// # Errors
/// Returns native profile, identity, composition, path collision or publication failures.
pub fn import_designer_readable(input: &Path, destination: &Path) -> Result<SourceManifest, Error> {
    import(input, destination, Layout::Readable)
}

/// Import each declared managed dialog and its readable metadata into one standalone form YAML.
///
/// This explicit acceptance layout uses reverse-map v4; no existing import default is changed.
/// Unverified dialog profiles refuse the entire import before the destination is created.
///
/// # Errors
/// Returns native profile, dialog codec, composition, collision or changed-input failures.
pub fn import_designer_forms(input: &Path, destination: &Path) -> Result<SourceManifest, Error> {
    import(input, destination, Layout::Forms)
}

/// Import sole plain query values with explicit resource links and unified form YAML in map v5.
///
/// This opt-in acceptance format does not alter existing projects or select supplier permissions.
///
/// # Errors
/// Returns native/query profile, unsafe paths, collisions, publication or changed-input failures.
pub fn import_designer_queries(input: &Path, destination: &Path) -> Result<SourceManifest, Error> {
    import(input, destination, Layout::Queries)
}

/// Stage all parsed descriptors before publishing the explicitly selected physical layout.
fn import(input: &Path, destination: &Path, layout: Layout) -> Result<SourceManifest, Error> {
    let inventory = Inventory::read(input)?;
    let input = fs::canonicalize(input).map_err(|error| files::io(&error))?;
    let destination = destination_path(&input, destination)?;
    let mut importer = Importer {
        root: &input,
        inventory: &inventory,
        loaded: BTreeMap::new(),
        uuids: BTreeSet::new(),
        external_root: None,
    };
    let mut sources = importer.sources(layout)?;
    let mut manifest = importer.manifest(&sources)?;
    if layout.version() >= super::DESCRIPTOR_LAYOUT_VERSION {
        manifest.schema_version = layout.version();
        manifest.descriptor_contexts =
            super::layout::contexts(&super::layout::documents(&sources))?;
    }
    if matches!(
        layout,
        Layout::Resources | Layout::Readable | Layout::Forms | Layout::Queries
    ) {
        resources::relocate(&mut manifest)?;
    }
    if matches!(layout, Layout::Readable | Layout::Forms | Layout::Queries) {
        for source in sources.values_mut() {
            super::readable::upgrade(source);
        }
    }
    if matches!(layout, Layout::Forms | Layout::Queries) {
        super::layout::forms::attach(&input, &mut manifest, &mut sources)?;
    }
    let extra = if matches!(layout, Layout::Queries) {
        super::layout::queries::extract(&mut manifest, &mut sources, |file| {
            files::read_text(&input.join(file.path.as_path()))
        })?
    } else {
        BTreeMap::new()
    };
    if manifest.external_root.is_some() {
        manifest.schema_version = super::EXTERNAL_LAYOUT_VERSION;
        super::root::declare_optional_module(&mut manifest, &inventory)?;
    }
    manifest.validate()?;
    let yaml: BTreeMap<_, _> = sources
        .iter()
        .map(|(path, source)| Ok((path, source.to_yaml()?)))
        .collect::<Result<_, Error>>()?;
    validate_output(&manifest, &sources)?;
    let manifest_text = super::package::manifest_text(&manifest)?;
    fs::create_dir(&destination).map_err(|error| files::io(&error))?;
    let result = materialize(
        &input,
        &destination,
        &inventory,
        &manifest,
        &yaml,
        &extra,
        &manifest_text,
    );
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
    external_root: Option<super::ExternalRoot>,
}

impl Importer<'_> {
    /// Load only the composition declared by the selected native root.
    fn sources(&mut self, layout: Layout) -> Result<BTreeMap<SourcePath, ObjectSource>, Error> {
        let (root_path, root_kind, external_root) = discover_root(self.root, self.inventory)?;
        if external_root.is_some() && !matches!(layout, Layout::Queries) {
            return Err(Error::root("source_profile_unsupported"));
        }
        self.external_root = external_root;
        let root = self.load(&root_path, root_kind, None)?;
        let mut sources = BTreeMap::new();
        let configuration = ObjectSource {
            document: root,
            nested: Vec::new(),
            form: None,
        };
        let references = references(&configuration.document.descriptor.object)?;
        for (kind, name) in references {
            let base = if self.external_root.is_some() {
                root_path
                    .as_str()
                    .strip_suffix(".xml")
                    .ok_or_else(|| Error::root("source_path_invalid"))?
            } else {
                ""
            };
            let path = reference_path(base, kind, &name)?;
            let document = self.load(&path, kind, Some(&name))?;
            let mut family = ObjectSource {
                document,
                nested: Vec::new(),
                form: None,
            };
            self.nested(&mut family)?;
            sources.insert(source_path(&path)?, family);
        }
        let root_source = self
            .external_root
            .as_ref()
            .map_or("configuration.yaml", |root| root.kind.source());
        sources.insert(SourcePath::parse(root_source)?, configuration);
        let sources = if layout.version() >= super::DESCRIPTOR_LAYOUT_VERSION {
            super::layout::separate(sources, self.external_root.as_ref())?
        } else {
            sources
        };
        Ok(sources)
    }

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
            profile: self
                .external_root
                .as_ref()
                .map_or(PROFILE, |root| root.kind.profile())
                .into(),
            external_root: self.external_root.clone(),
            input_revision: self.inventory.revision.clone(),
            directories: self.inventory.directories.clone(),
            files: files.into_values().collect(),
            descriptor_contexts: Vec::new(),
            query_resources: Vec::new(),
        })
    }
}

/// Immediate, bounded native descriptors choose the profile; unrelated XML remains payload.
fn discover_root(
    input: &Path,
    inventory: &Inventory,
) -> Result<(SourcePath, MetadataKind, Option<super::ExternalRoot>), Error> {
    use crate::project_core::{ProjectType, native};
    let mut roots = Vec::new();
    for path in inventory.files.keys().filter(|path| {
        !path.as_str().contains('/')
            && path
                .as_path()
                .extension()
                .is_some_and(|extension| extension == "xml")
    }) {
        let text = match files::read_text(&input.join(path.as_path())) {
            Ok(text) => text,
            Err(error)
                if path.as_str() != "Configuration.xml"
                    && matches!(
                        error.code,
                        "source_encoding_unsupported" | "source_document_too_large"
                    ) =>
            {
                continue;
            }
            Err(error) => return Err(error.in_field(path.as_str())),
        };
        let project_type = native::project_type(&text).ok().flatten();
        match project_type {
            Some(ProjectType::Configuration | ProjectType::Extension)
                if path.as_str() == "Configuration.xml" =>
            {
                roots.push((path.clone(), MetadataKind::Configuration, None));
            }
            Some(ProjectType::Processing | ProjectType::Report) => {
                let kind = if project_type == Some(ProjectType::Processing) {
                    super::ExternalKind::Processing
                } else {
                    super::ExternalKind::Report
                };
                roots.push((
                    path.clone(),
                    kind.metadata_kind(),
                    Some(super::ExternalRoot {
                        path: path.clone(),
                        kind,
                    }),
                ));
            }
            Some(ProjectType::Configuration | ProjectType::Extension) => {
                return Err(Error::root("source_profile_unsupported").in_field(path.as_str()));
            }
            None if path.as_str() == "Configuration.xml" => {
                Descriptor::from_xml(&text).map_err(|error| error.in_field(path.as_str()))?;
                return Err(Error::root("source_profile_unsupported"));
            }
            None => {}
        }
    }
    if roots.len() > 1 {
        return Err(Error::root("source_descriptor_duplicate"));
    }
    roots
        .pop()
        .ok_or_else(|| Error::root("source_descriptor_missing"))
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
            .filter(|file| file.is_payload())
            .map(|file| file.source_path().as_str()),
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
            .filter(|file| file.is_payload())
            .map(|file| file.source_path().as_str()),
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
    extra: &BTreeMap<SourcePath, String>,
    manifest_text: &str,
) -> Result<(), Error> {
    for (path, source) in yaml {
        files::write(&destination.join(path.as_path()), source.as_bytes())?;
    }
    for (path, source) in extra {
        files::write(&destination.join(path.as_path()), source.as_bytes())?;
    }
    for file in &manifest.files {
        if file.is_payload()
            && !matches!(file.role, FileRole::OptionalModule { .. })
            && !extra.contains_key(file.source_path())
        {
            files::copy_to(input, &destination.join(file.source_path().as_path()), file)?;
        }
    }
    if manifest.schema_version >= super::RESOURCE_LAYOUT_VERSION {
        for directory in resources::source_directories(manifest)? {
            fs::create_dir_all(destination.join(directory.as_path()))
                .map_err(|error| files::io(&error))?;
        }
    }
    if Inventory::read(input)? != *inventory {
        return Err(Error::root("source_input_changed"));
    }
    files::write(&destination.join(files::MANIFEST), manifest_text.as_bytes())
}
