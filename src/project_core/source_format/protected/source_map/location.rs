//! On-demand coordinates belong to captured input bytes; this mapper has no filesystem or IDE.

#[cfg(test)]
mod tests;

use std::ops::Range;

use roxmltree::Node;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use yaml_edit::YamlFile;

use super::{FileOrigin, InputAuthority, InputFile, MappedFile, validate_origin};
use crate::project_core::source_format::{
    Error, MD_NAMESPACE, Namespace, NativeUuid, ObjectSource, Property, PropertyName,
    PropertyValue, SourcePath, files, xml,
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
    let owner = ObjectSource::from_value(accepted_value.clone())?;
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
    let steps = object_steps(&accepted_value, &file.path, &uuid)?
        .ok_or_else(|| Error::root("source_map_origin_mismatch"))?;
    result.uuid = Some(uuid.clone());
    result.range = Some(yaml::range_at(&accepted_tree, &steps)?);
    if let Some((index, property)) = property {
        let steps = accepted_property_steps(&accepted_value, &steps, index)?;
        let original: Property = serde_json::from_value(value_at(&accepted_value, &steps)?.clone())
            .map_err(|_| Error::root("source_map_origin_mismatch"))?;
        if let (
            Property::Typed { name, value },
            Property::Typed {
                name: original_name,
                value: original_value,
            },
        ) = (&property, &original)
        {
            if name != original_name {
                return Err(Error::root("source_map_origin_mismatch"));
            }
            if value != original_value {
                return working_location(file, &uuid, *name, value, working, texts.working);
            }
        }
        result.range = Some(property_range(&accepted_tree, &steps, &original)?);
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
                        property: xml::property(property, text, &context),
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
    if let Some(node_steps) = object_steps(&value, &file.path, uuid)? {
        let node = value_at(&value, &node_steps)?;
        let mut matching = Vec::new();
        for (section, values) in sections(node, "properties") {
            for (index, property) in values.iter().enumerate() {
                if property.get("kind").and_then(Value::as_str) == Some("typed")
                    && property.get("name").and_then(Value::as_str) == Some(name.as_str())
                {
                    let mut steps = node_steps.clone();
                    steps.extend(property_steps(section, index));
                    matching.push(steps);
                }
            }
        }
        if matching.len() != 1 {
            return Ok(result);
        }
        let steps = &matching[0];
        let candidate: PropertyValue = serde_json::from_value(
            value_at(&value, steps)?
                .get("value")
                .ok_or_else(|| Error::root("source_map_origin_mismatch"))?
                .clone(),
        )
        .map_err(|_| Error::root("source_map_origin_mismatch"))?;
        if &candidate != selected {
            return Err(Error::root("source_map_origin_mismatch"));
        }
        result.range = Some(property_range(
            &tree,
            steps,
            &Property::Typed {
                name,
                value: candidate,
            },
        )?);
    }
    Ok(result)
}

/// A descriptor path and native UUID select one modeled node in this exact owner family.
fn object_steps(
    value: &Value,
    path: &SourcePath,
    uuid: &NativeUuid,
) -> Result<Option<Vec<Step>>, Error> {
    let mut found = Vec::new();
    let parts = value
        .get("document")
        .map(|part| (part, vec![Step::Key("document")]));
    let nested = value
        .get("nested")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(index, part)| (part, vec![Step::Key("nested"), Step::Index(index)]));
    for (part, mut steps) in parts.into_iter().chain(nested) {
        if part.get("path").and_then(Value::as_str) == Some(path.as_str()) {
            steps.extend([Step::Key("descriptor"), Step::Key("object")]);
            collect_objects(value_at(value, &steps)?, uuid, &mut steps, &mut found);
        }
    }
    match found.len() {
        0 => Ok(None),
        1 => Ok(found.pop()),
        _ => Err(Error::root("source_map_location_ambiguous")),
    }
}

/// Only modeled child-object fields participate in native identity lookup.
fn collect_objects(
    node: &Value,
    uuid: &NativeUuid,
    steps: &mut Vec<Step>,
    found: &mut Vec<Vec<Step>>,
) {
    if node
        .get("uuid")
        .and_then(Value::as_str)
        .is_some_and(|value| value.eq_ignore_ascii_case(uuid.as_str()))
    {
        found.push(steps.clone());
    }
    for (section, values) in sections(node, "children") {
        for (index, child) in values.iter().enumerate() {
            if let Some(object) = child.get("object") {
                let previous = steps.len();
                steps.extend(property_steps(section, index));
                steps.push(Step::Key("object"));
                collect_objects(object, uuid, steps, found);
                steps.truncate(previous);
            }
        }
    }
}

/// Keep serialized section positions even when properties occur after a preserved slot.
fn sections<'a>(node: &'a Value, kind: &'a str) -> impl Iterator<Item = (usize, &'a Vec<Value>)> {
    node.get("sections")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
        .filter(move |(_, section)| section.get("kind").and_then(Value::as_str) == Some(kind))
        .filter_map(|(index, section)| {
            section
                .get("items")
                .and_then(Value::as_array)
                .map(|items| (index, items))
        })
}

/// Native property ordinals correspond to the accepted properties section, not YAML field order.
fn accepted_property_steps(value: &Value, node: &[Step], index: usize) -> Result<Vec<Step>, Error> {
    let mut sections = sections(value_at(value, node)?, "properties");
    let (section, values) = sections
        .next()
        .ok_or_else(|| Error::root("source_map_origin_mismatch"))?;
    if index >= values.len() || sections.next().is_some() {
        return Err(Error::root("source_map_origin_mismatch"));
    }
    let mut result = node.to_vec();
    result.extend(property_steps(section, index));
    Ok(result)
}

/// Share owner schema coordinates for properties and inline child positions.
const fn property_steps(section: usize, index: usize) -> [Step; 4] {
    [
        Step::Key("sections"),
        Step::Index(section),
        Step::Key("items"),
        Step::Index(index),
    ]
}

/// Typed values and opaque fragments keep one conservative YAML range; no XML column is copied.
fn property_range(
    tree: &YamlFile,
    steps: &[Step],
    property: &Property,
) -> Result<Range<usize>, Error> {
    let mut steps = steps.to_vec();
    match property {
        Property::Typed {
            value: PropertyValue::Text(_),
            ..
        } => steps.extend([Step::Key("value"), Step::Key("value")]),
        Property::Typed { .. } => steps.push(Step::Key("value")),
        Property::Preserved { .. } => steps.extend([Step::Key("fragment"), Step::Key("xml")]),
    }
    yaml::range_at(tree, &steps)
}

/// Schema navigation applies the same actual fields to semantic values and CST ranges.
fn value_at<'a>(value: &'a Value, steps: &[Step]) -> Result<&'a Value, Error> {
    steps.iter().try_fold(value, |value, step| {
        match step {
            Step::Key(key) => value.get(*key),
            Step::Index(index) => value.get(*index),
        }
        .ok_or_else(|| Error::root("source_map_origin_mismatch"))
    })
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
