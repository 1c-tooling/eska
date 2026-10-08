//! Native composition and identity rules shared by import and export.

use std::collections::BTreeSet;

use crate::project::metadata_model::MetadataKind;

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

/// Check every active native UUID across main and inline objects, independently of file paths.
pub(super) fn identities(node: &MetadataNode, values: &mut BTreeSet<String>) -> Result<(), Error> {
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
