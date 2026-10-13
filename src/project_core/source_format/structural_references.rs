//! Preserved XML is searched conservatively, without inventing inverse rules or rewriting references.

use std::{collections::BTreeMap, path::Path};

use crate::project_core::{MetadataKind, source_format::PropertyValue};

use super::{
    AcceptedBase, Child, Error, MetadataNode, Namespace, NativeUuid, Property, PropertyName,
    Section, SourcePath, XmlFragment, export::Package, fragment, tabular_identities,
    type_description, validation::scope,
};

/// Exact native keys retain enum behavior; field tokens also cover local form paths and BSL mentions.
#[derive(Default)]
struct Invalidated {
    exact: BTreeMap<String, NativeUuid>,
    fields: BTreeMap<String, NativeUuid>,
}

/// Validate selected references only when removal or renaming invalidates an accepted dependency key.
pub(super) fn validate_references(
    base: &AcceptedBase,
    package: &Package,
    mut read_payload: impl FnMut(&SourcePath) -> Result<String, Error>,
) -> Result<(), Error> {
    let removed = invalidated_keys(base, package)?;
    if removed.exact.is_empty() && removed.fields.is_empty() {
        return Ok(());
    }
    for (path, document) in &package.documents {
        for attribute in &document.descriptor.xml.attributes {
            validate_text(&attribute.value, path, &removed)?;
        }
        validate_node(
            &document.descriptor.object,
            &document.descriptor.xml.namespaces,
            path,
            &removed,
        )?;
    }
    for (path, value) in &package.query_inputs.values {
        validate_text(value, path, &removed)?;
    }
    for (native, source) in &package.payload_sources {
        if native.as_str() == "ConfigDumpInfo.xml" {
            continue;
        }
        if is_xml(native.as_path()) {
            validate_xml(&read_payload(source)?, native, &removed)?;
        } else if !removed.fields.is_empty()
            && native
                .as_path()
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("bsl"))
        {
            validate_text(&read_payload(source)?, native, &removed)?;
        }
    }
    Ok(())
}

/// Removed values invalidate UUID and name; renamed values keep UUID and case-insensitive identity.
fn invalidated_keys(base: &AcceptedBase, package: &Package) -> Result<Invalidated, Error> {
    let mut result = Invalidated::default();
    for (path, accepted) in &base.package.documents {
        let owner = &accepted.descriptor.object;
        let selected = &package
            .documents
            .get(path)
            .ok_or_else(|| Error::root("source_descriptor_missing"))?
            .descriptor
            .object;
        if matches!(owner.kind, MetadataKind::Catalog | MetadataKind::Document) {
            invalidated_fields(owner, selected, &mut result)?;
        }
        if owner.kind != MetadataKind::Enum {
            continue;
        }
        let values: BTreeMap<_, _> = super::child_rename::values(selected)
            .map(|node| (node.uuid.key(), node))
            .collect();
        for node in super::child_rename::values(owner) {
            let next = values.get(&node.uuid.key());
            let (Some(PropertyValue::Text(parent)), Some(PropertyValue::Text(name))) = (
                owner.property(PropertyName::Name),
                node.property(PropertyName::Name),
            ) else {
                return Err(Error::root("source_name_missing"));
            };
            if let Some(next) = next {
                let Some(PropertyValue::Text(next_name)) = next.property(PropertyName::Name) else {
                    return Err(Error::root("source_name_missing"));
                };
                if name.to_lowercase() == next_name.to_lowercase() {
                    continue;
                }
            } else {
                result.exact.insert(node.uuid.key(), node.uuid.clone());
            }
            result.exact.insert(
                format!("Enum.{parent}.EnumValue.{name}").to_lowercase(),
                node.uuid.clone(),
            );
        }
    }
    Ok(result)
}

/// Removed fields invalidate UUID and name; renamed fields retain UUID and case-insensitive name identity.
fn invalidated_fields(
    accepted: &MetadataNode,
    selected: &MetadataNode,
    result: &mut Invalidated,
) -> Result<(), Error> {
    let selected: BTreeMap<_, _> = fields(selected)
        .map(|node| (node.uuid.key(), node))
        .collect();
    for node in fields(accepted) {
        let next = selected.get(&node.uuid.key());
        if next.is_some_and(|next| {
            let before = node.property(PropertyName::Name);
            let after = next.property(PropertyName::Name);
            before == after
                || matches!((before, after), (Some(PropertyValue::Text(left)), Some(PropertyValue::Text(right))) if left.to_lowercase() == right.to_lowercase())
        }) {
            continue;
        }
        let Some(PropertyValue::Text(name)) = node.property(PropertyName::Name) else {
            return Err(Error::root("source_name_missing"));
        };
        if name.is_empty() {
            return Err(Error::root("source_name_missing"));
        }
        if next.is_none() {
            result.exact.insert(node.uuid.key(), node.uuid.clone());
            for section in &node.sections {
                if let Section::TabularSectionIdentities(ids) = section {
                    for uuid in ids.uuids() {
                        result.exact.insert(uuid.key(), node.uuid.clone());
                    }
                }
            }
        }
        if node.kind == MetadataKind::TabularSection {
            for name in tabular_identities::names(node, Some(accepted))? {
                result.exact.insert(name.to_lowercase(), node.uuid.clone());
            }
        }
        result.fields.insert(name.to_lowercase(), node.uuid.clone());
    }
    Ok(())
}

/// Declared parentage avoids interpreting arbitrary nested metadata or resources as a local field.
fn fields(root: &MetadataNode) -> impl Iterator<Item = &MetadataNode> {
    children(root)
        .filter(|node| {
            matches!(
                node.kind,
                MetadataKind::Attribute | MetadataKind::TabularSection
            )
        })
        .chain(
            children(root)
                .filter(|node| node.kind == MetadataKind::TabularSection)
                .flat_map(children)
                .filter(|node| node.kind == MetadataKind::Attribute),
        )
}

/// The same child list carries actual native identity in both descriptor transports.
fn children(node: &MetadataNode) -> impl Iterator<Item = &MetadataNode> {
    node.sections
        .iter()
        .filter_map(|section| match section {
            Section::Children(children) => Some(children),
            _ => None,
        })
        .flatten()
        .filter_map(|child| match child {
            Child::Inline { object } => Some(object.as_ref()),
            _ => None,
        })
}

/// Typed targets must not silently bind a different table that reuses an invalidated generated name.
fn validate_node(
    node: &MetadataNode,
    inherited: &[Namespace],
    path: &SourcePath,
    removed: &Invalidated,
) -> Result<(), Error> {
    let context = scope(inherited, &node.namespaces)?;
    for attribute in &node.attributes {
        validate_text(&attribute.value, path, removed)?;
    }
    for section in &node.sections {
        match section {
            Section::Properties(properties) => {
                for property in properties {
                    match property {
                        Property::Preserved { fragment } => {
                            validate_fragment(fragment, &context, path, removed)?;
                        }
                        Property::Typed {
                            value: PropertyValue::TypeDescription(description),
                            ..
                        } => {
                            for alternative in &description.data.alternatives {
                                if let Some(name) =
                                    type_description::named_target(alternative, &context)?
                                    && let Some(uuid) = removed.exact.get(&name.to_lowercase())
                                {
                                    return Err(reference_error(uuid, path));
                                }
                            }
                        }
                        Property::Typed { .. } => {}
                    }
                }
            }
            Section::Children(children) => {
                for child in children {
                    match child {
                        Child::Inline { object } => validate_node(object, &context, path, removed)?,
                        Child::Preserved { fragment } => {
                            validate_fragment(fragment, &context, path, removed)?;
                        }
                        Child::Reference { .. } => {}
                    }
                }
            }
            Section::Preserved(value) => validate_fragment(value, &context, path, removed)?,
            Section::TabularSectionIdentities(_) => {}
        }
    }
    Ok(())
}

/// Namespace restoration preserves entity decoding and CDATA before checking unresolved dependency text.
fn validate_fragment(
    value: &XmlFragment,
    inherited: &[Namespace],
    path: &SourcePath,
    removed: &Invalidated,
) -> Result<(), Error> {
    validate_xml(&fragment::wrap(value, inherited)?, path, removed)
}

/// Decoded mentions refuse an invalidated reference; comments and processing instructions have no authority.
fn validate_xml(text: &str, path: &SourcePath, removed: &Invalidated) -> Result<(), Error> {
    let document = roxmltree::Document::parse(text)
        .map_err(|_| Error::root("source_structural_xml_unverified").in_field(path.as_str()))?;
    for node in document.descendants().filter(roxmltree::Node::is_element) {
        if !removed.fields.is_empty() {
            validate_text(node.tag_name().name(), path, removed)?;
        }
        for attribute in node.attributes() {
            validate_text(attribute.value(), path, removed)?;
        }
        validate_element_text(node, path, removed)?;
    }
    Ok(())
}

/// Comments and PIs do not split a value; child elements separate independent text runs.
fn validate_element_text(
    node: roxmltree::Node<'_, '_>,
    path: &SourcePath,
    removed: &Invalidated,
) -> Result<(), Error> {
    let mut value = String::new();
    for child in node.children() {
        if child.is_element() {
            validate_text(&value, path, removed)?;
            value.clear();
        } else if child.is_text() {
            value.push_str(child.text().unwrap_or_default());
        }
    }
    validate_text(&value, path, removed)
}

/// An opaque mention may be a dependency; its inverse semantics cannot be guessed from textual shape.
fn validate_text(value: &str, path: &SourcePath, removed: &Invalidated) -> Result<(), Error> {
    let value = value.to_lowercase();
    let dependency = removed
        .exact
        .iter()
        .find(|(key, _)| value.contains(key.as_str()))
        .or_else(|| {
            removed
                .fields
                .iter()
                .find(|(key, _)| contains_identifier(&value, key))
        });
    if let Some((_, uuid)) = dependency {
        return Err(reference_error(uuid, path));
    }
    Ok(())
}

/// Both typed and opaque mentions report the dependency's accepted UUID and actual selected native path.
fn reference_error(uuid: &NativeUuid, path: &SourcePath) -> Error {
    Error::root("source_structural_reference_preserved")
        .in_field(uuid.as_str())
        .in_field(path.as_str())
}

/// Field mentions require identifier boundaries; substrings in unrelated names are not references.
fn contains_identifier(text: &str, name: &str) -> bool {
    text.match_indices(name).any(|(start, _)| {
        let boundary = |ch: char| !(ch == '_' || ch.is_alphanumeric());
        text[..start].chars().next_back().is_none_or(boundary)
            && text[start + name.len()..]
                .chars()
                .next()
                .is_none_or(boundary)
    })
}

/// Only native XML payloads have verified decoded coordinates; other resource formats remain outside this guard.
fn is_xml(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
}
