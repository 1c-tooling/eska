//! Session-parameter parsing rejects ambiguous identities before publishing a typed value.

use crate::{MetadataError, ObjectBelonging, Result, SessionParameter};

const MD: &str = "http://v8.1c.ru/8.3/MDClasses";

/// Parse one native descriptor; a missing type is incomplete rather than an invented default.
pub fn parse_session_parameter_xml(xml: &str) -> Result<SessionParameter> {
    let doc = super::helpers::parse_xml(xml)?;
    let root = doc.root_element();
    if !root.has_tag_name((MD, "MetaDataObject")) {
        return Err(invalid("SessionParameter requires a native metadata root"));
    }
    let object = required_child(root, "SessionParameter")?;
    if root.children().filter(|child| child.is_element()).count() != 1 {
        return Err(invalid("SessionParameter requires one native metadata declaration"));
    }
    let uuid =
        super::helpers::parse_uuid(object.attribute("uuid").unwrap_or(""), "session parameter")?;
    let properties = required_child(object, "Properties")?;
    let name = text(required_child(properties, "Name")?)?;
    if name.is_empty() {
        return Err(invalid("SessionParameter requires Name"));
    }
    let attribute_type =
        super::type_parser::parse_type_xml_strict(required_child(properties, "Type")?)?;
    let belonging = match child(properties, "ObjectBelonging")?.map(text).transpose()? {
        None => ObjectBelonging::Unknown,
        Some("Own") => ObjectBelonging::Own,
        Some("Adopted") => ObjectBelonging::Adopted,
        Some(_) => return Err(invalid("Invalid SessionParameter ObjectBelonging")),
    };
    let extends = child(properties, "ExtendedConfigurationObject")?
        .map(text)
        .transpose()?
        .filter(|value| !value.is_empty())
        .map(|value| super::helpers::parse_uuid(value, "session parameter adoption"))
        .transpose()?;
    if belonging == ObjectBelonging::Adopted && extends.is_none() {
        return Err(invalid("Adopted SessionParameter requires ExtendedConfigurationObject"));
    }
    if belonging != ObjectBelonging::Adopted && extends.is_some() {
        return Err(invalid("Only adopted SessionParameter can reference a base identity"));
    }
    if child(object, "ChildObjects")?
        .is_some_and(|node| node.children().any(|child| child.is_element()))
    {
        return Err(invalid("SessionParameter has no child metadata"));
    }
    Ok(SessionParameter::from_parts(uuid, name.into(), attribute_type, belonging, extends))
}

/// Select a schema child once; duplicate fields cannot silently replace current input.
fn child<'a, 'input>(
    parent: roxmltree::Node<'a, 'input>,
    name: &str,
) -> Result<Option<roxmltree::Node<'a, 'input>>> {
    let mut matches = parent.children().filter(|node| node.has_tag_name((MD, name)));
    let first = matches.next();
    if matches.next().is_some() {
        return Err(invalid("Duplicate SessionParameter property"));
    }
    Ok(first)
}

/// Required typed fields must be present in the metadata namespace.
fn required_child<'a, 'input>(
    parent: roxmltree::Node<'a, 'input>,
    name: &str,
) -> Result<roxmltree::Node<'a, 'input>> {
    child(parent, name)?.ok_or_else(|| invalid("Missing SessionParameter property"))
}

/// Scalar properties cannot hide nested values.
fn text<'a>(node: roxmltree::Node<'a, '_>) -> Result<&'a str> {
    if node.children().any(|child| child.is_element()) {
        return Err(invalid("Expected scalar SessionParameter property"));
    }
    Ok(node.text().unwrap_or(""))
}

/// Use the native structured error boundary for unsupported current descriptors.
fn invalid(message: &str) -> MetadataError {
    MetadataError::InvalidFormat(message.into())
}
