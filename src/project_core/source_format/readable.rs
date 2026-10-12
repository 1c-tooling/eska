//! A versioned wire envelope, not a second semantic model or a new editable copy of native data.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;

use super::{
    Child, Descriptor, Error, ExactUnsigned, MetadataNode, Namespace, NativeUuid, Property,
    PropertyName, PropertyValue, Section, XmlAttribute, XmlFragment, XmlProfile,
    property::ValueKind,
};
use crate::project_core::MetadataKind;

#[cfg(test)]
mod tests;

/// Upgrade only the representation; original typed/preserved classification remains authoritative.
pub(super) fn upgrade(source: &mut super::ObjectSource) {
    for part in std::iter::once(&mut source.document).chain(&mut source.nested) {
        part.descriptor.schema_version = super::READABLE_SCHEMA_VERSION;
    }
}

/// An explicit schema change may reserialize syntax, but cannot discard unhandled annotations.
pub(super) fn upgrade_text(input: &str) -> Result<String, Error> {
    let mut source = super::ObjectSource::from_yaml(input)?;
    if std::iter::once(&source.document)
        .chain(&source.nested)
        .all(|part| part.descriptor.schema_version == super::READABLE_SCHEMA_VERSION)
    {
        return Ok(input.into());
    }
    if yaml_edit::lex(input)
        .iter()
        .any(|(kind, _)| *kind == yaml_edit::SyntaxKind::COMMENT)
    {
        return Err(Error::root("source_layout_trivia_unsupported"));
    }
    upgrade(&mut source);
    let text = source.to_yaml()?;
    Ok(if input.contains("\r\n") {
        text.replace('\n', "\r\n")
    } else {
        text
    })
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    schema_version: u32,
    rules_version: u32,
    profile: String,
    object: Node,
    native_xml: XmlProfile,
}

/// Properties have one editable value; native slots retain only their names and opaque fragments.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Node {
    kind: MetadataKind,
    uuid: NativeUuid,
    properties: BTreeMap<PropertyName, Value>,
    children: Vec<NodeChild>,
    native: NativeNode,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeNode {
    namespaces: Vec<Namespace>,
    attributes: Vec<XmlAttribute>,
    sections: Vec<NativeSection>,
}

#[derive(Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "items",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum NativeSection {
    Properties(Vec<PropertySlot>),
    Children,
    Preserved(XmlFragment),
}

#[derive(Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
enum PropertySlot {
    Name(PropertyName),
    Preserved { fragment: XmlFragment },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum NodeChild {
    Inline {
        object: Box<Node>,
    },
    Reference {
        metadata_kind: MetadataKind,
        name: String,
    },
    Preserved {
        fragment: XmlFragment,
    },
}

/// Serialize only the selected envelope; exact native values are never regenerated from XML.
pub(super) fn serialize<S: Serializer>(
    descriptor: &Descriptor,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let object = encode_node(&descriptor.object).map_err(serde::ser::Error::custom)?;
    Envelope {
        schema_version: descriptor.schema_version,
        rules_version: descriptor.rules_version,
        profile: descriptor.profile.clone(),
        object,
        native_xml: descriptor.xml.clone(),
    }
    .serialize(serializer)
}

/// Reject unbound, duplicated or missing values before rebuilding the single lossless model.
pub(super) fn deserialize(value: Value) -> Result<Descriptor, Error> {
    let wire: Envelope =
        serde_json::from_value(value).map_err(|_| Error::root("source_schema_invalid"))?;
    Ok(Descriptor {
        schema_version: wire.schema_version,
        rules_version: wire.rules_version,
        profile: wire.profile,
        xml: wire.native_xml,
        object: decode_node(wire.object)?,
    })
}

/// Native order and unsupported syntax remain separate from the named everyday property mapping.
fn encode_node(node: &MetadataNode) -> Result<Node, Error> {
    let mut properties = BTreeMap::new();
    let mut children = Vec::new();
    let mut sections = Vec::new();
    for section in &node.sections {
        sections.push(match section {
            Section::Properties(items) => NativeSection::Properties(
                items
                    .iter()
                    .map(|item| match item {
                        Property::Typed { name, value } => {
                            properties.insert(*name, value_to_wire(value)?);
                            Ok(PropertySlot::Name(*name))
                        }
                        Property::Preserved { fragment } => Ok(PropertySlot::Preserved {
                            fragment: fragment.clone(),
                        }),
                    })
                    .collect::<Result<_, Error>>()?,
            ),
            Section::Children(items) => {
                children = items
                    .iter()
                    .map(|item| match item {
                        Child::Inline { object } => Ok(NodeChild::Inline {
                            object: Box::new(encode_node(object)?),
                        }),
                        Child::Reference {
                            metadata_kind,
                            name,
                        } => Ok(NodeChild::Reference {
                            metadata_kind: *metadata_kind,
                            name: name.clone(),
                        }),
                        Child::Preserved { fragment } => Ok(NodeChild::Preserved {
                            fragment: fragment.clone(),
                        }),
                    })
                    .collect::<Result<_, Error>>()?;
                NativeSection::Children
            }
            Section::Preserved(fragment) => NativeSection::Preserved(fragment.clone()),
        });
    }
    Ok(Node {
        kind: node.kind,
        uuid: node.uuid.clone(),
        properties,
        children,
        native: NativeNode {
            namespaces: node.namespaces.clone(),
            attributes: node.attributes.clone(),
            sections,
        },
    })
}

/// Consuming each binding exactly once detects both undeclared properties and repeated slot names.
fn decode_node(node: Node) -> Result<MetadataNode, Error> {
    let mut properties = node.properties;
    let mut children = Some(node.children);
    let mut sections = Vec::new();
    for section in node.native.sections {
        sections.push(match section {
            NativeSection::Properties(items) => Section::Properties(
                items
                    .into_iter()
                    .map(|item| match item {
                        PropertySlot::Name(name) => {
                            let value = properties
                                .remove(&name)
                                .ok_or_else(|| Error::root("source_property_binding_invalid"))?;
                            Ok(Property::Typed {
                                name,
                                value: value_from_wire(name, node.kind, value)?,
                            })
                        }
                        PropertySlot::Preserved { fragment } => {
                            Ok(Property::Preserved { fragment })
                        }
                    })
                    .collect::<Result<_, Error>>()?,
            ),
            NativeSection::Children => Section::Children(
                children
                    .take()
                    .ok_or_else(|| Error::root("source_section_duplicate"))?
                    .into_iter()
                    .map(|child| match child {
                        NodeChild::Inline { object } => Ok(Child::Inline {
                            object: Box::new(decode_node(*object)?),
                        }),
                        NodeChild::Reference {
                            metadata_kind,
                            name,
                        } => Ok(Child::Reference {
                            metadata_kind,
                            name,
                        }),
                        NodeChild::Preserved { fragment } => Ok(Child::Preserved { fragment }),
                    })
                    .collect::<Result<_, Error>>()?,
            ),
            NativeSection::Preserved(fragment) => Section::Preserved(fragment),
        });
    }
    if !properties.is_empty() || children.is_some_and(|items| !items.is_empty()) {
        return Err(Error::root("source_property_binding_invalid"));
    }
    Ok(MetadataNode {
        kind: node.kind,
        uuid: node.uuid,
        namespaces: node.native.namespaces,
        attributes: node.native.attributes,
        sections,
    })
}

/// Exact integers stay quoted digits; empty/nil use explicit wrappers distinct from strings/null.
pub(super) fn value_to_wire(value: &PropertyValue) -> Result<Value, Error> {
    Ok(match value {
        PropertyValue::Text(value) | PropertyValue::Enumeration(value) => {
            Value::String(value.clone())
        }
        PropertyValue::Integer(value) => Value::String(value.as_str().into()),
        PropertyValue::Boolean(value) => Value::Bool(*value),
        PropertyValue::Localized(items) => {
            serde_json::to_value(items).map_err(|_| Error::root("source_schema_invalid"))?
        }
        PropertyValue::Empty => serde_json::json!({"state":"empty"}),
        PropertyValue::Nil => serde_json::json!({"state":"nil"}),
    })
}

/// A native owner-specific domain decides the meaning; scalar spelling cannot invent its type.
pub(super) fn value_from_wire(
    name: PropertyName,
    owner: MetadataKind,
    value: Value,
) -> Result<PropertyValue, Error> {
    let kind = name
        .value_kind(owner)
        .ok_or_else(|| Error::root("source_property_type_invalid"))?;
    if let Value::Object(mapping) = &value
        && mapping.len() == 1
        && let Some(state) = mapping.get("state").and_then(Value::as_str)
    {
        return match state {
            "empty" => Ok(PropertyValue::Empty),
            "nil" => Ok(PropertyValue::Nil),
            _ => Err(Error::root("source_property_type_invalid")),
        };
    }
    let invalid = || Error::root("source_property_type_invalid").in_field(name.as_str());
    match (kind, value) {
        (ValueKind::Text, Value::String(value)) => Ok(PropertyValue::Text(value)),
        (ValueKind::Boolean, Value::Bool(value)) => Ok(PropertyValue::Boolean(value)),
        (ValueKind::Integer, Value::String(value)) => {
            Ok(PropertyValue::Integer(ExactUnsigned::parse(&value)?))
        }
        (ValueKind::Enumeration(_), Value::String(value)) => Ok(PropertyValue::Enumeration(value)),
        (ValueKind::Localized, Value::Array(value)) => serde_json::from_value(Value::Array(value))
            .map(PropertyValue::Localized)
            .map_err(|_| invalid()),
        _ => Err(invalid()),
    }
}
