//! A verified path transition changes bindings while retaining original policy and protected values.

mod addition;
mod deletion;
mod queries;
mod references;

use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest, Sha256};

use super::{AcceptedBase, Error, FileEntry, Inventory, PropertyName, SourcePath, files};
use crate::project_core::source_format::{
    Child, NativeDocument, ObjectSource, Operation, Property, Section, export::Package,
    file_identity, package::manifest_text,
};

/// Original bytes remain verifiable even when the effective package uses renamed authoring paths.
pub(super) struct Bindings {
    pub original: Inventory,
    pub texts: BTreeMap<SourcePath, String>,
    pub origins: BTreeMap<SourcePath, FileEntry>,
    pub created: BTreeSet<String>,
}

impl AcceptedBase {
    /// Bind renamed objects by UUID, retaining the independently approved policy and native stencils.
    pub(in crate::project_core::source_format) fn with_composition(
        &self,
        candidate: &Package,
        mut read: impl FnMut(&SourcePath) -> Result<String, Error>,
    ) -> Result<Self, Error> {
        let added = addition::validate(self, candidate)?;
        validate_identity(self, candidate, &added)?;
        validate_reference_order(self, candidate)?;
        let before = file_identity::files(&self.package)?;
        let after = file_identity::files(candidate)?;
        let old_documents = self
            .package
            .documents
            .values()
            .map(|document| (document.descriptor.object.uuid.key(), document))
            .collect::<BTreeMap<_, _>>();
        let mut documents = BTreeMap::new();
        let surviving = candidate
            .documents
            .values()
            .map(|document| document.descriptor.object.uuid.key())
            .collect::<BTreeSet<_>>();
        let renames = references::renames(&self.package, candidate)?;
        for document in candidate.documents.values() {
            let Some(old) = old_documents.get(&document.descriptor.object.uuid.key()) else {
                documents.insert(document.path.clone(), document.clone());
                continue;
            };
            let mut selected = (*old).clone();
            deletion::retain_references(self, &surviving, &mut selected)?;
            insert_references(self, candidate, &added, &mut selected, document)?;
            bind_document(self, &mut selected, document)?;
            references::rebind(self, &mut selected, document, &renames)?;
            documents.insert(selected.path.clone(), selected);
        }
        let source_paths = before
            .iter()
            .filter_map(|(identity, old)| after.get(identity).map(|next| (old, next)))
            .collect::<BTreeMap<_, _>>();
        let original_paths = source_paths
            .iter()
            .map(|(old, new)| (*new, *old))
            .collect::<BTreeMap<_, _>>();
        let mut texts = BTreeMap::new();
        let mut snapshot = candidate.snapshot.clone();
        let origins = select_original_files(self, &before, &after, &mut snapshot)?;
        for (source, paths) in &candidate.owners {
            let mut owner = owner_source(self, candidate, &documents, &before, paths)?;
            rebind_form(self, &mut owner, &added, &source_paths)?;
            insert_text(&mut snapshot, &mut texts, source.clone(), owner.to_yaml()?);
        }
        rebind_resources(
            self,
            candidate,
            &source_paths,
            &original_paths,
            &mut snapshot,
            &mut texts,
        )?;
        let marker = SourcePath::parse(files::MANIFEST)?;
        insert_text(
            &mut snapshot,
            &mut texts,
            marker,
            manifest_text(&candidate.manifest)?,
        );
        snapshot.revision = files::revision(&snapshot.files, &snapshot.directories);
        let package = Package::from_snapshot(snapshot, |path| {
            if let Some(text) = texts.get(path) {
                return Ok(text.clone());
            }
            if let Some(old) = original_paths.get(path) {
                return self.read_file(old);
            }
            let text = read(path)?;
            texts.insert(path.clone(), text.clone());
            Ok(text)
        })?;
        let mut support = self.support.clone();
        support.objects.extend(
            super::document_ownership(&package.documents)?
                .into_iter()
                .filter(|(uuid, _)| added.contains(uuid)),
        );
        Ok(Self {
            root: self.root.clone(),
            reference: self.reference.clone(),
            manifest: self.manifest.clone(),
            support,
            source_binding: None,
            composition: Some(Bindings {
                original: self.package.snapshot.clone(),
                texts,
                origins,
                created: added,
            }),
            package,
        })
    }

    /// Origin records identify actual pinned bytes, never serialized composition stencils.
    pub(in crate::project_core::source_format) fn origin_entry(
        &self,
        path: &SourcePath,
    ) -> Option<&FileEntry> {
        self.composition.as_ref().map_or_else(
            || self.package.snapshot.files.get(path),
            |bindings| bindings.origins.get(path),
        )
    }

    /// A changed composition uses a map version that permits distinct accepted and working paths.
    pub(in crate::project_core::source_format) const fn composition_changed(&self) -> bool {
        self.composition.is_some()
    }

    /// A new owner has working evidence only; it cannot claim an original accepted input.
    pub(in crate::project_core::source_format) fn has_created_owners(&self) -> bool {
        self.composition
            .as_ref()
            .is_some_and(|bindings| !bindings.created.is_empty())
    }

    /// A family YAML may contain both original owners and new nested descriptor identities.
    pub(in crate::project_core::source_format) fn created_owner(
        &self,
        uuid: &super::NativeUuid,
    ) -> bool {
        self.composition
            .as_ref()
            .is_some_and(|bindings| bindings.created.contains(&uuid.key()))
    }
}

/// Existing form stencils retain pinned query bindings while created owners use supplied new slots.
fn rebind_form(
    base: &AcceptedBase,
    owner: &mut ObjectSource,
    added: &BTreeSet<String>,
    source_paths: &BTreeMap<&SourcePath, &SourcePath>,
) -> Result<(), Error> {
    if let Some(form) = &owner.form
        && !added.contains(&owner.document.descriptor.object.uuid.key())
    {
        owner.form = Some(form.with_query_references(&queries::rebind(
            base,
            &form.to_xml()?,
            source_paths,
        )?)?);
    }
    Ok(())
}

/// A renamed reference must retain the same UUID and position; frozen order cannot be laundered.
fn validate_reference_order(base: &AcceptedBase, candidate: &Package) -> Result<(), Error> {
    let current = candidate
        .documents
        .values()
        .map(|document| (document.descriptor.object.uuid.key(), document))
        .collect::<BTreeMap<_, _>>();
    for old in base.package.documents.values() {
        let Some(new) = current.get(&old.descriptor.object.uuid.key()) else {
            continue;
        };
        let left = referenced_uuids(&base.package, old)?
            .into_iter()
            .filter(|uuid| current.contains_key(uuid))
            .collect::<Vec<_>>();
        let right = referenced_uuids(candidate, new)?
            .into_iter()
            .filter(|uuid| base.support.objects.contains_key(uuid))
            .collect::<Vec<_>>();
        if left != right {
            return Err(Error::root("source_import_mapping_unsupported"));
        }
    }
    Ok(())
}

/// Native references resolve through the validated declaration graph, not a displayed name alone.
fn referenced_uuids(package: &Package, owner: &NativeDocument) -> Result<Vec<String>, Error> {
    let folder = if owner.descriptor.object.kind == crate::project_core::MetadataKind::Configuration
    {
        ""
    } else {
        owner
            .path
            .as_str()
            .strip_suffix(".xml")
            .ok_or_else(|| Error::root("source_path_invalid"))?
    };
    super::super::composition::references(&owner.descriptor.object)?
        .into_iter()
        .map(|(kind, name)| {
            let path = super::super::composition::reference_path(folder, kind, &name)?;
            Ok(package
                .documents
                .get(&path)
                .ok_or_else(|| Error::root("source_descriptor_missing"))?
                .descriptor
                .object
                .uuid
                .key())
        })
        .collect()
}

/// Surviving declarations retain their identity and ancestry; removals require pinned rights.
fn validate_identity(
    base: &AcceptedBase,
    candidate: &Package,
    added: &BTreeSet<String>,
) -> Result<(), Error> {
    if base.package.manifest.schema_version != candidate.manifest.schema_version
        || base.package.manifest.profile != candidate.manifest.profile
        || base.package.manifest.external_root != candidate.manifest.external_root
    {
        return Err(Error::root("source_import_mapping_unsupported"));
    }
    deletion::validate(base, candidate, added)?;
    let root = base.package.manifest.root_path()?;
    let old = &base.package.documents[&root].descriptor;
    let new = candidate
        .documents
        .get(&root)
        .ok_or_else(|| Error::root("source_identity_mismatch"))?;
    if old.object.uuid != new.descriptor.object.uuid
        || old.object.property(PropertyName::Name)
            != new.descriptor.object.property(PropertyName::Name)
    {
        return Err(Error::root("source_identity_mismatch"));
    }
    Ok(())
}

/// Insert supplied new references without reordering or rewriting any surviving inline member.
fn insert_references(
    base: &AcceptedBase,
    candidate: &Package,
    added: &BTreeSet<String>,
    selected: &mut NativeDocument,
    current: &NativeDocument,
) -> Result<(), Error> {
    if added.is_empty() {
        return Ok(());
    }
    let folder =
        if current.descriptor.object.kind == crate::project_core::MetadataKind::Configuration {
            ""
        } else {
            current
                .path
                .as_str()
                .strip_suffix(".xml")
                .ok_or_else(|| Error::root("source_path_invalid"))?
        };
    for (index, section) in current.descriptor.object.sections.iter().enumerate() {
        let Section::Children(children) = section else {
            continue;
        };
        let before = match selected.descriptor.object.sections.get(index) {
            Some(Section::Children(children)) => children.as_slice(),
            None if index == selected.descriptor.object.sections.len() => &[],
            _ => return Err(Error::root("source_import_mapping_unsupported")),
        };
        let mut existing = before.iter();
        let mut result = Vec::new();
        let mut shifted = false;
        for child in children {
            let uuid = if let Child::Reference {
                metadata_kind,
                name,
            } = child
            {
                let path = super::super::composition::reference_path(folder, *metadata_kind, name)?;
                Some(
                    &candidate
                        .documents
                        .get(&path)
                        .ok_or_else(|| Error::root("source_descriptor_missing"))?
                        .descriptor
                        .object
                        .uuid,
                )
            } else {
                None
            };
            if uuid.is_some_and(|uuid| added.contains(&uuid.key())) {
                result.push(child.clone());
                shifted = true;
                continue;
            }
            let previous = existing
                .next()
                .ok_or_else(|| Error::root("source_import_mapping_unsupported"))?;
            let uuid = match (previous, child) {
                (
                    Child::Reference {
                        metadata_kind: old, ..
                    },
                    Child::Reference {
                        metadata_kind: new, ..
                    },
                ) if old == new => uuid,
                (Child::Inline { object: old }, Child::Inline { object: new })
                    if old.uuid == new.uuid && old.kind == new.kind =>
                {
                    Some(&old.uuid)
                }
                (Child::Preserved { fragment: old }, Child::Preserved { fragment: new })
                    if old == new && !shifted =>
                {
                    None
                }
                _ => return Err(Error::root("source_import_mapping_unsupported")),
            };
            if shifted
                && let Some(uuid) = uuid
                && !base.permission(uuid, Operation::Reorder).allowed
            {
                return Err(Error::root("source_import_structure_denied").in_field(uuid.as_str()));
            }
            result.push(previous.clone());
        }
        if existing.next().is_some() {
            return Err(Error::root("source_import_mapping_unsupported"));
        }
        if index == selected.descriptor.object.sections.len() {
            selected
                .descriptor
                .object
                .sections
                .push(Section::Children(result));
        } else {
            selected.descriptor.object.sections[index] = Section::Children(result);
        }
    }
    Ok(())
}

/// Names and reference labels are the sole structural values selected from the new composition.
fn bind_document(
    base: &AcceptedBase,
    selected: &mut NativeDocument,
    candidate: &NativeDocument,
) -> Result<(), Error> {
    let before = &mut selected.descriptor.object;
    let after = &candidate.descriptor.object;
    if before.property(PropertyName::Name) != after.property(PropertyName::Name) {
        let permission = base.permission(&before.uuid, Operation::Resource);
        if !permission.allowed
            || permission.state != crate::project_core::support::State::Unrestricted
        {
            return Err(
                Error::root("source_import_structure_denied").in_field(before.uuid.as_str())
            );
        }
        let name = after
            .property(PropertyName::Name)
            .cloned()
            .ok_or_else(|| Error::root("source_name_missing"))?;
        for section in &mut before.sections {
            if let Section::Properties(properties) = section {
                for property in properties {
                    if let Property::Typed {
                        name: PropertyName::Name,
                        value,
                    } = property
                    {
                        *value = name.clone();
                    }
                }
            }
        }
    }
    let mut refs = after
        .sections
        .iter()
        .filter_map(|section| {
            if let Section::Children(children) = section {
                Some(children.iter())
            } else {
                None
            }
        })
        .flatten()
        .filter(|child| matches!(child, Child::Reference { .. }));
    for section in &mut before.sections {
        if let Section::Children(children) = section {
            for child in children {
                if let Child::Reference { metadata_kind, .. } = child {
                    let next = refs
                        .next()
                        .ok_or_else(|| Error::root("source_identity_mismatch"))?;
                    if !matches!(next, Child::Reference { metadata_kind: kind, .. } if kind == metadata_kind)
                    {
                        return Err(Error::root("source_identity_mismatch"));
                    }
                    if next != child && !base.permission(&before.uuid, Operation::Resource).allowed
                    {
                        return Err(Error::root("source_import_structure_denied")
                            .in_field(before.uuid.as_str()));
                    }
                    *child = next.clone();
                }
            }
        }
    }
    if refs.next().is_some() {
        return Err(Error::root("source_identity_mismatch"));
    }
    selected.path = candidate.path.clone();
    Ok(())
}

/// Generated stencils have their own fingerprints; their original origin remains stored separately.
fn insert_text(
    snapshot: &mut Inventory,
    texts: &mut BTreeMap<SourcePath, String>,
    path: SourcePath,
    text: String,
) {
    snapshot.files.insert(
        path.clone(),
        FileEntry {
            path: path.clone(),
            bytes: text.len() as u64,
            sha256: files::hex(&Sha256::digest(text.as_bytes())),
            role: super::super::FileRole::Preserved,
        },
    );
    texts.insert(path, text);
}

/// Reassemble renamed owner families from original descriptors and independently pinned form bodies.
fn owner_source(
    base: &AcceptedBase,
    candidate: &Package,
    documents: &BTreeMap<SourcePath, NativeDocument>,
    original_files: &BTreeMap<String, SourcePath>,
    paths: &[SourcePath],
) -> Result<ObjectSource, Error> {
    let mut parts = paths.iter().map(|path| {
        documents
            .get(path)
            .cloned()
            .ok_or_else(|| Error::root("source_descriptor_missing"))
    });
    let document = parts
        .next()
        .ok_or_else(|| Error::root("source_descriptor_missing"))??;
    let identity = format!("owner:{}:source", document.descriptor.object.uuid.key());
    let form = if matches!(
        document.descriptor.object.kind,
        crate::project_core::MetadataKind::Form | crate::project_core::MetadataKind::CommonForm
    ) {
        let native = &document.path;
        let package = if original_files.contains_key(&identity) {
            &base.package
        } else {
            candidate
        };
        let native = original_files
            .get(&identity)
            .and_then(|source| base.package.owners.get(source))
            .and_then(|paths| paths.first())
            .unwrap_or(native);
        package
            .forms
            .get(&super::super::layout::forms::native_path(native)?)
            .cloned()
    } else {
        None
    };
    Ok(ObjectSource {
        document,
        nested: parts.collect::<Result<_, _>>()?,
        form,
    })
}

/// Query resources keep old native gaps and actual values while source links follow owner slots.
fn rebind_resources(
    base: &AcceptedBase,
    candidate: &Package,
    mappings: &BTreeMap<&SourcePath, &SourcePath>,
    original_paths: &BTreeMap<&SourcePath, &SourcePath>,
    snapshot: &mut Inventory,
    texts: &mut BTreeMap<SourcePath, String>,
) -> Result<(), Error> {
    for resource in &candidate.manifest.query_resources {
        if let Some(file) = candidate
            .manifest
            .files
            .iter()
            .find(|file| file.path == resource.path && file.is_payload())
        {
            let source = file.source_path();
            let Some(original) = original_paths.get(source) else {
                continue;
            };
            let text = queries::rebind(base, &base.read_file(original)?, mappings)?;
            insert_text(snapshot, texts, source.clone(), text);
        }
    }
    Ok(())
}

/// A path transition retains original byte authority for every declared stable member slot.
fn select_original_files(
    base: &AcceptedBase,
    before: &BTreeMap<String, SourcePath>,
    after: &BTreeMap<String, SourcePath>,
    snapshot: &mut Inventory,
) -> Result<BTreeMap<SourcePath, FileEntry>, Error> {
    let mut origins = BTreeMap::new();
    for (identity, source) in after {
        let Some(old) = before.get(identity) else {
            continue;
        };
        let entry = base.package.snapshot.files.get(old);
        if let Some(entry) = entry {
            let mut selected = entry.clone();
            selected.path = source.clone();
            snapshot.files.insert(source.clone(), selected);
            origins.insert(source.clone(), entry.clone());
        }
        if (identity.starts_with("path:") || identity.starts_with("authority:")) && old != source {
            return Err(Error::root("source_identity_mismatch"));
        }
    }
    Ok(origins)
}
