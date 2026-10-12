//! Reject ambiguous identity and incomplete source values before materialization.

use std::collections::{BTreeMap, BTreeSet};

use crate::project_core::MetadataKind;

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
        if !matches!(
            self.schema_version,
            SCHEMA_VERSION | super::READABLE_SCHEMA_VERSION
        ) {
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
                properties(values, &context, object.kind)?;
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
fn properties(
    values: &[Property],
    context: &[Namespace],
    owner: MetadataKind,
) -> Result<(), Error> {
    let mut names = BTreeSet::new();
    for value in values {
        match value {
            Property::Typed { name, value } => {
                if !names.insert((MD_NAMESPACE.to_owned(), name.as_str().to_owned())) {
                    return Err(Error::root("source_property_duplicate"));
                }
                if name
                    .value_kind(owner)
                    .is_none_or(|kind| !kind.accepts(value))
                {
                    return Err(Error::root("source_property_type_invalid").in_field(name.as_str()));
                }
                super::metadata_reference::validate_property(*name, owner, value, context)?;
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
                    (_, PropertyValue::Text(text)) => {
                        if text.is_empty() {
                            return Err(Error::root("source_property_type_invalid"));
                        }
                        xml_text(text)?;
                    }
                    (
                        _,
                        PropertyValue::Empty
                        | PropertyValue::Nil
                        | PropertyValue::Boolean(_)
                        | PropertyValue::Integer(_)
                        | PropertyValue::Reference(_)
                        | PropertyValue::ReferenceList(_),
                    ) => {}
                    (_, PropertyValue::Enumeration(text)) => xml_text(text)?,
                    (_, PropertyValue::TypeDescription(value)) => value.validate(context)?,
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
    let wrapper = super::fragment::wrap(value, inherited)?;
    crate::project_core::native::check_envelope(&wrapper)
        .map_err(|_| Error::root("source_preserved_xml_invalid"))?;
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
pub(super) fn scope(inherited: &[Namespace], local: &[Namespace]) -> Result<Vec<Namespace>, Error> {
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

/// XML 1.0 Fifth Edition names admit native Unicode identifiers, excluding namespace separators.
pub(super) fn ncname(value: &str) -> bool {
    let mut characters = value.chars();
    characters.next().is_some_and(name_start) && characters.all(|ch| name_start(ch)
        || matches!(ch, '-' | '.' | '0'..='9' | '\u{b7}' | '\u{300}'..='\u{36f}' | '\u{203f}'..='\u{2040}'))
}

/// Keep the normative `NameStartChar` ranges independent from Unicode letter classifications.
const fn name_start(ch: char) -> bool {
    matches!(ch, 'A'..='Z' | '_' | 'a'..='z' | '\u{c0}'..='\u{d6}' | '\u{d8}'..='\u{f6}'
        | '\u{f8}'..='\u{2ff}' | '\u{370}'..='\u{37d}' | '\u{37f}'..='\u{1fff}'
        | '\u{200c}'..='\u{200d}' | '\u{2070}'..='\u{218f}' | '\u{2c00}'..='\u{2fef}'
        | '\u{3001}'..='\u{d7ff}' | '\u{f900}'..='\u{fdcf}' | '\u{fdf0}'..='\u{fffd}'
        | '\u{10000}'..='\u{effff}')
}
