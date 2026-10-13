//! Owner-local metadata snapshots; opening a tree never reads modules, resources or a baseline.

mod form_dialog;
mod modules;
mod predefined;
mod projection;
mod role_rights;
mod semantic;
mod standalone;

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
use crate::project_core::metadata_model::ObjectId;

use super::{
    Error, FileRole, MetadataNode, NativeDocument, NativeUuid, ObjectSource, PropertyName,
    PropertyValue, SourceManifest, SourcePath,
    composition::{identities, reference_path, references, source_path},
    files, yaml,
};

pub use form_dialog::DeclaredForm;
pub use modules::DeclaredModule;
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

/// Stable facts from the validated composition, available before reading an owner's YAML.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeclaredOwner<'a> {
    pub kind: MetadataKind,
    /// The configuration has no enclosing declaration; its name lives in the root snapshot.
    pub name: Option<&'a str>,
    pub native_path: &'a SourcePath,
    pub uuid: &'a NativeUuid,
    /// Nested physical files retain the logical parent's context; top-level IDs stay unqualified.
    pub parent: Option<&'a ObjectId>,
}

/// A declared owner has one main descriptor and the complete native identities of its family.
struct OwnerSpec {
    main: SourcePath,
    kind: MetadataKind,
    name: Option<String>,
    parent: Option<NativeUuid>,
    descriptors: BTreeMap<SourcePath, NativeUuid>,
    rights: Option<SourcePath>,
    predefined: Option<SourcePath>,
    logical_parent: Option<ObjectId>,
    /// Separated files validate their external declarations against mandatory reverse contexts.
    external: Option<BTreeMap<SourcePath, (MetadataKind, String)>>,
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
    native_owners: BTreeMap<SourcePath, SourcePath>,
    modules: BTreeMap<SourcePath, modules::ModuleSpec>,
    role_rights: BTreeMap<SourcePath, SourcePath>,
    predefined: BTreeMap<SourcePath, SourcePath>,
    forms: BTreeMap<SourcePath, form_dialog::FormSpec>,
    form_dialogs: BTreeMap<SourcePath, SourcePath>,
}

impl SourceReader {
    /// Read only the file map and configuration; missing unrelated payloads do not block a view.
    ///
    /// # Errors
    /// Returns unsupported schema, unsafe path, root composition or native identity failures.
    pub fn open(root: &Path) -> Result<Self, Error> {
        require_directory(root)?;
        let pending = match fs::symlink_metadata(root.join(super::layout::PENDING_RECEIPT)) {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(files::io(&error)),
        };
        let root = fs::canonicalize(root).map_err(|error| files::io(&error))?;
        let text = read_text(&root, &SourcePath::parse(files::MANIFEST)?).map_err(|error| {
            if pending {
                Error::root("source_layout_incomplete")
            } else {
                error
            }
        })?;
        let manifest: SourceManifest =
            serde_json::from_str(&text).map_err(|_| Error::root("source_schema_invalid"))?;
        if pending && manifest.schema_version >= super::DESCRIPTOR_LAYOUT_VERSION {
            return Err(Error::root("source_layout_incomplete"));
        }
        manifest.validate()?;
        let manifest_sha256 = files::hex(&Sha256::digest(text.as_bytes()));
        let mut groups = descriptor_groups(&manifest)?;
        let root_path = SourcePath::parse("configuration.yaml")?;
        let root_spec = OwnerSpec {
            main: SourcePath::parse("Configuration.xml")?,
            kind: MetadataKind::Configuration,
            name: None,
            parent: None,
            rights: None,
            predefined: None,
            logical_parent: None,
            external: None,
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
        let owners = if manifest.schema_version >= super::DESCRIPTOR_LAYOUT_VERSION {
            standalone::owners(&manifest, &configuration, groups, root_spec)?
        } else {
            family_owners(&configuration, groups, root_spec)?
        };
        let mut owners = owners;
        let native_owners = owners
            .iter()
            .map(|(source, spec)| (spec.main.clone(), source.clone()))
            .collect();
        let modules = modules::declared_modules(&manifest, &owners)?;
        let role_rights = role_rights::declared_role_rights(&manifest, &mut owners)?;
        let predefined = predefined::declared_predefined(&manifest, &mut owners)?;
        let mut forms = form_dialog::declared_forms(&owners)?;
        let form_dialogs = form_dialog::declared_form_dialogs(&manifest, &mut forms)?;
        Ok(Self {
            root,
            manifest,
            manifest_sha256,
            configuration,
            owners,
            native_owners,
            modules,
            role_rights,
            predefined,
            forms,
            form_dialogs,
        })
    }

    /// Return the root facts read at opening; reopen when this file or the manifest changes.
    #[must_use]
    pub const fn configuration(&self) -> &OwnerSnapshot {
        &self.configuration
    }

    /// Return the canonical directory bound to this manifest and composition snapshot.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
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

    /// Resolve a declared main descriptor to its exact mapped physical file without reading text.
    /// Legacy nested documents remain family members; separated documents have independent slots.
    #[must_use]
    pub fn owner_for_native_main(&self, native: &SourcePath) -> Option<&SourcePath> {
        self.native_owners.get(native)
    }

    /// Read structural identity without opening the selected YAML or inferring a directory name.
    ///
    /// # Errors
    /// Returns an undeclared owner or an inconsistent main identity.
    pub fn declared_owner(&self, path: &SourcePath) -> Result<DeclaredOwner<'_>, Error> {
        let spec = self.spec(path)?;
        let uuid = spec
            .descriptors
            .get(&spec.main)
            .ok_or_else(|| Error::root("source_identity_mismatch").in_field(path.as_str()))?;
        Ok(DeclaredOwner {
            kind: spec.kind,
            name: spec.name.as_deref(),
            native_path: &spec.main,
            uuid,
            parent: spec.logical_parent.as_ref(),
        })
    }

    /// Enumerate only manifest modules attached to declared descriptor locations.
    pub fn module_paths(&self) -> impl Iterator<Item = &SourcePath> {
        self.modules.keys()
    }

    /// Read stable ownership and encoding without opening a module or its owner YAML.
    #[must_use]
    pub fn declared_module(&self, path: &SourcePath) -> Option<DeclaredModule<'_>> {
        self.modules.get(path).map(|module| DeclaredModule {
            owner: &module.owner,
            native_descriptor: &module.native_descriptor,
            native_path: &module.native_path,
            binary: module.binary,
        })
    }

    /// Read the current text of one declared module, preserving BOM and line endings.
    ///
    /// # Errors
    /// Returns an undeclared/binary module, unsafe entry, I/O, encoding or byte-limit failure.
    pub fn read_module_text(&self, path: &SourcePath, max_bytes: u64) -> Result<String, Error> {
        let module = self
            .declared_module(path)
            .ok_or_else(|| Error::root("source_module_undeclared").in_field(path.as_str()))?;
        if module.binary {
            return Err(Error::root("source_module_binary").in_field(path.as_str()));
        }
        read_text_limited(&self.root, path, max_bytes)
    }

    /// Enumerate preserved rights attached to declared roles without reading any payload.
    pub fn role_rights_paths(&self) -> impl Iterator<Item = &SourcePath> {
        self.role_rights.keys()
    }

    /// Return a role's declared resource; absence differs from a mapped but unreadable file.
    #[must_use]
    pub fn role_rights_path(&self, owner: &SourcePath) -> Option<&SourcePath> {
        self.owners.get(owner).and_then(|spec| spec.rights.as_ref())
    }

    /// Bind a declared rights resource to its authoritative YAML owner without reading either file.
    #[must_use]
    pub fn role_rights_owner(&self, path: &SourcePath) -> Option<&SourcePath> {
        self.role_rights.get(path)
    }

    /// Read current verbatim UTF-8 rights bytes through the same path and size checks as metadata.
    ///
    /// This method does not validate XML or interpret permissions; those are consumer concerns.
    ///
    /// # Errors
    /// Returns undeclared/unsafe paths, missing files, unsupported encoding or an exceeded byte limit.
    pub fn read_role_rights_text(
        &self,
        path: &SourcePath,
        max_bytes: u64,
    ) -> Result<String, Error> {
        if self.role_rights_owner(path).is_none() {
            return Err(Error::root("source_role_rights_undeclared").in_field(path.as_str()));
        }
        read_text_limited(&self.root, path, max_bytes)
    }

    /// Enumerate preserved predefined-data sidecars attached to declared catalog/chart owners.
    pub fn predefined_paths(&self) -> impl Iterator<Item = &SourcePath> {
        self.predefined.keys()
    }

    /// A missing declaration differs from a declared resource whose current bytes are unavailable.
    #[must_use]
    pub fn predefined_path(&self, owner: &SourcePath) -> Option<&SourcePath> {
        self.owners
            .get(owner)
            .and_then(|spec| spec.predefined.as_ref())
    }

    /// Bind current predefined data to its enclosing YAML independently of either payload's validity.
    #[must_use]
    pub fn predefined_owner(&self, resource: &SourcePath) -> Option<&SourcePath> {
        self.predefined.get(resource)
    }

    /// Read exact current UTF-8 bytes without interpreting XML or manufacturing predefined names.
    ///
    /// # Errors
    /// Returns undeclared/unsafe paths, missing files, unsupported encoding or an exceeded byte limit.
    pub fn read_predefined_text(&self, path: &SourcePath, max_bytes: u64) -> Result<String, Error> {
        if self.predefined_owner(path).is_none() {
            return Err(Error::root("source_predefined_undeclared").in_field(path.as_str()));
        }
        read_text_limited(&self.root, path, max_bytes)
    }

    /// Enumerate full form descriptor paths, including nested forms, without opening their owners.
    pub fn form_paths(&self) -> impl Iterator<Item = &SourcePath> {
        self.forms.keys()
    }

    /// Read a form's stable owner, native identity and optional dialog without loading payloads.
    #[must_use]
    pub fn declared_form(&self, native: &SourcePath) -> Option<DeclaredForm<'_>> {
        let (path, spec) = self.forms.get_key_value(native)?;
        Some(DeclaredForm {
            owner: &spec.owner,
            native_descriptor: path,
            kind: spec.kind,
            name: &spec.name,
            uuid: &spec.uuid,
            dialog: spec.dialog.as_ref(),
        })
    }

    /// Enumerate preserved dialogs attached to declared common and nested forms.
    pub fn form_dialog_paths(&self) -> impl Iterator<Item = &SourcePath> {
        self.form_dialogs.keys()
    }

    /// Return a common form's declared dialog; a missing declared file remains distinguishable.
    #[must_use]
    pub fn form_dialog_path(&self, owner: &SourcePath) -> Option<&SourcePath> {
        let main = &self.owners.get(owner)?.main;
        self.forms.get(main).and_then(|spec| spec.dialog.as_ref())
    }

    /// Bind one declared form dialog to its YAML owner without reading either payload.
    #[must_use]
    pub fn form_dialog_owner(&self, path: &SourcePath) -> Option<&SourcePath> {
        let form = self.form_dialogs.get(path)?;
        self.forms.get(form).map(|spec| &spec.owner)
    }

    /// Resolve a dialog to its full form identity, preserving equal names in different owners.
    #[must_use]
    pub fn form_for_dialog(&self, path: &SourcePath) -> Option<DeclaredForm<'_>> {
        self.declared_form(self.form_dialogs.get(path)?)
    }

    /// Read native dialog XML, deriving mapped v4 dialogs from their sole form YAML source.
    ///
    /// Earlier layouts return verbatim XML. Both physical input and generated XML obey the limit.
    ///
    /// # Errors
    /// Returns undeclared/unsafe paths, missing files, unsupported encoding or an exceeded byte limit.
    pub fn read_form_dialog_text(
        &self,
        path: &SourcePath,
        max_bytes: u64,
    ) -> Result<String, Error> {
        let text = self.read_form_authoring_text(path, max_bytes)?;
        if self.embedded_form(path) {
            let xml = self.parse_form_source(path, &text)?.to_xml()?;
            if xml.len() as u64 > max_bytes {
                return Err(Error::root("source_document_too_large").in_field(path.as_str()));
            }
            Ok(xml)
        } else {
            Ok(text)
        }
    }

    /// Read one declared dialog's typed identity/layout while keeping opaque resources unchanged.
    ///
    /// # Errors
    /// Returns the bounded text-reader errors or an unverified/invalid managed-form layout.
    pub fn read_form_layout(
        &self,
        path: &SourcePath,
        max_bytes: u64,
    ) -> Result<super::ManagedFormLayout, Error> {
        let text = self.read_form_authoring_text(path, max_bytes)?;
        self.parse_form_layout(path, &text)
    }

    /// Read the declared native dialog or its mapped standalone YAML body, without opening neighbors.
    ///
    /// # Errors
    /// Returns bounded text-read failures, unverified lexical bindings or an invalid managed form.
    pub fn read_form_source(
        &self,
        path: &SourcePath,
        max_bytes: u64,
    ) -> Result<super::ManagedFormSource, Error> {
        let text = self.read_form_authoring_text(path, max_bytes)?;
        self.parse_form_source(path, &text)
    }

    /// Decode the declared format of an unsaved dialog; v4 expects complete standalone form YAML.
    ///
    /// # Errors
    /// Returns an undeclared dialog or invalid/unverified native source bindings.
    pub fn parse_form_source(
        &self,
        path: &SourcePath,
        text: &str,
    ) -> Result<super::ManagedFormSource, Error> {
        if self.form_dialog_owner(path).is_none() {
            return Err(Error::root("source_form_dialog_undeclared").in_field(path.as_str()));
        }
        if self.embedded_form(path) {
            self.parse_owner(path, text)?
                .source
                .form
                .ok_or_else(|| Error::root("source_form_schema_invalid").in_field(path.as_str()))
        } else {
            super::ManagedFormSource::from_xml(text).map_err(|error| error.in_field(path.as_str()))
        }
    }

    /// Parse an addressed editor overlay without discovering new resources or reading neighboring forms.
    ///
    /// # Errors
    /// Returns an undeclared dialog or an unverified/invalid managed-form layout.
    pub fn parse_form_layout(
        &self,
        path: &SourcePath,
        text: &str,
    ) -> Result<super::ManagedFormLayout, Error> {
        if self.form_dialog_owner(path).is_none() {
            return Err(Error::root("source_form_dialog_undeclared").in_field(path.as_str()));
        }
        if self.embedded_form(path) {
            return self.parse_form_source(path, text)?.layout_snapshot();
        }
        super::ManagedFormLayout::from_xml(text).map_err(|error| error.in_field(path.as_str()))
    }

    /// Manifest ownership selects YAML explicitly; content sniffing cannot change a document's kind.
    fn embedded_form(&self, path: &SourcePath) -> bool {
        self.manifest.schema_version == super::MANAGED_FORM_LAYOUT_VERSION
            && self.form_dialogs.contains_key(path)
    }

    /// All addressed dialog reads check declaration before touching a physical file.
    fn read_form_authoring_text(&self, path: &SourcePath, max_bytes: u64) -> Result<String, Error> {
        if self.form_dialog_owner(path).is_none() {
            return Err(Error::root("source_form_dialog_undeclared").in_field(path.as_str()));
        }
        read_text_limited(&self.root, path, max_bytes)
    }

    /// Read exact declared-owner bytes for a bounded analysis request or editor snapshot.
    ///
    /// # Errors
    /// Returns undeclared/unsafe paths, missing files, encoding failures or the tightened limit.
    pub fn read_owner_text(&self, path: &SourcePath, max_bytes: u64) -> Result<String, Error> {
        self.spec(path)?;
        read_text_limited(&self.root, path, max_bytes)
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
        super::layout::forms::validate_source(&result.source, self.embedded_form(path))
            .map_err(|error| error.in_field(path.as_str()))?;
        if result.source.document.descriptor.object.kind == MetadataKind::Configuration
            && !super::composition::same_reference_members(
                &result.source.document.descriptor.object,
                &self.configuration.source.document.descriptor.object,
            )?
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
            super::layout::forms::validate_source(&source, self.embedded_form(path))?;
            let parents = validate_owner(&source, self.spec(path)?)?;
            if source.document.descriptor.object.kind == MetadataKind::Configuration
                && !super::composition::same_reference_members(
                    &source.document.descriptor.object,
                    &self.configuration.source.document.descriptor.object,
                )?
            {
                return Err(Error::root("source_declaration_mismatch"));
            }
            let documents = semantic::documents(
                &source,
                &tree,
                &parents,
                self.spec(path)?.logical_parent.as_ref(),
                mode,
            )?;
            let offsets = documents
                .iter()
                .flat_map(|document| {
                    document
                        .descriptor
                        .objects
                        .iter()
                        .flat_map(|object| {
                            std::iter::once(&object.range).chain(
                                object
                                    .properties
                                    .iter()
                                    .flatten()
                                    .map(|property| &property.range),
                            )
                        })
                        .chain(
                            document
                                .descriptor
                                .diagnostics
                                .iter()
                                .map(|diagnostic| &diagnostic.range),
                        )
                })
                .flat_map(|range| [range.start, range.end])
                .collect::<Vec<_>>();
            let coordinates = super::TextCoordinates::new(text, &offsets)
                .map_err(|_| Error::root("source_coordinates_invalid"))?;
            Ok(MetadataSnapshot {
                path: path.clone(),
                source_sha256: files::hex(&Sha256::digest(text.as_bytes())),
                source_bytes: text.len() as u64,
                manifest_sha256: self.manifest_sha256.clone(),
                documents,
                coordinates,
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

/// Keep legacy family grouping and deferred reads unchanged until an explicit layout migration.
fn family_owners(
    configuration: &OwnerSnapshot,
    mut groups: BTreeMap<SourcePath, BTreeMap<SourcePath, NativeUuid>>,
    root_spec: OwnerSpec,
) -> Result<BTreeMap<SourcePath, OwnerSpec>, Error> {
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
                rights: None,
                predefined: None,
                logical_parent: None,
                external: None,
                descriptors,
            },
        );
    }
    if !groups.is_empty() {
        return Err(Error::root("source_descriptor_undeclared"));
    }
    owners.insert(configuration.path.clone(), root_spec);
    Ok(owners)
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
        if let Some(external) = &spec.external {
            let base = if node.kind == MetadataKind::Configuration {
                ""
            } else {
                part.path
                    .as_str()
                    .strip_suffix(".xml")
                    .ok_or_else(|| Error::root("source_path_invalid"))?
            };
            let mut actual = BTreeMap::new();
            for (kind, name) in children {
                if actual
                    .insert(reference_path(base, kind, &name)?, (kind, name))
                    .is_some()
                {
                    return Err(Error::root("source_descriptor_duplicate"));
                }
            }
            if actual != *external {
                return Err(Error::root("source_declaration_mismatch"));
            }
            continue;
        }
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
pub(super) fn node_name(node: &MetadataNode) -> Result<&str, Error> {
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
    read_text_limited(root, path, files::MAX_TEXT_BYTES)
}

/// Check selected path components before the bounded read; no unrelated payload is opened.
fn read_text_limited(root: &Path, path: &SourcePath, max_bytes: u64) -> Result<String, Error> {
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
    files::read_text_limited(&selected, max_bytes).map_err(|error| error.in_field(path.as_str()))
}
