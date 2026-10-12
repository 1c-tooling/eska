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
mod attribute_tests;
#[cfg(test)]
mod tests;

/// Supported inline children introduce neither generated identities nor separate payloads.
pub(super) const fn supported_owner(kind: MetadataKind) -> bool {
    child_kind(kind).is_some()
}

/// Creation authority follows the accepted parent class rather than the authored child class.
pub(super) const fn child_kind(owner: MetadataKind) -> Option<MetadataKind> {
    match owner {
        MetadataKind::Enum => Some(MetadataKind::EnumValue),
        MetadataKind::Catalog | MetadataKind::Document => Some(MetadataKind::Attribute),
        _ => None,
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
    let Some(document) = base
        .package
        .documents
        .get(native)
        .filter(|document| document.descriptor.object.uuid.key() == owner.key())
    else {
        return Ok(());
    };
    let Some(kind) = child_kind(document.descriptor.object.kind) else {
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
    let context = super::validation::scope(
        &document.descriptor.xml.namespaces,
        &document.descriptor.object.namespaces,
    )?;
    let values = working.ok_or_else(|| Error::root("source_schema_invalid"))?;
    let accepted: BTreeMap<_, _> = before.iter().zip(children.iter()).collect();
    let mut additions = BTreeMap::new();
    let mut seen_other_class = false;
    for (identity, value) in after.iter().zip(values) {
        if let Some(child) = accepted.get(identity) {
            if !matches!(child, Child::Inline { object } if object.kind == kind) {
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
        if object.kind != kind {
            return Err(Error::root("source_child_creation_profile_unsupported"));
        }
        validate_new_node(&object)?;
        validate_new_types(&object, &context)?;
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

/// A closed inline profile avoids guessing generated identities, defaults or opaque inverse rules.
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
        rest @ ..,
    ] = properties.as_slice()
    else {
        return Err(Error::root("source_child_creation_profile_unsupported"));
    };
    let supported = match node.kind {
        MetadataKind::EnumValue => rest.is_empty(),
        MetadataKind::Attribute => matches!(
            rest,
            [Property::Typed {
                name: PropertyName::Type,
                value: PropertyValue::TypeDescription(_),
            }]
        ),
        _ => false,
    };
    if !supported
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

/// Fresh fields admit observed primitive types and declared configuration types, never arbitrary namespace values.
pub(super) fn validate_new_types(
    node: &MetadataNode,
    context: &[super::Namespace],
) -> Result<(), Error> {
    if let Some(PropertyValue::TypeDescription(value)) = node.property(PropertyName::Type) {
        value.validate(context)?;
        for alternative in &value.data.alternatives {
            let (namespace, local) =
                super::type_description::qualified_name(&alternative.name, context)?;
            let primitive = namespace == "http://www.w3.org/2001/XMLSchema"
                && alternative.kind == super::TypeAlternativeKind::Type
                && matches!(local, "boolean" | "string" | "decimal" | "dateTime");
            if !primitive && super::type_description::named_target(alternative, context)?.is_none()
            {
                return Err(Error::root("source_child_creation_profile_unsupported"));
            }
        }
        if value.data.alternatives.is_empty() {
            return Err(Error::root("source_child_creation_profile_unsupported"));
        }
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
