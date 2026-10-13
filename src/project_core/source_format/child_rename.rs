//! Renaming verified inline members preserves identity and accepted namespace ownership.

use serde_json::Value;

use super::{
    AcceptedBase, Child, Error, MetadataNode, NativeUuid, Operation, Property, PropertyName,
    PropertyValue, Section, SourcePath, attribute_profile, child_creation,
};
use crate::project_core::MetadataKind;

#[cfg(test)]
mod attribute_tests;
#[cfg(test)]
mod tests;

/// Inline enum values and verified attributes own no generated types, paths or separate files.
pub(super) const fn supported_object(kind: MetadataKind) -> bool {
    matches!(kind, MetadataKind::EnumValue | MetadataKind::Attribute)
}

/// Apply names by accepted UUID before membership selection; final sibling names are checked afterwards.
pub(super) fn select(
    base: &AcceptedBase,
    native: &SourcePath,
    owner: &NativeUuid,
    children: &mut [Child],
    working: Option<&Vec<Value>>,
    schema: u32,
) -> Result<bool, Error> {
    let Some(document) = base.package.documents.get(native) else {
        return Ok(false);
    };
    let Some(accepted) = child_creation::accepted_owner(&document.descriptor, owner) else {
        return Ok(false);
    };
    if !base.permission(owner, Operation::Reorder).allowed {
        return Ok(false);
    }
    let context = child_creation::accepted_context(&document.descriptor, accepted)?;
    let mut changed = false;
    for child in children {
        let Child::Inline { object } = child else {
            continue;
        };
        let Some(before) =
            inline_children(accepted).find(|node| node.uuid.key() == object.uuid.key())
        else {
            continue;
        };
        if !base.permission(&object.uuid, Operation::Rename).allowed {
            continue;
        }
        let supported = match accepted.kind {
            MetadataKind::Enum => {
                before.kind == MetadataKind::EnumValue
                    && child_creation::validate_new_node(before).is_ok()
            }
            MetadataKind::Catalog | MetadataKind::Document | MetadataKind::TabularSection => {
                before.kind == MetadataKind::Attribute
                    && attribute_profile::attribute(before, &context)?
            }
            _ => false,
        };
        if !supported {
            continue;
        }
        let current = working
            .into_iter()
            .flatten()
            .filter_map(|child| child.get("object"))
            .find(|node| {
                node.get("uuid")
                    .and_then(Value::as_str)
                    .is_some_and(|id| id.eq_ignore_ascii_case(object.uuid.as_str()))
            });
        let Some(current) = current.filter(|node| {
            node.get("kind").and_then(Value::as_str) == Some(before.kind.designer_tag())
        }) else {
            continue;
        };
        let Some(name) = candidate_name(current, schema)
            .map_err(|error| error.in_field(object.uuid.as_str()))?
        else {
            continue;
        };
        if !child_creation::identifier(&name) {
            return Err(Error::root("source_metadata_name_invalid").in_field(object.uuid.as_str()));
        }
        for section in &mut object.sections {
            if let Section::Properties(properties) = section {
                for property in properties {
                    if let Property::Typed {
                        name: PropertyName::Name,
                        value,
                    } = property
                    {
                        let next = PropertyValue::Text(name.clone());
                        changed |= *value != next;
                        *value = next;
                    }
                }
            }
        }
    }
    Ok(changed)
}

/// A value namespace contains only the owner's direct inline enum values.
pub(super) fn values(owner: &MetadataNode) -> impl Iterator<Item = &MetadataNode> {
    inline_children(owner).filter(|node| node.kind == MetadataKind::EnumValue)
}

/// Parentage belongs to declared inline members; opaque slots and external references are not rename candidates.
fn inline_children(owner: &MetadataNode) -> impl Iterator<Item = &MetadataNode> {
    owner
        .sections
        .iter()
        .filter_map(|section| match section {
            Section::Children(children) => Some(children),
            _ => None,
        })
        .flatten()
        .filter_map(|child| match child {
            Child::Inline { object } => Some(object.as_ref()),
            _ => None,
        })
}

/// Both closed transports retain exactly one text Name; duplicate legacy sections/fields are ambiguous.
fn candidate_name(current: &Value, schema: u32) -> Result<Option<String>, Error> {
    let candidate = if schema == super::READABLE_SCHEMA_VERSION {
        current
            .get("properties")
            .and_then(|properties| properties.get("Name"))
    } else {
        let mut sections = current
            .get("sections")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|section| section.get("kind").and_then(Value::as_str) == Some("properties"));
        let section = sections.next();
        if sections.next().is_some() {
            return Err(Error::root("source_section_duplicate"));
        }
        let mut properties = section
            .and_then(|section| section.get("items"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|property| {
                property.get("kind").and_then(Value::as_str) == Some("typed")
                    && property.get("name").and_then(Value::as_str) == Some("Name")
            });
        let property = properties.next();
        if properties.next().is_some() {
            return Err(Error::root("source_property_duplicate"));
        }
        property.and_then(|property| property.get("value"))
    };
    let Some(candidate) = candidate else {
        return Ok(None);
    };
    if schema == super::READABLE_SCHEMA_VERSION {
        candidate
            .as_str()
            .map(|value| Some(value.to_owned()))
            .ok_or_else(|| Error::root("source_metadata_name_invalid"))
    } else {
        match serde_json::from_value::<PropertyValue>(candidate.clone()) {
            Ok(PropertyValue::Text(name)) => Ok(Some(name)),
            _ => Err(Error::root("source_metadata_name_invalid")),
        }
    }
}

/// Residual Name changes report the actual parent when its namespace is locked, otherwise the accepted member.
pub(super) fn diagnostic_uuid(
    base: &AcceptedBase,
    native: Option<&str>,
    uuid: &NativeUuid,
) -> Option<NativeUuid> {
    let path = SourcePath::parse(native?).ok()?;
    let descriptor = &base.package.documents.get(&path)?.descriptor;
    let root = &descriptor.object;
    let owner = std::iter::once(root)
        .chain(inline_children(root))
        .find(|parent| {
            child_creation::accepted_owner(descriptor, &parent.uuid).is_some()
                && inline_children(parent)
                    .any(|node| supported_object(node.kind) && node.uuid.key() == uuid.key())
        })?;
    Some(
        if base.permission(&owner.uuid, Operation::Reorder).allowed {
            uuid.clone()
        } else {
            owner.uuid.clone()
        },
    )
}
