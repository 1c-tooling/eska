//! Reject ambiguous identity and incomplete source values before materialization.

use std::collections::{BTreeMap, BTreeSet};

use crate::project::metadata_model::MetadataKind;

use super::{
    Child, Descriptor, Error, MD_NAMESPACE, MetadataNode, Namespace, PROFILE, Property,
    PropertyName, PropertyValue, RULES_VERSION, SCHEMA_VERSION, Section, XmlAttribute, XmlFragment,
};

const MAX_DEPTH: usize = 64;

impl Descriptor {
    /// Check schema/rule versions, `QName` context, identity and typed value invariants.
    ///
    /// # Errors
    /// Returns a structured error without normalizing or migrating any document.
    pub fn validate(&self) -> Result<(), Error> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(Error::root("source_schema_unsupported"));
        }
        if self.rules_version != RULES_VERSION {
            return Err(Error::root("source_rules_unsupported"));
        }
        if self.profile != PROFILE || self.xml.version != "2.20" {
            return Err(Error::root("source_profile_unsupported"));
        }
        namespaces(&self.xml.namespaces)?;
        if !self
            .xml
            .namespaces
            .iter()
            .any(|binding| binding.prefix.is_none() && binding.uri == MD_NAMESPACE)
        {
            return Err(Error::root("source_namespace_invalid"));
        }
        attributes(&self.xml.attributes, &["version"], &self.xml.namespaces)?;
        node(&self.object, 0, &mut BTreeSet::new(), &self.xml.namespaces)
            .map_err(|error| error.in_field("object"))
    }
}

/// Validate one owner's complete sections and all descendants in their native order.
fn node(
    object: &MetadataNode,
    depth: usize,
    uuids: &mut BTreeSet<String>,
    inherited: &[Namespace],
) -> Result<(), Error> {
    if depth > MAX_DEPTH {
        return Err(Error::root("source_depth_exceeded"));
    }
    if !uuids.insert(object.uuid.key()) {
        return Err(Error::root("native_uuid_duplicate"));
    }
    let context = scope(inherited, &object.namespaces)?;
    if !context
        .iter()
        .any(|binding| binding.prefix.is_none() && binding.uri == MD_NAMESPACE)
    {
        return Err(Error::root("source_namespace_invalid"));
    }
    attributes(&object.attributes, &["uuid"], &context)?;
    let mut has_properties = false;
    let mut has_children = false;
    for section in &object.sections {
        match section {
            Section::Properties(values) => {
                if has_properties {
                    return Err(Error::root("source_section_duplicate"));
                }
                has_properties = true;
                properties(values, &context)?;
            }
            Section::Children(values) => {
                if has_children {
                    return Err(Error::root("source_section_duplicate"));
                }
                has_children = true;
                children(values, depth, uuids, &context)?;
            }
            Section::Preserved(value) => {
                if fragment(value, &context)?.is_some_and(|(namespace, tag)| {
                    namespace == MD_NAMESPACE
                        && (matches!(tag.as_str(), "Properties" | "ChildObjects")
                            || MetadataKind::from_xml_tag(&tag).is_ok())
                }) {
                    return Err(Error::root("source_preserved_slot_invalid"));
                }
            }
        }
    }
    if !matches!(object.property(PropertyName::Name), Some(PropertyValue::Text(name)) if !name.is_empty())
    {
        return Err(Error::root("source_name_missing"));
    }
    Ok(())
}

/// Reject duplicate typed properties and invalid value kinds without guessing coercions.
fn properties(values: &[Property], context: &[Namespace]) -> Result<(), Error> {
    let mut names = BTreeSet::new();
    for value in values {
        match value {
            Property::Typed { name, value } => {
                if !names.insert((MD_NAMESPACE.to_owned(), name.as_str().to_owned())) {
                    return Err(Error::root("source_property_duplicate"));
                }
                match (name, value) {
                    (PropertyName::Synonym, PropertyValue::Localized(items)) => {
                        let mut languages = BTreeSet::new();
                        for item in items {
                            xml_text(&item.language)?;
                            xml_text(&item.text)?;
                            if item.language.is_empty() || !languages.insert(&item.language) {
                                return Err(Error::root("source_language_duplicate"));
                            }
                        }
                    }
                    (PropertyName::Synonym, PropertyValue::Text(_))
                    | (_, PropertyValue::Localized(_)) => {
                        return Err(Error::root("source_property_type_invalid"));
                    }
                    (_, PropertyValue::Text(text)) => xml_text(text)?,
                    (_, PropertyValue::Empty | PropertyValue::Nil) => {}
                }
            }
            Property::Preserved { fragment: value } => {
                if let Some(name) = fragment(value, context)?
                    && !names.insert(name)
                {
                    return Err(Error::root("source_property_duplicate"));
                }
            }
        }
    }
    Ok(())
}

/// Check declared child uniqueness while retaining their sequence as the native order.
fn children(
    values: &[Child],
    depth: usize,
    uuids: &mut BTreeSet<String>,
    context: &[Namespace],
) -> Result<(), Error> {
    let mut references = BTreeSet::new();
    for value in values {
        match value {
            Child::Inline { object } => node(object, depth + 1, uuids, context)?,
            Child::Reference {
                metadata_kind,
                name,
            } => {
                xml_text(name)?;
                if name.is_empty() || !references.insert((*metadata_kind, name)) {
                    return Err(Error::root("source_reference_duplicate"));
                }
            }
            Child::Preserved { fragment: value } => {
                if fragment(value, context)?.is_some_and(|(namespace, tag)| {
                    namespace == MD_NAMESPACE && MetadataKind::from_xml_tag(&tag).is_ok()
                }) {
                    return Err(Error::root("source_preserved_slot_invalid"));
                }
            }
        }
    }
    Ok(())
}

/// Namespace prefixes remain unique and XML's reserved bindings cannot be redefined.
fn namespaces(values: &[Namespace]) -> Result<(), Error> {
    let mut prefixes = BTreeSet::new();
    for value in values {
        xml_text(&value.uri)?;
        if (value.uri.is_empty() && value.prefix.is_some()) || !prefixes.insert(&value.prefix) {
            return Err(Error::root("source_namespace_invalid"));
        }
        if let Some(prefix) = &value.prefix
            && (!xml_name(prefix)
                || prefix == "xmlns"
                || (prefix == "xml" && value.uri != "http://www.w3.org/XML/1998/namespace"))
        {
            return Err(Error::root("source_namespace_invalid"));
        }
    }
    Ok(())
}

/// Ensure an attribute cannot duplicate identity/version or inject native markup.
fn attributes(
    values: &[XmlAttribute],
    reserved: &[&str],
    context: &[Namespace],
) -> Result<(), Error> {
    let mut names = BTreeSet::new();
    for value in values {
        xml_text(&value.value)?;
        if let Some((prefix, _)) = value.name.split_once(':')
            && prefix != "xml"
            && !context
                .iter()
                .any(|binding| binding.prefix.as_deref() == Some(prefix))
        {
            return Err(Error::root("source_attribute_namespace_missing"));
        }
        if !value.name.split(':').all(xml_name)
            || value.name.matches(':').count() > 1
            || reserved.contains(&value.name.as_str())
            || value.name.starts_with("xmlns")
            || !names.insert(&value.name)
        {
            return Err(Error::root("source_attribute_invalid"));
        }
    }
    Ok(())
}

/// Conservatively validate the ASCII native vocabulary used by this experimental profile.
fn xml_name(value: &str) -> bool {
    value
        .as_bytes()
        .first()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

/// Check opaque XML independently; namespaces in `QName` values remain in the explicit context.
fn fragment(
    value: &XmlFragment,
    inherited: &[Namespace],
) -> Result<Option<(String, String)>, Error> {
    let context = scope(inherited, &value.namespaces)?;
    if value.xml.contains("<!DOCTYPE") || value.xml.contains("<!ENTITY") {
        return Err(Error::root("source_xml_dtd_forbidden"));
    }
    let mut wrapper = String::from("<preserved");
    for binding in &context {
        wrapper.push_str(" xmlns");
        if let Some(prefix) = &binding.prefix {
            wrapper.push(':');
            wrapper.push_str(prefix);
        }
        wrapper.push_str("=\"");
        wrapper.push_str(
            &binding
                .uri
                .replace('&', "&amp;")
                .replace('"', "&quot;")
                .replace('<', "&lt;"),
        );
        wrapper.push('"');
    }
    wrapper.push('>');
    wrapper.push_str(&value.xml);
    wrapper.push_str("</preserved>");
    let document = roxmltree::Document::parse(&wrapper)
        .map_err(|_| Error::root("source_preserved_xml_invalid"))?;
    if document
        .root_element()
        .children()
        .filter(|child| child.is_element() || child.is_comment())
        .count()
        != 1
    {
        return Err(Error::root("source_preserved_xml_invalid"));
    }
    if document
        .root_element()
        .children()
        .any(|child| child.is_text() && child.text().is_some_and(|text| !text.trim().is_empty()))
    {
        return Err(Error::root("source_preserved_xml_invalid"));
    }
    Ok(document
        .root_element()
        .children()
        .find(roxmltree::Node::is_element)
        .map(|child| {
            (
                child.tag_name().namespace().unwrap_or_default().to_owned(),
                child.tag_name().name().to_owned(),
            )
        }))
}

/// Store only local overrides; expand inherited bindings at validation/materialization time.
fn scope(inherited: &[Namespace], local: &[Namespace]) -> Result<Vec<Namespace>, Error> {
    namespaces(local)?;
    let mut values: BTreeMap<_, _> = inherited
        .iter()
        .map(|binding| (binding.prefix.clone(), binding.uri.clone()))
        .collect();
    for binding in local {
        values.insert(binding.prefix.clone(), binding.uri.clone());
    }
    Ok(values
        .into_iter()
        .map(|(prefix, uri)| Namespace { prefix, uri })
        .collect())
}

/// XML 1.0 cannot round-trip control characters merely because YAML accepts their escapes.
fn xml_text(value: &str) -> Result<(), Error> {
    if value.chars().any(|character| {
        !matches!(character, '\t' | '\n' | '\r')
            && (character < ' ' || matches!(character, '\u{fffe}' | '\u{ffff}'))
    }) {
        return Err(Error::root("source_xml_character_invalid"));
    }
    Ok(())
}
