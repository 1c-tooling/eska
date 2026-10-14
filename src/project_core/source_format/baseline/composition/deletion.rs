//! Whole-owner removal cannot erase protected descendants or change surviving identity and ancestry.

use super::super::{AcceptedBase, document_ownership};
use crate::project_core::{
    MetadataKind,
    source_format::{
        Child, Error, NativeDocument, NativeUuid, Operation, Section, composition::reference_path,
        export::Package,
    },
    support::State,
};
use std::collections::{BTreeMap, BTreeSet};

/// Only removed declared subtrees may lose UUIDs; inline changes in surviving owners stay separate.
pub(super) fn validate(
    base: &AcceptedBase,
    current: &Package,
    added: &BTreeSet<String>,
) -> Result<(), Error> {
    let objects = document_ownership(&current.documents)?;
    for (uuid, object) in &objects {
        if !added.contains(uuid) && base.support.objects.get(uuid) != Some(object) {
            return Err(Error::root("source_import_mapping_unsupported"));
        }
    }
    let declared = base
        .package
        .documents
        .values()
        .map(|document| document.descriptor.object.uuid.key())
        .collect::<BTreeSet<_>>();
    for (uuid, object) in &base.support.objects {
        if objects.contains_key(uuid) {
            continue;
        }
        let mut owner = uuid.as_str();
        while !declared.contains(owner) {
            owner = base
                .support
                .objects
                .get(owner)
                .and_then(|object| object.parent.as_deref())
                .ok_or_else(|| Error::root("source_identity_mismatch"))?;
        }
        if objects.contains_key(owner) || object.parent.is_none() {
            return Err(Error::root("source_import_mapping_unsupported"));
        }
        let permission = base.permission(&NativeUuid::parse(uuid)?, Operation::Resource);
        if !permission.allowed || permission.state != State::Unrestricted {
            return Err(Error::root("source_import_structure_denied").in_field(uuid.clone()));
        }
    }
    Ok(())
}

/// Parent removal authority and shifted sibling order are checked before changing native stencils.
pub(super) fn retain_references(
    base: &AcceptedBase,
    surviving: &BTreeSet<String>,
    document: &mut NativeDocument,
) -> Result<(), Error> {
    let owner = &document.descriptor.object;
    let folder = if owner.kind == MetadataKind::Configuration {
        ""
    } else {
        document
            .path
            .as_str()
            .strip_suffix(".xml")
            .ok_or_else(|| Error::root("source_path_invalid"))?
    };
    let mut removed = BTreeMap::<usize, BTreeSet<usize>>::new();
    for (section_index, section) in owner.sections.iter().enumerate() {
        let Section::Children(children) = section else {
            continue;
        };
        let mut shifted = false;
        for (index, child) in children.iter().enumerate() {
            if let Child::Inline { object } = child
                && shifted
                && !base.permission(&object.uuid, Operation::Reorder).allowed
            {
                return Err(
                    Error::root("source_import_structure_denied").in_field(object.uuid.as_str())
                );
            }
            let Child::Reference {
                metadata_kind,
                name,
            } = child
            else {
                continue;
            };
            let path = reference_path(folder, *metadata_kind, name)?;
            let uuid = &base
                .package
                .documents
                .get(&path)
                .ok_or_else(|| Error::root("source_descriptor_missing"))?
                .descriptor
                .object
                .uuid;
            if !surviving.contains(&uuid.key()) {
                if !base.permission(&owner.uuid, Operation::Resource).allowed {
                    return Err(
                        Error::root("source_import_structure_denied").in_field(owner.uuid.as_str())
                    );
                }
                removed.entry(section_index).or_default().insert(index);
                shifted = true;
            } else if shifted && !base.permission(uuid, Operation::Reorder).allowed {
                return Err(Error::root("source_import_structure_denied").in_field(uuid.as_str()));
            }
        }
    }
    for (section_index, section) in document.descriptor.object.sections.iter_mut().enumerate() {
        if let Section::Children(children) = section {
            let Some(removed) = removed.get(&section_index) else {
                continue;
            };
            let mut index = 0;
            children.retain(|_| {
                let keep = !removed.contains(&index);
                index += 1;
                keep
            });
        }
    }
    Ok(())
}
