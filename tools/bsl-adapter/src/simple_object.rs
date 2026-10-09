//! Constants and enums use the same root identity, ownership and type boundary as other families.

#[cfg(test)]
mod tests;

use bsl_metadata::{MdoType, MetadataObject, metadata_object::EnumValue};
use eska::project_core::{
    MetadataKind,
    metadata_parser::{ParsedDescriptor, ParsedObject},
};

use crate::{AdapterError, AdapterIssue, attribute_type, identity, properties::Properties};

/// Convert a constant descriptor, retaining absent Type separately from an explicitly empty Type.
///
/// # Errors
/// Returns incomplete/wrong descriptors, unloaded properties, invalid identities or unsupported types.
pub fn constant_object(descriptor: &ParsedDescriptor) -> Result<MetadataObject, AdapterError> {
    let root = identity::descriptor_root(descriptor)?;
    let mut model = object(root, MetadataKind::Constant, MdoType::Constant)?;
    if let Some(field) = Properties::new(root)?.field("Type")? {
        model.set_constant_type(attribute_type(field)?);
    }
    Ok(model)
}

/// Convert enum values in declaration order and validate every real value UUID before inclusion.
///
/// # Errors
/// Returns incomplete/wrong descriptors, unloaded properties or malformed root/value identities.
pub fn enum_object(descriptor: &ParsedDescriptor) -> Result<MetadataObject, AdapterError> {
    let root = identity::descriptor_root(descriptor)?;
    let mut model = object(root, MetadataKind::Enum, MdoType::Enum)?;
    for child in descriptor.objects.iter().filter(|child| {
        child.metadata.parent() == Some(root.metadata.id())
            && child.metadata.kind() == MetadataKind::EnumValue
    }) {
        Properties::new(child)?;
        identity::uuid(child)?;
        model.enum_values.push(EnumValue {
            name: child.metadata.name().into(),
            name_en: None,
            uuid: child.metadata.uuid().into(),
        });
    }
    Ok(model)
}

/// Reconstruct a root from actual source identity and preserve the selected kernel's ownership state.
fn object(
    root: &ParsedObject,
    expected: MetadataKind,
    kind: MdoType,
) -> Result<MetadataObject, AdapterError> {
    if root.metadata.kind() != expected {
        return Err(AdapterError {
            issue: AdapterIssue::WrongKind {
                actual: root.metadata.kind(),
            },
            range: root.range.clone(),
        });
    }
    let fields = Properties::new(root)?;
    let (belonging, extends) = identity::ownership(&fields)?;
    let mut model = MetadataObject::new(kind, root.metadata.name());
    model.set_uuid(identity::uuid(root)?);
    model.set_object_belonging(belonging);
    if let Some(uuid) = extends {
        model.set_extends_uuid(uuid);
    }
    Ok(model)
}
