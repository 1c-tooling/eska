//! A verified path transition changes bindings while retaining original policy and protected values.

mod queries;
mod references;

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use super::{
    AcceptedBase, Error, FileEntry, Inventory, PropertyName, SourcePath, document_ownership, files,
};
use crate::project_core::source_format::{
    Child, NativeDocument, ObjectSource, Operation, Property, Section, export::Package,
    file_identity, package::manifest_text,
};

/// Original bytes remain verifiable even when the effective package uses renamed authoring paths.
pub(super) struct Bindings {
    pub original: Inventory,
    pub texts: BTreeMap<SourcePath, String>,
    pub origins: BTreeMap<SourcePath, FileEntry>,
}

impl AcceptedBase {
    /// Bind renamed objects by UUID, retaining the independently approved policy and native stencils.
    pub(in crate::project_core::source_format) fn with_composition(
        &self,
        candidate: &Package,
    ) -> Result<Self, Error> {
        validate_identity(self, candidate)?;
        validate_reference_order(self, candidate)?;
        let before = file_identity::files(&self.package)?;
        let after = file_identity::files(candidate)?;
        if !before.keys().eq(after.keys()) {
            return Err(Error::root("source_import_mapping_unsupported"));
        }
        let old_documents = self
            .package
            .documents
            .values()
            .map(|document| (document.descriptor.object.uuid.key(), document))
            .collect::<BTreeMap<_, _>>();
        let mut documents = BTreeMap::new();
        let renames = references::renames(&self.package, candidate)?;
        for document in candidate.documents.values() {
            let old = old_documents
                .get(&document.descriptor.object.uuid.key())
                .ok_or_else(|| Error::root("source_identity_mismatch"))?;
            let mut selected = (*old).clone();
            bind_document(self, &mut selected, document)?;
            references::rebind(self, &mut selected, document, &renames)?;
            documents.insert(selected.path.clone(), selected);
        }
        let source_paths = before
            .iter()
            .map(|(identity, old)| (old, &after[identity]))
            .collect::<BTreeMap<_, _>>();
        let original_paths = source_paths
            .iter()
            .map(|(old, new)| (*new, *old))
            .collect::<BTreeMap<_, _>>();
        let mut texts = BTreeMap::new();
        let mut snapshot = candidate.snapshot.clone();
        let origins = select_original_files(self, &before, &after, &mut snapshot)?;
        for (source, paths) in &candidate.owners {
            let mut owner = owner_source(self, &documents, &before, paths)?;
            if let Some(form) = &owner.form {
                owner.form = Some(form.with_query_references(&queries::rebind(
                    self,
                    &form.to_xml()?,
                    &source_paths,
                )?)?);
            }
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
            texts.get(path).cloned().map_or_else(
                || {
                    self.read_file(
                        original_paths
                            .get(path)
                            .copied()
                            .ok_or_else(|| Error::root("source_file_unmapped"))?,
                    )
                },
                Ok,
            )
        })?;
        Ok(Self {
            root: self.root.clone(),
            reference: self.reference.clone(),
            manifest: self.manifest.clone(),
            support: self.support.clone(),
            composition: Some(Bindings {
                original: self.package.snapshot.clone(),
                texts,
                origins,
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
}

/// A renamed reference must retain the same UUID and position; frozen order cannot be laundered.
fn validate_reference_order(base: &AcceptedBase, candidate: &Package) -> Result<(), Error> {
    let current = candidate
        .documents
        .values()
        .map(|document| (document.descriptor.object.uuid.key(), document))
        .collect::<BTreeMap<_, _>>();
    for old in base.package.documents.values() {
        let new = current
            .get(&old.descriptor.object.uuid.key())
            .ok_or_else(|| Error::root("source_identity_mismatch"))?;
        let left = referenced_uuids(&base.package, old)?;
        let right = referenced_uuids(candidate, new)?;
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

/// Composition may change names and paths, but never root profile, UUID set, class or parentage.
fn validate_identity(base: &AcceptedBase, candidate: &Package) -> Result<(), Error> {
    if base.package.manifest.schema_version != candidate.manifest.schema_version
        || base.package.manifest.profile != candidate.manifest.profile
        || base.package.manifest.external_root != candidate.manifest.external_root
        || document_ownership(&candidate.documents)? != base.support.objects
    {
        return Err(Error::root("source_import_mapping_unsupported"));
    }
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
    let original = original_files
        .get(&identity)
        .ok_or_else(|| Error::root("source_file_unmapped"))?;
    let form = if matches!(
        document.descriptor.object.kind,
        crate::project_core::MetadataKind::Form | crate::project_core::MetadataKind::CommonForm
    ) {
        let native = base
            .package
            .owners
            .get(original)
            .and_then(|paths| paths.first())
            .ok_or_else(|| Error::root("source_descriptor_missing"))?;
        base.package
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
            let original = original_paths
                .get(source)
                .ok_or_else(|| Error::root("source_file_unmapped"))?;
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
        let old = &before[identity];
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
