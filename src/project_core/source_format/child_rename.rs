//! Renaming a closed inline value preserves identity and accepted namespace ownership.

use serde_json::Value;

use super::{
    AcceptedBase, Child, Error, MetadataNode, NativeUuid, Operation, Property, PropertyName,
    PropertyValue, Section, SourcePath, child_creation,
};
use crate::project_core::MetadataKind;

#[cfg(test)]
mod tests;

/// Only inline enum values can change names without generated types, paths or separate files.
pub(super) const fn supported_object(kind: MetadataKind) -> bool {
    matches!(kind, MetadataKind::EnumValue)
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
    let Some(accepted) = accepted_owner(base, native) else {
        return Ok(false);
    };
    if accepted.uuid.key() != owner.key() || !base.permission(owner, Operation::Reorder).allowed {
        return Ok(false);
    }
    let mut changed = false;
    for child in children {
        let Child::Inline { object } = child else {
            continue;
        };
        let Some(before) = values(accepted).find(|node| node.uuid.key() == object.uuid.key())
        else {
            continue;
        };
        if child_creation::validate_new_node(before).is_err()
            || !base.permission(&object.uuid, Operation::Rename).allowed
        {
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
            node.get("kind").and_then(Value::as_str) == Some(MetadataKind::EnumValue.designer_tag())
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

/// Necessary identity/class authority comes from the pinned native owner, never working node fields.
fn accepted_owner<'a>(base: &'a AcceptedBase, native: &SourcePath) -> Option<&'a MetadataNode> {
    let owner = &base.package.documents.get(native)?.descriptor.object;
    (owner.kind == MetadataKind::Enum).then_some(owner)
}

/// A value namespace contains only the owner's direct inline enum values.
pub(super) fn values(owner: &MetadataNode) -> impl Iterator<Item = &MetadataNode> {
    owner
        .sections
        .iter()
        .filter_map(|section| match section {
            Section::Children(children) => Some(children),
            _ => None,
        })
        .flatten()
        .filter_map(|child| match child {
            Child::Inline { object } if supported_object(object.kind) => Some(object.as_ref()),
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

/// Residual Name changes report the parent when its namespace is locked, otherwise the accepted value.
pub(super) fn diagnostic_uuid(
    base: &AcceptedBase,
    native: Option<&str>,
    uuid: &NativeUuid,
) -> Option<NativeUuid> {
    let path = SourcePath::parse(native?).ok()?;
    let owner = accepted_owner(base, &path)?;
    values(owner).find(|node| node.uuid.key() == uuid.key())?;
    Some(
        if base.permission(&owner.uuid, Operation::Reorder).allowed {
            uuid.clone()
        } else {
            owner.uuid.clone()
        },
    )
}
