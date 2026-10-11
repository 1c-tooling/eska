//! A role identity and its independently loaded rights are one explicit native model.

#[cfg(test)]
mod tests;

use bsl_metadata::{Role, RoleData};
use eska::project_core::{MetadataKind, metadata_parser::ParsedDescriptor};

use crate::{AdapterError, AdapterIssue, identity, properties::Properties};

/// Combine a captured role descriptor with current rights supplied by the input owner.
///
/// The caller must distinguish a genuinely absent rights resource from a declared resource
/// that is pending, unreadable or invalid. This function never reads files or supplies defaults.
///
/// # Errors
/// Returns an incomplete descriptor, wrong kind, missing properties or invalid source UUID.
pub fn role(descriptor: &ParsedDescriptor, rights: RoleData) -> Result<Role, AdapterError> {
    let root = identity::descriptor_root(descriptor)?;
    if root.metadata.kind() != MetadataKind::Role {
        return Err(AdapterError {
            issue: AdapterIssue::WrongKind {
                actual: root.metadata.kind(),
            },
            range: root.range.clone(),
        });
    }
    let _fields = Properties::new(root)?;
    Ok(Role::with_data(
        identity::uuid(root)?,
        root.metadata.name().to_owned(),
        rights,
    ))
}
