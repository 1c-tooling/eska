//! Native composition and identity rules shared by import and export.

use std::collections::BTreeSet;

use crate::project_core::MetadataKind;

use super::{Child, Error, MetadataNode, Section, SourcePath};

/// Native child names are a single safe component; directory layout follows the kind registry.
pub(super) fn reference_path(
    base: &str,
    kind: MetadataKind,
    name: &str,
) -> Result<SourcePath, Error> {
    SourcePath::parse(name)?;
    if name.contains('/') {
        return Err(Error::root("source_path_invalid"));
    }
    let folder =
        reference_folder(kind).ok_or_else(|| Error::root("source_reference_kind_unsupported"))?;
    SourcePath::parse(&format!(
        "{}{folder}/{name}.xml",
        if base.is_empty() {
            String::new()
        } else {
            format!("{base}/")
        }
    ))
}

/// Descriptor paths and pinned payload ownership use the same native collection vocabulary.
pub(super) fn reference_folder(kind: MetadataKind) -> Option<&'static str> {
    kind.collection_folder().or(match kind {
        MetadataKind::Form => Some("Forms"),
        MetadataKind::Template => Some("Templates"),
        MetadataKind::Command => Some("Commands"),
        _ => None,
    })
}

/// One YAML replaces a main descriptor while preserving its native relative owner directory.
pub(super) fn source_path(native: &SourcePath) -> Result<SourcePath, Error> {
    let base = native
        .as_str()
        .strip_suffix(".xml")
        .ok_or_else(|| Error::root("source_path_invalid"))?;
    SourcePath::parse(&format!("{base}/object.yaml"))
}

/// Inline references need a verified layout of their own; the initial profile refuses guessing.
pub(super) fn references(node: &MetadataNode) -> Result<Vec<(MetadataKind, String)>, Error> {
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

/// Reader bindings retain the same declared members while an unsaved document changes their order.
pub(super) fn same_reference_members(
    left: &MetadataNode,
    right: &MetadataNode,
) -> Result<bool, Error> {
    let mut left = references(left)?;
    let mut right = references(right)?;
    left.sort_unstable();
    right.sort_unstable();
    Ok(left == right)
}

/// Check modeled object and service identities across the declared composition without granting policy authority.
pub(super) fn identities(node: &MetadataNode, values: &mut BTreeSet<String>) -> Result<(), Error> {
    if !values.insert(node.uuid.key()) {
        return Err(Error::root("native_uuid_duplicate"));
    }
    for section in &node.sections {
        if let Section::TabularSectionIdentities(identities) = section {
            for uuid in identities.uuids() {
                if !values.insert(uuid.key()) {
                    return Err(Error::root("native_uuid_duplicate"));
                }
            }
        }
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
