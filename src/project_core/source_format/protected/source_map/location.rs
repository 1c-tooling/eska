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
    PropertyValue, Section, document_paths, files, xml,
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
    /// Payload offsets remain unchanged. Metadata uses accepted composition and only selects
    /// working YAML for a typed value that differs from accepted bytes. A changed working
    /// identity can retain a selected value but has no inferred object coordinate.
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
        .find(|part| part.path == file.path)
        .map(|part| &part.descriptor.object)
        .ok_or_else(|| Error::root("source_map_origin_mismatch"))?;
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
    if !envelope.has_tag_name((MD_NAMESPACE, "MetaDataObject"))
        || envelope.attribute("version") != Some("2.20")
        || !root.has_tag_name((MD_NAMESPACE, accepted_object.kind.designer_tag()))
        || accepted_object.uuid.key() != root_uuid.key()
        || !root
            .attribute("uuid")
            .is_some_and(|uuid| uuid.eq_ignore_ascii_case(root_uuid.as_str()))
    {
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
    let location = document_paths::locate(&owner, &file.path, &uuid)
        .ok_or_else(|| Error::root("source_map_origin_mismatch"))?;
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
        result.range = Some(property_range(
            &accepted_tree,
            &steps,
            original,
            location.schema,
        )?);
    }
    Ok(result)
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
                    let context: Vec<_> = section
                        .namespaces()
                        .map(|binding| Namespace {
                            prefix: binding.name().map(str::to_owned),
                            uri: binding.uri().to_owned(),
                        })
                        .collect();
                    return Ok(Hit::Property {
                        uuid,
                        index,
                        property: xml::property(
                            property,
                            text,
                            &context,
                            MetadataKind::from_xml_tag(node.tag_name().name())
                                .map_err(|_| Error::root("source_map_origin_mismatch"))?,
                        ),
                    });
                }
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
    let owner = ObjectSource::from_value(value)?;
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

/// Verification happens before decoding, so coordinates cannot silently follow a newer file.
fn verify_bytes(bytes: &[u8], size: u64, checksum: &str) -> Result<(), Error> {
    if u64::try_from(bytes.len()).ok() != Some(size)
        || files::hex(&Sha256::digest(bytes)) != checksum
    {
        return Err(Error::root("source_map_input_changed"));
    }
    Ok(())
}
