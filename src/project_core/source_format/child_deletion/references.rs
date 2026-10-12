//! Preserved XML is searched conservatively, without inventing inverse rules or rewriting references.

use std::{collections::BTreeMap, path::Path};

use crate::project_core::{MetadataKind, source_format::PropertyValue};

use super::super::{
    AcceptedBase, Child, Error, MetadataNode, Namespace, NativeUuid, Property, PropertyName,
    Section, SourcePath, XmlFragment, export::Package, fragment, validation::scope,
};

/// Validate selected references only when accepted enum values were actually removed.
pub(in crate::project_core::source_format) fn validate_references(
    base: &AcceptedBase,
    package: &Package,
    mut read_payload: impl FnMut(&SourcePath) -> Result<String, Error>,
) -> Result<(), Error> {
    let mut removed = BTreeMap::new();
    for (path, accepted) in &base.package.documents {
        let owner = &accepted.descriptor.object;
        if owner.kind != MetadataKind::Enum {
            continue;
        }
        let selected = package
            .documents
            .get(path)
            .ok_or_else(|| Error::root("source_descriptor_missing"))?;
        for node in super::removed_values(owner, &selected.descriptor.object) {
            let (Some(PropertyValue::Text(parent)), Some(PropertyValue::Text(name))) = (
                owner.property(PropertyName::Name),
                node.property(PropertyName::Name),
            ) else {
                return Err(Error::root("source_name_missing"));
            };
            removed.insert(node.uuid.key(), node.uuid.clone());
            removed.insert(
                format!("Enum.{parent}.EnumValue.{name}").to_lowercase(),
                node.uuid.clone(),
            );
        }
    }
    if removed.is_empty() {
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
    for (native, source) in &package.payload_sources {
        if native.as_str() == "ConfigDumpInfo.xml" || !is_xml(native.as_path()) {
            continue;
        }
        let text = read_payload(source)?;
        validate_xml(&text, native, &removed)?;
    }
    Ok(())
}

/// Metadata bindings distinguish preserved XML from typed fields already checked by the target index.
fn validate_node(
    node: &MetadataNode,
    inherited: &[Namespace],
    path: &SourcePath,
    removed: &BTreeMap<String, NativeUuid>,
) -> Result<(), Error> {
    let context = scope(inherited, &node.namespaces)?;
    for attribute in &node.attributes {
        validate_text(&attribute.value, path, removed)?;
    }
    for section in &node.sections {
        match section {
            Section::Properties(properties) => {
                for property in properties {
                    if let Property::Preserved { fragment } = property {
                        validate_fragment(fragment, &context, path, removed)?;
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
        }
    }
    Ok(())
}

/// Namespace restoration preserves entity decoding and CDATA before checking unresolved dependency text.
fn validate_fragment(
    value: &XmlFragment,
    inherited: &[Namespace],
    path: &SourcePath,
    removed: &BTreeMap<String, NativeUuid>,
) -> Result<(), Error> {
    validate_xml(&fragment::wrap(value, inherited)?, path, removed)
}

/// Matches in decoded text or attributes refuse removal; comments and processing instructions have no authority.
fn validate_xml(
    text: &str,
    path: &SourcePath,
    removed: &BTreeMap<String, NativeUuid>,
) -> Result<(), Error> {
    let document = roxmltree::Document::parse(text)
        .map_err(|_| Error::root("source_structural_xml_unverified").in_field(path.as_str()))?;
    for node in document.descendants() {
        let values = node
            .text()
            .filter(|_| node.is_text())
            .into_iter()
            .chain(node.attributes().map(|attribute| attribute.value()));
        for value in values {
            validate_text(value, path, removed)?;
        }
    }
    Ok(())
}

/// An opaque mention may be a dependency; its inverse semantics cannot be guessed from textual shape.
fn validate_text(
    value: &str,
    path: &SourcePath,
    removed: &BTreeMap<String, NativeUuid>,
) -> Result<(), Error> {
    let value = value.to_lowercase();
    if let Some((_, uuid)) = removed.iter().find(|(key, _)| value.contains(key.as_str())) {
        return Err(Error::root("source_structural_reference_preserved")
            .in_field(uuid.as_str())
            .in_field(path.as_str()));
    }
    Ok(())
}

/// Only native XML payloads have verified decoded coordinates; other resource formats remain outside this guard.
fn is_xml(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
}
