//! On-demand coordinates belong to captured input bytes; this mapper has no filesystem or IDE.

#[cfg(test)]
mod tests;

use std::ops::Range;

use roxmltree::Node;
use serde::Serialize;
use sha2::{Digest, Sha256};
use yaml_edit::YamlFile;

use super::{FileOrigin, InputAuthority, InputFile, MappedFile, validate_origin};
use crate::project_core::MetadataKind;
use crate::project_core::source_format::{
    Error, MD_NAMESPACE, Namespace, NativeUuid, ObjectSource, Property, PropertyName,
    PropertyValue, Section, document_paths, files, tabular_identities, xml,
    yaml::{self, Step},
};

/// Supplied authoring text must match the origin's captured size and checksum before parsing.
#[derive(Clone, Copy, Debug)]
pub struct OriginTexts<'a> {
    pub accepted: &'a str,
    pub working: Option<&'a str>,
}

/// UTF-8 byte ranges refer to this exact input revision; an absent range means file-level only.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SourceLocation {
    pub authority: InputAuthority,
    pub input: InputFile,
    pub range: Option<Range<usize>>,
    pub uuid: Option<NativeUuid>,
}

impl MappedFile {
    /// Map a native byte span to selected BSL/resource bytes or an object/property in owner YAML.
    ///
    /// Payload offsets remain unchanged. Existing metadata uses accepted bindings and selects
    /// working YAML for changed typed values. Fresh supported inline nodes require matching
    /// working/native models. A changed accepted identity has no inferred working coordinate.
    ///
    /// # Errors
    /// Returns changed input bytes, invalid spans, missing origin text or inconsistent metadata.
    pub fn locate(
        &self,
        native: &[u8],
        range: Range<usize>,
        texts: Option<OriginTexts<'_>>,
    ) -> Result<SourceLocation, Error> {
        validate_origin(self)?;
        verify_bytes(native, self.bytes, &self.sha256)?;
        if range.start > range.end || range.end > native.len() {
            return Err(Error::root("source_map_span_invalid"));
        }
        if let Some(location) = super::query::plain_location(self, native, &range)? {
            return Ok(location);
        }
        if super::created::is_created(&self.origin) {
            return super::created::locate(self, native, range, texts);
        }
        match &self.origin {
            FileOrigin::Payload {
                authority,
                input,
                owner,
                ..
            } => Ok(SourceLocation {
                authority: *authority,
                input: input.clone(),
                range: Some(range),
                uuid: owner.clone(),
            }),
            FileOrigin::Metadata {
                uuid,
                accepted,
                working,
                ..
            } => metadata_location(
                self,
                uuid,
                accepted,
                working.as_ref(),
                native,
                &range,
                texts.ok_or_else(|| Error::root("source_map_origin_text_missing"))?,
            ),
            FileOrigin::ManagedForm { .. } => super::form::locate(
                self,
                native,
                &range,
                texts.ok_or_else(|| Error::root("source_map_origin_text_missing"))?,
            ),
            FileOrigin::QueryResource { .. } => {
                super::query::resource_location(self, native, &range, texts)
            }
            FileOrigin::CreatedMetadata { .. } | FileOrigin::CreatedManagedForm { .. } => {
                Err(Error::root("source_map_origin_invalid"))
            }
        }
    }
}

/// A property ordinal follows accepted native order, including preserved comments and slots.
enum Hit {
    Object(NativeUuid),
    Property {
        uuid: NativeUuid,
        index: usize,
        property: Property,
    },
}

/// Read only requested owner texts, never a whole project or a potentially newer native cache.
fn metadata_location(
    file: &MappedFile,
    root_uuid: &NativeUuid,
    accepted: &InputFile,
    working: Option<&InputFile>,
    native: &[u8],
    range: &Range<usize>,
    texts: OriginTexts<'_>,
) -> Result<SourceLocation, Error> {
    verify_bytes(texts.accepted.as_bytes(), accepted.bytes, &accepted.sha256)?;
    let (accepted_value, accepted_tree) = yaml::decode_with_tree(texts.accepted)?;
    let owner = ObjectSource::from_value(accepted_value)?;
    let accepted_object = std::iter::once(&owner.document)
        .chain(&owner.nested)
        .find(|part| part.descriptor.object.uuid.key() == root_uuid.key())
        .ok_or_else(|| Error::root("source_map_origin_mismatch"))?;
    let accepted_path = &accepted_object.path;
    let accepted_object = &accepted_object.descriptor.object;
    let text =
        std::str::from_utf8(native).map_err(|_| Error::root("source_map_native_text_invalid"))?;
    if !text.is_char_boundary(range.start) || !text.is_char_boundary(range.end) {
        return Err(Error::root("source_map_span_invalid"));
    }
    crate::project_core::native::check_envelope(text)
        .map_err(|_| Error::root("source_map_native_text_invalid"))?;
    let document = roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            nodes_limit: 1_000_000,
            ..Default::default()
        },
    )
    .map_err(|_| Error::root("source_map_native_text_invalid"))?;
    let envelope = document.root_element();
    let root = envelope
        .children()
        .find(Node::is_element)
        .ok_or_else(|| Error::root("source_map_native_text_invalid"))?;
    if !metadata_root_valid(envelope, root, accepted_object.kind, root_uuid) {
        return Err(Error::root("source_map_origin_mismatch"));
    }
    let mut result = SourceLocation {
        authority: InputAuthority::Accepted,
        input: accepted.clone(),
        range: None,
        uuid: Some(root_uuid.clone()),
    };
    if !root.range().contains(&range.start) {
        return Ok(result);
    }
    let hit = native_hit(root, text, range.start)?;
    let (uuid, property) = match hit {
        Hit::Object(uuid) => (uuid, None),
        Hit::Property {
            uuid,
            index,
            property,
        } => (uuid, Some((index, property))),
    };
    let Some(location) = document_paths::locate(&owner, accepted_path, &uuid) else {
        return created_location(file, text, &uuid, property, working, texts.working);
    };
    result.uuid = Some(uuid.clone());
    result.range = Some(yaml::range_at(&accepted_tree, &location.steps)?);
    if let Some((index, property)) = property {
        let (original, steps) = accepted_property(&location, index)?;
        if let (
            Property::Typed { name, value },
            Property::Typed {
                name: original_name,
                value: original_value,
            },
        ) = (&property, original)
        {
            if name != original_name {
                return Err(Error::root("source_map_origin_mismatch"));
            }
            if value != original_value {
                return working_location(file, &uuid, *name, value, working, texts.working);
            }
        }
        if matches!((&property, original), (Property::Preserved { fragment: selected }, Property::Preserved { fragment: before }) if selected.xml != before.xml)
            && matches!(
                file.origin,
                FileOrigin::Metadata {
                    uses_working_values: true,
                    ..
                }
            )
        {
            return working_preserved_location(
                file,
                &uuid,
                index,
                &property,
                working,
                texts.working,
            );
        }
        result.range = Some(property_range(
            &accepted_tree,
            &steps,
            original,
            location.schema,
        )?);
    }
    Ok(result)
}

/// The actual native envelope and UUID must match the independently resolved accepted class.
fn metadata_root_valid(
    envelope: Node<'_, '_>,
    root: Node<'_, '_>,
    kind: MetadataKind,
    uuid: &NativeUuid,
) -> bool {
    envelope.has_tag_name((MD_NAMESPACE, "MetaDataObject"))
        && envelope.attribute("version") == Some("2.20")
        && root.has_tag_name((MD_NAMESPACE, kind.designer_tag()))
        && root
            .attribute("uuid")
            .is_some_and(|value| value.eq_ignore_ascii_case(uuid.as_str()))
}

/// A renamed reference retains its opaque slot but originates in exact current YAML bytes.
fn working_preserved_location(
    file: &MappedFile,
    uuid: &NativeUuid,
    index: usize,
    selected: &Property,
    input: Option<&InputFile>,
    text: Option<&str>,
) -> Result<SourceLocation, Error> {
    let input = input.ok_or_else(|| Error::root("source_map_origin_mismatch"))?;
    let text = text.ok_or_else(|| Error::root("source_map_origin_text_missing"))?;
    verify_bytes(text.as_bytes(), input.bytes, &input.sha256)?;
    let (value, tree) = yaml::decode_with_tree(text)?;
    let owner = metadata_owner(value)?;
    let location = document_paths::locate(&owner, &file.path, uuid)
        .ok_or_else(|| Error::root("source_map_origin_mismatch"))?;
    let (property, steps) = accepted_property(&location, index)?;
    if property != selected {
        return Err(Error::root("source_map_origin_mismatch"));
    }
    Ok(SourceLocation {
        authority: InputAuthority::Working,
        input: input.clone(),
        uuid: Some(uuid.clone()),
        range: Some(property_range(&tree, &steps, property, location.schema)?),
    })
}

/// A fresh inline node belongs entirely to verified working bytes and must match the selected native node.
fn created_location(
    file: &MappedFile,
    native: &str,
    uuid: &NativeUuid,
    property: Option<(usize, Property)>,
    input: Option<&InputFile>,
    text: Option<&str>,
) -> Result<SourceLocation, Error> {
    if !matches!(
        file.origin,
        FileOrigin::Metadata {
            uses_working_values: true,
            ..
        }
    ) {
        return Err(Error::root("source_map_origin_mismatch"));
    }
    let input = input.ok_or_else(|| Error::root("source_map_origin_mismatch"))?;
    let text = text.ok_or_else(|| Error::root("source_map_origin_text_missing"))?;
    verify_bytes(text.as_bytes(), input.bytes, &input.sha256)?;
    let (value, tree) = yaml::decode_with_tree(text)?;
    let working = metadata_owner(value)?;
    let selected = ObjectSource {
        form: None,
        document: super::super::super::NativeDocument {
            path: file.path.clone(),
            descriptor: super::super::super::Descriptor::from_xml(native)?,
        },
        nested: Vec::new(),
    };
    let Some((kind, context)) =
        super::super::super::child_creation::created_context(&selected.document.descriptor, uuid)?
    else {
        return Err(Error::root("source_map_origin_mismatch"));
    };
    let location = document_paths::locate(&working, &file.path, uuid)
        .ok_or_else(|| Error::root("source_map_origin_mismatch"))?;
    let selected_location = document_paths::locate(&selected, &file.path, uuid)
        .ok_or_else(|| Error::root("source_map_origin_mismatch"))?;
    if location.node.kind != kind || location.node != selected_location.node {
        return Err(Error::root("source_map_origin_mismatch"));
    }
    super::super::super::child_creation::validate_new_node(location.node)?;
    super::super::super::child_creation::validate_new_types(location.node, &context)?;
    let mut steps = location.steps.clone();
    let range = if let Some((index, property)) = property {
        let section = location
            .node
            .sections
            .iter()
            .position(|section| matches!(section, Section::Properties(_)))
            .ok_or_else(|| Error::root("source_map_origin_mismatch"))?;
        steps.extend(document_paths::property_steps(
            location.schema,
            section,
            index,
            &property,
        ));
        property_range(&tree, &steps, &property, location.schema)?
    } else {
        yaml::range_at(&tree, &steps)?
    };
    Ok(SourceLocation {
        authority: InputAuthority::Working,
        input: input.clone(),
        range: Some(range),
        uuid: Some(uuid.clone()),
    })
}

/// Descend only real metadata children; UUID-like text inside opaque XML is never an identity.
fn native_hit(node: Node<'_, '_>, text: &str, offset: usize) -> Result<Hit, Error> {
    let uuid = NativeUuid::parse(
        node.attribute("uuid")
            .ok_or_else(|| Error::root("source_map_origin_mismatch"))?,
    )?;
    for section in node.children().filter(Node::is_element) {
        if section.has_tag_name((MD_NAMESPACE, "Properties")) {
            for (index, property) in section
                .children()
                .filter(|node| !node.is_text())
                .enumerate()
            {
                if property.range().contains(&offset) {
                    return Ok(Hit::Property {
                        uuid,
                        index,
                        property: native_property(property, node, text)?,
                    });
                }
            }
        } else if section.has_tag_name((MD_NAMESPACE, "InternalInfo")) {
            if let Some(hit) = generated_table_name(node, section, text, offset, &uuid)? {
                return Ok(hit);
            }
        } else if section.has_tag_name((MD_NAMESPACE, "ChildObjects")) {
            for child in section.children().filter(Node::is_element) {
                if child.tag_name().namespace() == Some(MD_NAMESPACE)
                    && child.attribute("uuid").is_some()
                    && child.range().contains(&offset)
                {
                    return native_hit(child, text, offset);
                }
            }
        }
    }
    Ok(Hit::Object(uuid))
}

/// A changed generated declaration name follows its sole editable table Name; service UUIDs keep their accepted origin.
fn generated_table_name(
    node: Node<'_, '_>,
    section: Node<'_, '_>,
    text: &str,
    offset: usize,
    uuid: &NativeUuid,
) -> Result<Option<Hit>, Error> {
    let context: Vec<_> = section
        .namespaces()
        .map(|binding| Namespace {
            prefix: binding.name().map(str::to_owned),
            uri: binding.uri().to_owned(),
        })
        .collect();
    if tabular_identities::decode(section, text, &context).is_none()
        || !section
            .children()
            .filter(Node::is_element)
            .any(|declaration| {
                declaration.attributes().any(|attribute| {
                    attribute.name() == "name" && attribute.range_value().contains(&offset)
                })
            })
    {
        return Ok(None);
    }
    let Some(properties) = node
        .children()
        .find(|child| child.has_tag_name((MD_NAMESPACE, "Properties")))
    else {
        return Ok(None);
    };
    let Some((index, property)) = properties
        .children()
        .filter(|node| !node.is_text())
        .enumerate()
        .find(|(_, property)| property.has_tag_name((MD_NAMESPACE, "Name")))
    else {
        return Ok(None);
    };
    Ok(Some(Hit::Property {
        uuid: uuid.clone(),
        index,
        property: native_property(property, node, text)?,
    }))
}

/// Native properties retain their actual parent scope and class for either direct or derived source locations.
fn native_property(
    property: Node<'_, '_>,
    owner: Node<'_, '_>,
    text: &str,
) -> Result<Property, Error> {
    let context: Vec<_> = property
        .parent_element()
        .unwrap_or(property)
        .namespaces()
        .map(|binding| Namespace {
            prefix: binding.name().map(str::to_owned),
            uri: binding.uri().to_owned(),
        })
        .collect();
    let kind = MetadataKind::from_xml_tag(owner.tag_name().name())
        .map_err(|_| Error::root("source_map_origin_mismatch"))?;
    Ok(xml::property(property, text, &context, kind))
}

/// Reordered working sections/properties are addressed by their actual fields, not old indices.
fn working_location(
    file: &MappedFile,
    uuid: &NativeUuid,
    name: PropertyName,
    selected: &PropertyValue,
    input: Option<&InputFile>,
    text: Option<&str>,
) -> Result<SourceLocation, Error> {
    if !matches!(
        file.origin,
        FileOrigin::Metadata {
            uses_working_values: true,
            ..
        }
    ) {
        return Err(Error::root("source_map_origin_mismatch"));
    }
    let input = input.ok_or_else(|| Error::root("source_map_origin_mismatch"))?;
    let text = text.ok_or_else(|| Error::root("source_map_origin_text_missing"))?;
    verify_bytes(text.as_bytes(), input.bytes, &input.sha256)?;
    let (value, tree) = yaml::decode_with_tree(text)?;
    let mut result = SourceLocation {
        authority: InputAuthority::Working,
        input: input.clone(),
        range: None,
        uuid: Some(uuid.clone()),
    };
    let owner = metadata_owner(value)?;
    if let Some(location) = document_paths::locate(&owner, &file.path, uuid) {
        for (section, contents) in location.node.sections.iter().enumerate() {
            if let Section::Properties(items) = contents {
                for (index, property) in items.iter().enumerate() {
                    if let Property::Typed {
                        name: candidate_name,
                        value,
                    } = property
                        && *candidate_name == name
                    {
                        if value != selected {
                            return Err(Error::root("source_map_origin_mismatch"));
                        }
                        let mut steps = location.steps.clone();
                        steps.extend(document_paths::property_steps(
                            location.schema,
                            section,
                            index,
                            property,
                        ));
                        result.range =
                            Some(property_range(&tree, &steps, property, location.schema)?);
                        return Ok(result);
                    }
                }
            }
        }
    }
    Ok(result)
}

/// The generated native ordinal resolves against accepted slots, independent of YAML key order.
fn accepted_property<'a>(
    location: &document_paths::NodeLocation<'a>,
    index: usize,
) -> Result<(&'a Property, Vec<Step>), Error> {
    for (section, contents) in location.node.sections.iter().enumerate() {
        if let Section::Properties(items) = contents {
            let property = items
                .get(index)
                .ok_or_else(|| Error::root("source_map_origin_mismatch"))?;
            let mut steps = location.steps.clone();
            steps.extend(document_paths::property_steps(
                location.schema,
                section,
                index,
                property,
            ));
            return Ok((property, steps));
        }
    }
    Err(Error::root("source_map_origin_mismatch"))
}

/// Named typed values use their own scalar/collection range; opaque native XML keeps its slot.
fn property_range(
    tree: &YamlFile,
    steps: &[Step],
    property: &Property,
    schema: u32,
) -> Result<Range<usize>, Error> {
    let mut steps = steps.to_vec();
    match property {
        Property::Preserved { .. } => steps.extend([Step::Key("fragment"), Step::Key("xml")]),
        Property::Typed { .. }
            if schema == crate::project_core::source_format::READABLE_SCHEMA_VERSION => {}
        Property::Typed {
            value: PropertyValue::TypeDescription(_),
            ..
        } => steps.extend([Step::Key("value"), Step::Key("value"), Step::Key("data")]),
        Property::Typed {
            value: PropertyValue::ReferenceList(_),
            ..
        } => steps.extend([Step::Key("value"), Step::Key("value"), Step::Key("items")]),
        Property::Typed {
            value: PropertyValue::EnumFillValue(_),
            ..
        } => steps.extend([
            Step::Key("value"),
            Step::Key("value"),
            Step::Key("reference"),
        ]),
        Property::Typed {
            value:
                PropertyValue::Reference(_)
                | PropertyValue::Text(_)
                | PropertyValue::Boolean(_)
                | PropertyValue::Integer(_)
                | PropertyValue::Enumeration(_),
            ..
        } => steps.extend([Step::Key("value"), Step::Key("value")]),
        Property::Typed { .. } => steps.push(Step::Key("value")),
    }
    yaml::range_at(tree, &steps)
}

/// A metadata coordinate has no dialog authority; rejected body edits cannot hide selected metadata.
fn metadata_owner(mut value: serde_json::Value) -> Result<ObjectSource, Error> {
    if let Some(object) = value.as_object_mut() {
        object.remove("form");
    }
    ObjectSource::from_value(value)
}

/// Verification happens before decoding, so coordinates cannot silently follow a newer file.
pub(super) fn verify_bytes(bytes: &[u8], size: u64, checksum: &str) -> Result<(), Error> {
    if u64::try_from(bytes.len()).ok() != Some(size)
        || files::hex(&Sha256::digest(bytes)) != checksum
    {
        return Err(Error::root("source_map_input_changed"));
    }
    Ok(())
}
