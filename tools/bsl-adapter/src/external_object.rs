//! External artifacts retain their explicit project profile after core normalizes XML tag aliases.

#[cfg(test)]
mod tests;

use bsl_metadata::{ExternalObjectKind, MetadataObject};
use eska::project_core::{MetadataKind, metadata_parser::ParsedDescriptor};

use crate::{AdapterError, AdapterIssue, identity, metadata_object};

/// Project one EPF/ERF owner under its independently known source profile, without a Configuration.
///
/// Core normalizes external and internal descriptor tags to the same logical kind; the artifact's
/// validated project profile therefore supplies the external discriminator rather than a qualified name,
/// directory name or guessed owner. Field and tabular-section conversion retains the shared rules.
///
/// # Errors
/// Returns mismatched logical families, incomplete properties, invalid source UUIDs or field types.
pub fn external_object(
    descriptor: &ParsedDescriptor,
    kind: ExternalObjectKind,
) -> Result<MetadataObject, AdapterError> {
    let root = identity::descriptor_root(descriptor)?;
    let expected = match kind {
        ExternalObjectKind::DataProcessor => MetadataKind::DataProcessor,
        ExternalObjectKind::Report => MetadataKind::Report,
    };
    if root.metadata.kind() != expected {
        return Err(AdapterError {
            issue: AdapterIssue::WrongKind {
                actual: root.metadata.kind(),
            },
            range: root.range.clone(),
        });
    }
    let mut object = metadata_object(descriptor, Vec::new())?;
    object.mdo_type = kind.mdo_type();
    Ok(object)
}
