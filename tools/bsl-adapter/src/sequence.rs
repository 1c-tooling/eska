//! Sequence descriptors retain their own dimensions and qualified document/register links.

use crate::{AdapterError, AdapterIssue, attribute_type, identity, properties::Properties};
use bsl_metadata::{MdoType, ObjectBelonging, Sequence, SequenceDimension};
use eska::project_core::{
    MetadataKind,
    metadata_parser::{ParsedDescriptor, ParsedObject},
};

#[cfg(test)]
mod tests;

/// Convert current sequence metadata without fabricating a register or reading a native sidecar.
///
/// # Errors
/// Rejects incomplete identity, wrong relation kinds, duplicate names/UUIDs and invalid dimension types.
pub fn sequence(descriptor: &ParsedDescriptor) -> Result<Sequence, AdapterError> {
    let root = identity::descriptor_root(descriptor)?;
    if root.metadata.kind() != MetadataKind::Sequence {
        return Err(AdapterError {
            issue: AdapterIssue::WrongKind {
                actual: root.metadata.kind(),
            },
            range: root.range.clone(),
        });
    }
    let fields = Properties::new(root)?;
    let (belonging, extends) = ownership(root, &fields)?;
    let mut dimensions: Vec<SequenceDimension> = Vec::new();
    for child in descriptor
        .objects
        .iter()
        .filter(|child| child.metadata.parent() == Some(root.metadata.id()))
    {
        if child.metadata.kind() != MetadataKind::Dimension {
            return Err(AdapterError {
                issue: AdapterIssue::IncompleteDescriptor,
                range: child.range.clone(),
            });
        }
        let dimension = dimension(child)?;
        if dimensions
            .iter()
            .any(|other| other.uuid == dimension.uuid || same_name(&other.name, &dimension.name))
        {
            return Err(AdapterError {
                issue: AdapterIssue::IncompleteDescriptor,
                range: child.range.clone(),
            });
        }
        dimensions.push(dimension);
    }
    let documents = references(root, &fields, "Documents", belonging)?
        .map(|values| {
            values
                .into_iter()
                .map(|value| match value.split_once('.') {
                    Some(("Document", name)) if !name.is_empty() && !name.contains('.') => {
                        Ok(name.to_owned())
                    }
                    _ => Err(unsupported(root, "Documents", value)),
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    let register_records = references(root, &fields, "RegisterRecords", belonging)?
        .map(|values| {
            values
                .into_iter()
                .map(|value| {
                    let target = value.split_once('.').and_then(|(kind, name)| {
                        let kind = kind.parse::<MdoType>().ok()?;
                        (kind.is_register() && !name.is_empty() && !name.contains('.'))
                            .then(|| (kind, name.to_owned()))
                    });
                    target.ok_or_else(|| unsupported(root, "RegisterRecords", value))
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    Ok(Sequence::from_parts(
        identity::uuid(root)?,
        root.metadata.name().into(),
        dimensions,
        documents,
        register_records,
        belonging,
        extends,
    ))
}

/// Own reference lists are explicit; borrowed absence must survive until UUID-proven composition.
fn references(
    object: &ParsedObject,
    fields: &Properties<'_>,
    name: &str,
    belonging: ObjectBelonging,
) -> Result<Option<Vec<String>>, AdapterError> {
    let Some(field) = fields.field(name)? else {
        if belonging == ObjectBelonging::Adopted {
            return Ok(None);
        }
        return Err(AdapterError {
            issue: AdapterIssue::MissingProperty { name: name.into() },
            range: object.range.clone(),
        });
    };
    let mut values: Vec<String> = Vec::new();
    for value in fields.references(name, "Item")? {
        if values.iter().any(|other| same_name(other, value)) {
            return Err(Properties::invalid(field));
        }
        values.push(value.to_owned());
    }
    Ok(Some(values))
}

/// Qualified reference failures retain the owner's original source range rather than a generated BSL range.
fn unsupported(object: &ParsedObject, name: &str, value: String) -> AdapterError {
    AdapterError {
        issue: AdapterIssue::UnsupportedPropertyValue {
            name: name.into(),
            value,
        },
        range: object.range.clone(),
    }
}

/// Both owners and dimensions require consistent own/borrowed identity bookkeeping.
fn ownership(
    object: &ParsedObject,
    fields: &Properties<'_>,
) -> Result<(ObjectBelonging, Option<bsl_metadata::Uuid>), AdapterError> {
    let (belonging, extends) = identity::ownership(fields)?;
    if (belonging == ObjectBelonging::Adopted) != extends.is_some() {
        return Err(AdapterError {
            issue: AdapterIssue::InvalidProperty {
                name: "ExtendedConfigurationObject".into(),
            },
            range: object.range.clone(),
        });
    }
    Ok((belonging, extends))
}

/// A borrowed dimension may inherit Type; a new dimension requires an explicit native `TypeDescription`.
fn dimension(object: &ParsedObject) -> Result<SequenceDimension, AdapterError> {
    let fields = Properties::new(object)?;
    let (belonging, extends) = ownership(object, &fields)?;
    let attribute_type = fields.field("Type")?.map(attribute_type).transpose()?;
    if attribute_type.is_none() && belonging != ObjectBelonging::Adopted {
        return Err(AdapterError {
            issue: AdapterIssue::MissingProperty {
                name: "Type".into(),
            },
            range: object.range.clone(),
        });
    }
    if ["Period", "Период", "Recorder", "Регистратор"]
        .iter()
        .any(|name| same_name(name, object.metadata.name()))
    {
        return Err(AdapterError {
            issue: AdapterIssue::InvalidProperty {
                name: "Name".into(),
            },
            range: object.range.clone(),
        });
    }
    Ok(SequenceDimension {
        uuid: identity::uuid(object)?,
        name: object.metadata.name().into(),
        attribute_type,
        belonging,
        extends,
    })
}

/// Match the existing adapter's Unicode case-insensitive metadata-name comparison without another dependency.
fn same_name(left: &str, right: &str) -> bool {
    left.chars()
        .flat_map(char::to_lowercase)
        .eq(right.chars().flat_map(char::to_lowercase))
}
