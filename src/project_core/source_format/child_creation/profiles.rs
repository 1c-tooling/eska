//! New inline profiles retain explicit native fields and reject unverified inverse rules.

use super::super::{
    Child, Error, MetadataNode, Namespace, Property, PropertyName, PropertyValue, Section,
    TypeAlternativeKind, type_description,
};
use super::{identifier, validate_names};
use crate::project_core::MetadataKind;

pub(super) mod defaults;

/// A closed inline profile avoids guessing generated identities, defaults or opaque inverse rules.
pub(in crate::project_core::source_format) fn validate_new_node(
    node: &MetadataNode,
) -> Result<(), Error> {
    let (properties, children) = match node.sections.as_slice() {
        [Section::Properties(properties)] if node.kind != MetadataKind::TabularSection => {
            (properties, [].as_slice())
        }
        [
            Section::TabularSectionIdentities(_),
            Section::Properties(properties),
            Section::Children(children),
        ] if node.kind == MetadataKind::TabularSection => (properties, children.as_slice()),
        _ => return Err(Error::root("source_child_creation_profile_unsupported")),
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
        MetadataKind::EnumValue | MetadataKind::TabularSection => rest.is_empty(),
        MetadataKind::Attribute => defaults::attribute(rest)?,
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
    for child in children {
        let Child::Inline { object } = child else {
            return Err(Error::root("source_child_creation_profile_unsupported"));
        };
        if object.kind != MetadataKind::Attribute {
            return Err(Error::root("source_child_creation_profile_unsupported"));
        }
        validate_new_node(object)?;
    }
    validate_names(children)?;
    Ok(())
}

/// Fresh fields admit observed primitive types and declared configuration types, never arbitrary namespace values.
pub(in crate::project_core::source_format) fn validate_new_types(
    node: &MetadataNode,
    context: &[Namespace],
) -> Result<(), Error> {
    if let Some(PropertyValue::TypeDescription(value)) = node.property(PropertyName::Type) {
        value.validate(context)?;
        for alternative in &value.data.alternatives {
            let (namespace, local) = type_description::qualified_name(&alternative.name, context)?;
            let primitive = namespace == "http://www.w3.org/2001/XMLSchema"
                && alternative.kind == TypeAlternativeKind::Type
                && matches!(local, "boolean" | "string" | "decimal" | "dateTime");
            if !primitive && type_description::named_target(alternative, context)?.is_none() {
                return Err(Error::root("source_child_creation_profile_unsupported"));
            }
        }
        if value.data.alternatives.is_empty() {
            return Err(Error::root("source_child_creation_profile_unsupported"));
        }
    }
    for section in &node.sections {
        if let Section::Children(children) = section {
            for child in children {
                if let Child::Inline { object } = child {
                    validate_new_types(object, context)?;
                }
            }
        }
    }
    Ok(())
}
