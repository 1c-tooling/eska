//! Inline creation has a closed native profile; accepted owners grant authority to fresh identities.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::{
    AcceptedBase, Child, Error, MetadataNode, NativeUuid, Operation, PropertyName, PropertyValue,
    SourcePath,
    child_order::{self, Identity},
};
#[cfg(test)]
use super::{Property, Section};
use crate::project_core::MetadataKind;

mod identities;
mod profiles;
mod scope;
pub(super) use profiles::{validate_new_node, validate_new_types};
pub(super) use scope::created_context;

#[cfg(test)]
mod attribute_tests;
#[cfg(test)]
mod column_tests;
#[cfg(test)]
mod table_tests;
#[cfg(test)]
mod tests;

/// The observed inline profiles include explicit table identities and no separate payloads.
pub(super) const fn supported_owner(kind: MetadataKind) -> bool {
    matches!(
        kind,
        MetadataKind::Enum
            | MetadataKind::Catalog
            | MetadataKind::Document
            | MetadataKind::TabularSection
    )
}

/// Creation authority follows the accepted parent class rather than the authored child class.
pub(super) const fn supported_child(owner: MetadataKind, child: MetadataKind) -> bool {
    match owner {
        MetadataKind::Enum => matches!(child, MetadataKind::EnumValue),
        MetadataKind::Catalog | MetadataKind::Document => {
            matches!(
                child,
                MetadataKind::Attribute | MetadataKind::TabularSection
            )
        }
        MetadataKind::TabularSection => matches!(child, MetadataKind::Attribute),
        _ => false,
    }
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
    let Some(document) = base.package.documents.get(native) else {
        return Ok(());
    };
    let Some(accepted_owner) = scope::owner(&document.descriptor, owner) else {
        return Ok(());
    };
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
    let context = scope::context(&document.descriptor, accepted_owner)?;
    let values = working.ok_or_else(|| Error::root("source_schema_invalid"))?;
    let accepted: BTreeMap<_, _> = before.iter().zip(children.iter()).collect();
    let mut additions = BTreeMap::new();
    let services = identities::accepted_services(base)?;
    let mut previous_rank = 0;
    for (identity, value) in after.iter().zip(values) {
        if let Some(child) = accepted.get(identity) {
            let rank = class_rank(accepted_owner.kind, child);
            if rank < previous_rank {
                return Ok(());
            }
            previous_rank = rank;
            continue;
        }
        let Identity::Inline(uuid) = identity else {
            return Ok(());
        };
        if base.contains_object(&NativeUuid::parse(uuid)?) {
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
        if !supported_child(accepted_owner.kind, object.kind) {
            return Err(Error::root("source_child_creation_profile_unsupported"));
        }
        validate_new_node(&object)?;
        validate_new_types(&object, &context)?;
        identities::validate(base, &object, &services)?;
        let child = Child::Inline {
            object: Box::new(object),
        };
        let rank = class_rank(accepted_owner.kind, &child);
        if rank < previous_rank {
            return Ok(());
        }
        previous_rank = rank;
        additions.insert(identity.clone(), child);
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

/// Native class groups keep attributes before tables and both before existing payload-bearing references.
fn class_rank(owner: MetadataKind, child: &Child) -> u8 {
    match child {
        Child::Inline { object } if supported_child(owner, object.kind) => {
            u8::from(object.kind == MetadataKind::TabularSection)
        }
        _ => {
            if matches!(owner, MetadataKind::Catalog | MetadataKind::Document) {
                2
            } else {
                1
            }
        }
    }
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

/// Native identifiers admit Unicode letters, digits after the first character, and underscores.
pub(super) fn identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|ch| ch == '_' || ch.is_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_alphanumeric())
}

/// Attributes and tabular sections share a field namespace; enum values have a separate namespace.
pub(super) fn validate_names<'a>(
    children: impl IntoIterator<Item = &'a Child>,
) -> Result<(), Error> {
    let mut names = BTreeSet::new();
    for child in children {
        if let Child::Inline { object } = child
            && matches!(
                object.kind,
                MetadataKind::EnumValue | MetadataKind::Attribute | MetadataKind::TabularSection
            )
            && let Some(PropertyValue::Text(name)) = object.property(PropertyName::Name)
            && !names.insert((
                if object.kind == MetadataKind::EnumValue {
                    MetadataKind::EnumValue
                } else {
                    MetadataKind::Attribute
                },
                name.to_lowercase(),
            ))
        {
            return Err(
                Error::root("source_metadata_name_collision").in_field(object.uuid.as_str())
            );
        }
    }
    Ok(())
}
