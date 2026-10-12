//! Inline creation has a closed native profile; accepted owners grant authority to fresh identities.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::{
    AcceptedBase, Child, Error, MetadataNode, NativeUuid, Operation, Property, PropertyName,
    PropertyValue, Section, SourcePath,
    child_order::{self, Identity},
};
use crate::project_core::MetadataKind;

#[cfg(test)]
mod tests;

/// An enum value has no generated type or separate payload; other classes need their own inverse rules.
pub(super) const fn supported_owner(kind: MetadataKind) -> bool {
    matches!(kind, MetadataKind::Enum)
}

/// Add fresh members without deleting, renaming, substituting or reordering accepted siblings.
pub(super) fn select(
    base: &AcceptedBase,
    native: &SourcePath,
    owner: &NativeUuid,
    children: &mut Vec<Child>,
    working: Option<&Vec<Value>>,
    schema: u32,
) -> Result<(), Error> {
    if !base.package.documents.get(native).is_some_and(|document| {
        supported_owner(document.descriptor.object.kind)
            && document.descriptor.object.uuid.key() == owner.key()
    }) {
        return Ok(());
    }
    let (Some(before), Some(after)) = (
        child_order::from_children(children),
        working.and_then(|values| child_order::from_values(values)),
    ) else {
        return Ok(());
    };
    if !is_extension(&before, &after)
        || denied_permission(base, native, owner, &before, &after)?.is_some()
    {
        return Ok(());
    }
    let values = working.ok_or_else(|| Error::root("source_schema_invalid"))?;
    let accepted: BTreeMap<_, _> = before.iter().zip(children.iter()).collect();
    let mut additions = BTreeMap::new();
    let mut seen_other_class = false;
    for (identity, value) in after.iter().zip(values) {
        if let Some(child) = accepted.get(identity) {
            if !matches!(child, Child::Inline { object } if object.kind == MetadataKind::EnumValue)
            {
                seen_other_class = true;
            }
            continue;
        }
        let Identity::Inline(uuid) = identity else {
            return Ok(());
        };
        if seen_other_class || base.contains_object(&NativeUuid::parse(uuid)?) {
            return Ok(());
        }
        let value = value
            .get("object")
            .cloned()
            .ok_or_else(|| Error::root("source_schema_invalid"))?;
        let object: MetadataNode = if schema == super::READABLE_SCHEMA_VERSION {
            super::readable::node_from_value(value)?
        } else {
            serde_json::from_value(value).map_err(|_| Error::root("source_schema_invalid"))?
        };
        validate_new_node(&object)?;
        additions.insert(
            identity.clone(),
            Child::Inline {
                object: Box::new(object),
            },
        );
    }
    validate_names(children.iter().chain(additions.values()))?;
    let mut selected: BTreeMap<_, _> = before.into_iter().zip(std::mem::take(children)).collect();
    selected.extend(additions);
    for identity in after {
        children.push(
            selected
                .remove(&identity)
                .ok_or_else(|| Error::root("source_identity_ambiguous"))?,
        );
    }
    Ok(())
}

/// A collection extension retains every old member exactly once and in its previous relative order.
pub(super) fn is_extension(before: &[Identity], after: &[Identity]) -> bool {
    let known: BTreeSet<_> = before.iter().collect();
    let unique: BTreeSet<_> = after.iter().collect();
    before.len() < after.len()
        && known.len() == before.len()
        && unique.len() == after.len()
        && after
            .iter()
            .filter(|identity| known.contains(identity))
            .eq(before.iter())
}

/// Insertion cannot displace a frozen or supplier-locked existing member even when its order is retained.
pub(super) fn denied_permission(
    base: &AcceptedBase,
    native: &SourcePath,
    owner: &NativeUuid,
    before: &[Identity],
    after: &[Identity],
) -> Result<Option<NativeUuid>, Error> {
    if !base.permission(owner, Operation::Create).allowed {
        return Ok(Some(owner.clone()));
    }
    let positions: BTreeMap<_, _> = after
        .iter()
        .enumerate()
        .map(|(index, identity)| (identity, index))
        .collect();
    for (index, identity) in before.iter().enumerate() {
        if positions.get(identity) != Some(&index) {
            let uuid = child_order::member_uuid(base, native, identity)?;
            if !base.permission(&uuid, Operation::Reorder).allowed {
                return Ok(Some(uuid));
            }
        }
    }
    Ok(None)
}

/// Only the observed three-property enum-value shape can introduce native bytes without accepted slots.
pub(super) fn validate_new_node(node: &MetadataNode) -> Result<(), Error> {
    let [Section::Properties(properties)] = node.sections.as_slice() else {
        return Err(Error::root("source_child_creation_profile_unsupported"));
    };
    let [
        Property::Typed {
            name: PropertyName::Name,
            value: PropertyValue::Text(name),
        },
        Property::Typed {
            name: PropertyName::Synonym,
            value: synonym,
        },
        Property::Typed {
            name: PropertyName::Comment,
            value: comment,
        },
    ] = properties.as_slice()
    else {
        return Err(Error::root("source_child_creation_profile_unsupported"));
    };
    if node.kind != MetadataKind::EnumValue
        || !node.namespaces.is_empty()
        || !node.attributes.is_empty()
        || !matches!(synonym, PropertyValue::Empty | PropertyValue::Localized(_))
        || !matches!(comment, PropertyValue::Empty | PropertyValue::Text(_))
    {
        return Err(Error::root("source_child_creation_profile_unsupported"));
    }
    if !identifier(name) {
        return Err(Error::root("source_metadata_name_invalid").in_field(node.uuid.as_str()));
    }
    Ok(())
}

/// Native identifiers admit Unicode letters, digits after the first character, and underscores.
pub(super) fn identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|ch| ch == '_' || ch.is_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_alphanumeric())
}

/// New names cannot shadow any accepted sibling value under case-insensitive native lookup.
pub(super) fn validate_names<'a>(
    children: impl IntoIterator<Item = &'a Child>,
) -> Result<(), Error> {
    let mut names = BTreeSet::new();
    for child in children {
        if let Child::Inline { object } = child
            && object.kind == MetadataKind::EnumValue
            && let Some(PropertyValue::Text(name)) = object.property(PropertyName::Name)
            && !names.insert(name.to_lowercase())
        {
            return Err(
                Error::root("source_metadata_name_collision").in_field(object.uuid.as_str())
            );
        }
    }
    Ok(())
}
