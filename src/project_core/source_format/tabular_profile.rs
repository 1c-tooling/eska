//! Verified direct tables contain explicit generated identities, native properties and inline columns.

use super::{
    Child, Error, MetadataNode, Namespace, Property, PropertyName, PropertyValue, Section,
    attribute_profile, fragment, metadata_reference::READABLE_NAMESPACE, xml,
};
use crate::project_core::MetadataKind;

/// Removing a table cannot discard unknown service sections, owned resources or unverified column profiles.
pub(super) fn table(
    node: &MetadataNode,
    parent: &MetadataNode,
    context: &[Namespace],
) -> Result<bool, Error> {
    let [
        Section::TabularSectionIdentities(ids),
        Section::Properties(properties),
        Section::Children(children),
    ] = node.sections.as_slice()
    else {
        return Ok(false);
    };
    let [
        Property::Typed {
            name: PropertyName::Name,
            value: PropertyValue::Text(_),
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
        return Ok(false);
    };
    if node.kind != MetadataKind::TabularSection
        || !node.namespaces.is_empty()
        || !node.attributes.is_empty()
        || !matches!(synonym, PropertyValue::Empty | PropertyValue::Localized(_))
        || !matches!(comment, PropertyValue::Empty | PropertyValue::Text(_))
    {
        return Ok(false);
    }
    ids.validate(node, Some(parent), context)?;
    for property in rest {
        let Some(name) = attribute_profile::property_tag(property, context)? else {
            return Ok(false);
        };
        match name.as_str() {
            "ToolTip" | "FillChecking" | "LineNumberLength" => {}
            "Use" if parent.kind == MetadataKind::Catalog => {}
            "StandardAttributes" if standard_attributes(property, context)? => {}
            _ => return Ok(false),
        }
    }
    for child in children {
        let Child::Inline { object } = child else {
            return Ok(false);
        };
        if object.kind != MetadataKind::Attribute || !attribute_profile::attribute(object, context)?
        {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The observed standard `LineNumber` has no UUID, generated slots or descendants owning files.
fn standard_attributes(property: &Property, context: &[Namespace]) -> Result<bool, Error> {
    let Property::Preserved { fragment } = property else {
        return Ok(false);
    };
    let wrapped = fragment::wrap(fragment, context)?;
    let document = roxmltree::Document::parse(&wrapped)
        .map_err(|_| Error::root("source_fragment_xml_invalid"))?;
    let Some(root) = document.root_element().first_element_child() else {
        return Ok(false);
    };
    let Some(fields) = xml::typed_elements(root) else {
        return Ok(false);
    };
    let [field] = fields.as_slice() else {
        return Ok(false);
    };
    if root.attributes().len() != 0
        || declares_namespace(root, &wrapped)
        || !field.has_tag_name((READABLE_NAMESPACE, "StandardAttribute"))
        || field.attributes().len() != 1
        || field.attribute("name") != Some("LineNumber")
        || declares_namespace(*field, &wrapped)
    {
        return Ok(false);
    }
    let Some(values) = xml::typed_elements(*field) else {
        return Ok(false);
    };
    let expected = [
        "LinkByType",
        "FillChecking",
        "MultiLine",
        "FillFromFillingValue",
        "CreateOnInput",
        "TypeReductionMode",
        "MaxValue",
        "ToolTip",
        "ExtendedEdit",
        "Format",
        "ChoiceForm",
        "QuickChoice",
        "ChoiceHistoryOnInput",
        "EditFormat",
        "PasswordMode",
        "DataHistory",
        "MarkNegatives",
        "MinValue",
        "Synonym",
        "Comment",
        "FullTextSearch",
        "ChoiceParameterLinks",
        "FillValue",
        "Mask",
        "ChoiceParameters",
    ];
    Ok(values.len() == expected.len()
        && values.iter().zip(expected).all(|(value, name)| {
            value.has_tag_name((READABLE_NAMESPACE, name)) && standard_value(*value, name, &wrapped)
        }))
}

/// Known leaf values retain absence/nil/text; only the observed localized synonym can have elements.
fn standard_value(node: roxmltree::Node<'_, '_>, name: &str, input: &str) -> bool {
    if declares_namespace(node, input) {
        return false;
    }
    if node.attributes().len() != 0
        && !(matches!(name, "MinValue" | "MaxValue" | "FillValue")
            && node.attributes().len() == 1
            && node.attribute(("http://www.w3.org/2001/XMLSchema-instance", "nil")) == Some("true"))
    {
        return false;
    }
    if node.children().all(|child| child.is_text()) {
        return true;
    }
    if name != "Synonym" || node.attributes().len() != 0 {
        return false;
    }
    let Some(items) = xml::typed_elements(node) else {
        return false;
    };
    !items.is_empty()
        && items.into_iter().all(|item| {
            if !item.has_tag_name(("http://v8.1c.ru/8.1/data/core", "item"))
                || item.attributes().len() != 0
                || declares_namespace(item, input)
            {
                return false;
            }
            let Some(fields) = xml::typed_elements(item) else {
                return false;
            };
            fields.len() == 2
                && fields
                    .into_iter()
                    .zip(["lang", "content"])
                    .all(|(field, name)| {
                        field.has_tag_name(("http://v8.1c.ru/8.1/data/core", name))
                            && field.attributes().len() == 0
                            && field.children().all(|child| child.is_text())
                            && !declares_namespace(field, input)
                    })
        })
}

/// Local bindings remain unverified even when a repeated declaration resolves to the inherited URI.
fn declares_namespace(node: roxmltree::Node<'_, '_>, input: &str) -> bool {
    input[node.range()].split('>').next().is_none_or(|head| {
        head.split_whitespace()
            .any(|part| part.starts_with("xmlns"))
    })
}
