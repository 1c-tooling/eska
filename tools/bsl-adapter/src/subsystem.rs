//! Subsystem content and direct child names stay ordered in the native graph model.

#[cfg(test)]
mod tests;

use bsl_metadata::{MdoType, Subsystem};
use eska::project_core::{MetadataKind, metadata_parser::ParsedDescriptor};

use crate::{AdapterError, AdapterIssue, identity, properties::Properties};

/// Project one subsystem descriptor without inventing native fields for its nested documents.
///
/// # Errors
/// Returns an incomplete descriptor, wrong kind, invalid identity or malformed property structure.
pub fn subsystem(descriptor: &ParsedDescriptor) -> Result<Subsystem, AdapterError> {
    let root = identity::descriptor_root(descriptor)?;
    if root.metadata.kind() != MetadataKind::Subsystem {
        return Err(AdapterError {
            issue: AdapterIssue::WrongKind {
                actual: root.metadata.kind(),
            },
            range: root.range.clone(),
        });
    }
    // The graph model has no UUID field; source identity still must be valid.
    let _uuid = identity::uuid(root)?;
    let fields = Properties::new(root)?;
    let content = fields
        .references("Content", "Item")?
        .into_iter()
        .filter_map(|raw| {
            let (prefix, name) = raw.trim().split_once('.')?;
            if name.is_empty() {
                return None;
            }
            // The native graph represents only recognised MDO kinds, excluding organisational resources.
            prefix
                .parse::<MdoType>()
                .ok()
                .map(|kind| (kind, name.to_owned()))
        })
        .collect();
    let children = descriptor
        .references
        .iter()
        .filter(|reference| {
            reference.owner == *root.metadata.id() && reference.kind == MetadataKind::Subsystem
        })
        .map(|reference| reference.name.clone())
        .collect();
    Ok(Subsystem::new(root.metadata.name())
        .with_content(content)
        .with_child_subsystems(children))
}
