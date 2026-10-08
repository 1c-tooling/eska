//! Read native descriptors; unknown values keep their exact XML and effective namespace scope.

use roxmltree::{Document, Node};

use crate::project::{ProjectType, designer_xml, metadata_model::MetadataKind, metadata_parser};

use super::{
    Child, Descriptor, Error, LineEnding, LocalizedText, MD_NAMESPACE, MetadataNode, Namespace,
    NativeUuid, PROFILE, Property, PropertyName, PropertyValue, RULES_VERSION, SCHEMA_VERSION,
    Section, XmlAttribute, XmlFragment, XmlProfile,
};

const V8_NAMESPACE: &str = "http://v8.1c.ru/8.1/data/core";
const XSI_NAMESPACE: &str = "http://www.w3.org/2001/XMLSchema-instance";

/// Retain all supported sections and explicitly refuse an envelope the model cannot restore.
pub(super) fn decode(input: &str) -> Result<Descriptor, Error> {
    metadata_parser::check_envelope(input)
        .map_err(|_| Error::root("source_xml_envelope_invalid"))?;
    if input.contains("<!DOCTYPE") || input.contains("<!ENTITY") {
        return Err(Error::root("source_xml_dtd_forbidden"));
    }
    let doc = Document::parse_with_options(
        input,
        roxmltree::ParsingOptions {
            nodes_limit: 1_000_000,
            ..Default::default()
        },
    )
    .map_err(|_| Error::root("source_xml_invalid"))?;
    let root = doc.root_element();
    if !root.has_tag_name((MD_NAMESPACE, "MetaDataObject"))
        || root.attribute("version") != Some("2.20")
    {
        return Err(Error::root("source_profile_unsupported"));
    }
    let objects: Vec<_> = meaningful(root)?.collect();
    if objects.len() != 1
        || !objects[0].is_element()
        || doc
            .root()
            .children()
            .any(|child| child.is_comment() || child.is_pi())
    {
        return Err(Error::root("source_xml_envelope_unsupported"));
    }
    if matches!(designer_xml::project_type_from_root(root), Some(kind) if kind != ProjectType::Configuration)
    {
        return Err(Error::root("source_profile_unsupported"));
    }
    let context = namespaces(root);
    let value = Descriptor {
        schema_version: SCHEMA_VERSION,
        rules_version: RULES_VERSION,
        profile: PROFILE.into(),
        xml: XmlProfile {
            version: "2.20".into(),
            namespaces: context.clone(),
            attributes: attributes(root, input, "version"),
            bom: input.starts_with('\u{feff}'),
            line_ending: if input.contains("\r\n") {
                LineEnding::CrLf
            } else {
                LineEnding::Lf
            },
        },
        object: object(objects[0], input, &context)?,
    };
    value.validate()?;
    Ok(value)
}

/// Decode a native object without interpreting descendants outside its declared sections.
fn object(node: Node<'_, '_>, input: &str, inherited: &[Namespace]) -> Result<MetadataNode, Error> {
    let kind = kind(node)?;
    let context = namespaces(node);
    let mut sections = Vec::new();
    for child in meaningful(node)? {
        let section = if child.has_tag_name((MD_NAMESPACE, "Properties")) {
            section_attributes(child)?;
            Section::Properties(
                meaningful(child)?
                    .map(|part| property(part, input, &context))
                    .collect(),
            )
        } else if child.has_tag_name((MD_NAMESPACE, "ChildObjects")) {
            section_attributes(child)?;
            Section::Children(
                meaningful(child)?
                    .map(|part| child_object(part, input, &context))
                    .collect::<Result<_, _>>()?,
            )
        } else {
            Section::Preserved(fragment(child, input, &context))
        };
        sections.push(section);
    }
    Ok(MetadataNode {
        kind,
        uuid: NativeUuid::parse(
            node.attribute("uuid")
                .ok_or_else(|| Error::root("native_uuid_missing"))?,
        )?,
        namespaces: local(&context, inherited),
        attributes: attributes(node, input, "uuid"),
        sections,
    })
}

/// Inline UUIDs distinguish owned parts from references to separately stored descriptors.
fn child_object(node: Node<'_, '_>, input: &str, inherited: &[Namespace]) -> Result<Child, Error> {
    if node.is_element() && node.tag_name().namespace() == Some(MD_NAMESPACE) {
        let kind = kind(node)?;
        if node.attribute("uuid").is_some() {
            Ok(Child::Inline {
                object: Box::new(object(node, input, inherited)?),
            })
        } else if node.attributes().len() == 0 && node.children().all(|child| child.is_text()) {
            Ok(Child::Reference {
                metadata_kind: kind,
                name: text(node),
            })
        } else {
            Err(Error::root("source_reference_invalid"))
        }
    } else {
        Ok(Child::Preserved {
            fragment: fragment(node, input, inherited),
        })
    }
}

/// Promote only values with a proven inverse rule; retain every other shape as opaque XML.
fn property(node: Node<'_, '_>, input: &str, inherited: &[Namespace]) -> Property {
    let name = if node.is_element() && node.tag_name().namespace() == Some(MD_NAMESPACE) {
        match node.tag_name().name() {
            "Name" => Some(PropertyName::Name),
            "Comment" => Some(PropertyName::Comment),
            "Version" => Some(PropertyName::Version),
            "Synonym" => Some(PropertyName::Synonym),
            _ => None,
        }
    } else {
        None
    };
    if let Some(name) = name
        && let Some(value) = typed_value(node, name)
    {
        return Property::Typed { name, value };
    }
    Property::Preserved {
        fragment: fragment(node, input, inherited),
    }
}

/// Preserve empty, nil and language-keyed values without default inference or numeric coercion.
fn typed_value(node: Node<'_, '_>, name: PropertyName) -> Option<PropertyValue> {
    if node.attributes().len() == 1
        && matches!(node.attribute((XSI_NAMESPACE, "nil")), Some("true" | "1"))
        && node.children().all(|child| {
            child.is_text() && child.text().is_none_or(|value| value.trim().is_empty())
        })
    {
        return Some(PropertyValue::Nil);
    }
    if node.attributes().len() != 0 {
        return None;
    }
    if node.children().next().is_none() {
        return Some(PropertyValue::Empty);
    }
    if name == PropertyName::Synonym {
        let parts = meaningful(node)
            .ok()?
            .map(localized)
            .collect::<Option<Vec<_>>>()?;
        return Some(PropertyValue::Localized(parts));
    }
    node.children()
        .all(|child| child.is_text())
        .then(|| PropertyValue::Text(text(node)))
}

/// Language and text are recognized only in the native localized-string namespace and order.
fn localized(node: Node<'_, '_>) -> Option<LocalizedText> {
    if !node.has_tag_name((V8_NAMESPACE, "item")) || node.attributes().len() != 0 {
        return None;
    }
    let parts = meaningful(node).ok()?.collect::<Vec<_>>();
    if parts.len() != 2 {
        return None;
    }
    let values = parts
        .iter()
        .zip(["lang", "content"])
        .map(|(part, tag)| {
            (part.has_tag_name((V8_NAMESPACE, tag))
                && part.attributes().len() == 0
                && part.children().all(|child| child.is_text()))
            .then(|| text(*part))
        })
        .collect::<Option<Vec<_>>>()?;
    Some(LocalizedText {
        language: values[0].clone(),
        text: values[1].clone(),
    })
}

/// Refuse unknown native kinds rather than silently hiding them inside a preservation block.
fn kind(node: Node<'_, '_>) -> Result<MetadataKind, Error> {
    if node.tag_name().namespace() != Some(MD_NAMESPACE)
        || matches!(
            node.tag_name().name(),
            "ExternalDataProcessor" | "ExternalReport"
        )
    {
        return Err(Error::root("source_profile_unsupported"));
    }
    MetadataKind::from_xml_tag(node.tag_name().name())
        .map_err(|_| Error::root("source_kind_unsupported"))
}

/// Only inter-element whitespace may be regenerated; PI and meaningful mixed text are refused.
fn meaningful<'a, 'input>(
    node: Node<'a, 'input>,
) -> Result<impl Iterator<Item = Node<'a, 'input>>, Error> {
    if node.children().any(|child| {
        child.is_pi()
            || (child.is_text() && child.text().is_some_and(|value| !value.trim().is_empty()))
    }) {
        return Err(Error::root("source_xml_content_unsupported"));
    }
    Ok(node.children().filter(|child| !child.is_text()))
}

/// Section attributes have no modeled inverse rule yet, so refusing is safer than losing them.
fn section_attributes(node: Node<'_, '_>) -> Result<(), Error> {
    if node.attributes().len() != 0 {
        return Err(Error::root("source_section_attributes_unsupported"));
    }
    Ok(())
}

/// Capture the effective scope, including aliases used only inside QName-valued text.
fn namespaces(node: Node<'_, '_>) -> Vec<Namespace> {
    node.namespaces()
        .map(|binding| Namespace {
            prefix: binding.name().map(str::to_owned),
            uri: binding.uri().into(),
        })
        .collect()
}

/// Store only scope differences relative to the modeled owner, not every ancestor binding.
fn local(context: &[Namespace], inherited: &[Namespace]) -> Vec<Namespace> {
    let mut local: Vec<_> = context
        .iter()
        .filter(|binding| !inherited.contains(binding))
        .cloned()
        .collect();
    if !context.iter().any(|binding| binding.prefix.is_none())
        && inherited
            .iter()
            .any(|binding| binding.prefix.is_none() && !binding.uri.is_empty())
    {
        local.push(Namespace {
            prefix: None,
            uri: String::new(),
        });
    }
    local
}

/// Keep the original lexical `QName` of an extra attribute, even with multiple aliases per URI.
fn attributes(node: Node<'_, '_>, input: &str, reserved: &str) -> Vec<XmlAttribute> {
    node.attributes()
        .filter(|attribute| attribute.namespace().is_some() || attribute.name() != reserved)
        .map(|attribute| XmlAttribute {
            name: input[attribute.range_qname()].into(),
            value: attribute.value().into(),
        })
        .collect()
}

/// Raw unknown XML retains nested declarations; inherited wrapper bindings remain explicit.
fn fragment(node: Node<'_, '_>, input: &str, inherited: &[Namespace]) -> XmlFragment {
    XmlFragment {
        xml: input[node.range()].into(),
        namespaces: if node.is_element() {
            local(&namespaces(node), inherited)
        } else {
            Vec::new()
        },
    }
}

/// Concatenate native text nodes, including CDATA, according to XML value semantics.
fn text(node: Node<'_, '_>) -> String {
    node.children()
        .filter(Node::is_text)
        .filter_map(|child| child.text())
        .collect()
}
