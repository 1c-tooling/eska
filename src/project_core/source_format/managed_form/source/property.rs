//! Only canonical observed leaves/localized values leave opaque XML; their inverse slots remain pinned.

use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};

use roxmltree::{Document, Node};
use serde::{Deserialize, Serialize};

use super::{Error, ManagedFormSource, Part, decode};
use crate::project_core::source_format::native_writer;

#[cfg(test)]
mod tests;

const V8: &str = "http://v8.1c.ru/8.1/data/core";

/// The first writable dialog profile uses observed localized titles/tooltips and boolean leaves.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum FormPropertyName {
    Title,
    ToolTip,
    Visible,
    ReadOnly,
    Enabled,
}

impl FormPropertyName {
    /// Foreign properties cannot acquire native semantics from the same local tag spelling.
    fn from_node(node: Node<'_, '_>) -> Option<Self> {
        if node.tag_name().namespace() != Some(super::super::NAMESPACE) {
            return None;
        }
        Some(match node.tag_name().name() {
            "Title" => Self::Title,
            "ToolTip" => Self::ToolTip,
            "Visible" => Self::Visible,
            "ReadOnly" => Self::ReadOnly,
            "Enabled" => Self::Enabled,
            _ => return None,
        })
    }
}

/// Language keys identify pinned native items; values are the sole editable strings.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormLocalizedText {
    pub text: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formatted: Option<bool>,
}

/// Value kind follows the native property slot, without implicit coercions or defaults.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FormPropertyValue {
    Bool(bool),
    Localized(FormLocalizedText),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PropertyStencil {
    parts: Vec<PropertyPart>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum PropertyPart {
    Xml { value: String },
    Boolean,
    Formatted,
    Text { language: String, frame: TextFrame },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TextFrame {
    open: String,
    close: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    empty: Option<String>,
}

/// Promote only unique direct properties whose exact original XML is reproduced by the typed inverse.
pub(super) fn decode(
    node: Node<'_, '_>,
    input: &str,
) -> Option<(FormPropertyName, FormPropertyValue, PropertyStencil)> {
    let name = FormPropertyName::from_node(node)?;
    if node
        .parent_element()?
        .children()
        .filter(|child| FormPropertyName::from_node(*child) == Some(name))
        .count()
        != 1
    {
        return None;
    }
    let value = read_value(node, name)?;
    let mut holes = Vec::new();
    match &value {
        FormPropertyValue::Bool(_) => {
            let text = node.children().next()?;
            holes.push((text.range(), PropertyPart::Boolean));
        }
        FormPropertyValue::Localized(localized) => {
            if localized.formatted.is_some() {
                let attribute = node.attributes().find(|attribute| {
                    attribute.namespace().is_none() && attribute.name() == "formatted"
                })?;
                holes.push((attribute.range_value(), PropertyPart::Formatted));
            }
            for item in node.children().filter(Node::is_element) {
                let language = item
                    .children()
                    .find(|child| child.has_tag_name((V8, "lang")))?;
                let content = item
                    .children()
                    .find(|child| child.has_tag_name((V8, "content")))?;
                let frame = text_frame(content, input)?;
                holes.push((
                    content.range(),
                    PropertyPart::Text {
                        language: text(language),
                        frame,
                    },
                ));
            }
        }
    }
    let stencil = PropertyStencil {
        parts: split(input, node.range(), holes)?,
    };
    if render(&stencil, &value).ok()?.as_str() != input.get(node.range())? {
        return None;
    }
    Some((name, value, stencil))
}

/// Semantic output checks read the expected native property, without a second stored value or a raw fallback.
fn read_value(node: Node<'_, '_>, name: FormPropertyName) -> Option<FormPropertyValue> {
    if matches!(
        name,
        FormPropertyName::Visible | FormPropertyName::ReadOnly | FormPropertyName::Enabled
    ) {
        if node.attributes().len() != 0 || !node.children().all(|child| child.is_text()) {
            return None;
        }
        return boolean(&text(node)).map(FormPropertyValue::Bool);
    }
    let formatted = if let Some(value) = node.attribute("formatted") {
        Some(boolean(value)?)
    } else {
        None
    };
    if node.attributes().len() != usize::from(formatted.is_some())
        || (name == FormPropertyName::ToolTip && formatted.is_some())
    {
        return None;
    }
    let mut values = BTreeMap::new();
    for item in meaningful(node) {
        if !item.has_tag_name((V8, "item")) || item.attributes().len() != 0 {
            return None;
        }
        let parts: Vec<_> = meaningful(item).collect();
        if parts.len() != 2 {
            return None;
        }
        let language = parts.iter().find(|part| part.has_tag_name((V8, "lang")))?;
        let content = parts
            .iter()
            .find(|part| part.has_tag_name((V8, "content")))?;
        if [language, content].iter().any(|part| {
            part.attributes().len() != 0 || !part.children().all(|child| child.is_text())
        }) {
            return None;
        }
        let language = text(*language);
        if language.is_empty() || values.insert(language, text(*content)).is_some() {
            return None;
        }
    }
    Some(FormPropertyValue::Localized(FormLocalizedText {
        text: values,
        formatted,
    }))
}

/// Whitespace separates localized items; comments, PIs and meaningful text remain outside the promoted profile.
fn meaningful<'a, 'input>(node: Node<'a, 'input>) -> impl Iterator<Item = Node<'a, 'input>> {
    node.children().filter(|child| {
        !child.is_text() || child.text().is_some_and(|text| !text.trim().is_empty())
    })
}

/// XML text concatenation respects parsed entities and line endings; lexical promotion is checked separately.
fn text(node: Node<'_, '_>) -> String {
    node.children()
        .filter(Node::is_text)
        .filter_map(|node| node.text())
        .collect()
}

/// Only the observed boolean spellings belong to the accepted inverse profile.
fn boolean(text: &str) -> Option<bool> {
    match text {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// Editable property names match their sole native slots; languages and formatted-flag presence are pinned.
pub(super) fn validate_bindings(
    parts: &[Part],
    values: &BTreeMap<FormPropertyName, FormPropertyValue>,
) -> Result<(), Error> {
    let mut names = BTreeSet::new();
    for part in parts {
        if let Part::Property { name, stencil } = part {
            if !names.insert(name) {
                return Err(failure());
            }
            let value = values.get(name).ok_or_else(failure)?;
            validate_value(stencil, value)?;
        }
    }
    if names != values.keys().collect() {
        return Err(failure());
    }
    Ok(())
}

/// Native stencils cannot conceal a second editable scalar, a different property kind or unmatched item keys.
fn validate_value(stencil: &PropertyStencil, value: &FormPropertyValue) -> Result<(), Error> {
    let mut booleans = 0;
    let mut formatted = 0;
    let mut languages = BTreeSet::new();
    for part in &stencil.parts {
        match part {
            PropertyPart::Boolean => booleans += 1,
            PropertyPart::Formatted => formatted += 1,
            PropertyPart::Text { language, .. } => {
                if !languages.insert(language) {
                    return Err(failure());
                }
            }
            PropertyPart::Xml { .. } => {}
        }
    }
    let valid = match value {
        FormPropertyValue::Bool(_) => booleans == 1 && formatted == 0 && languages.is_empty(),
        FormPropertyValue::Localized(value) => {
            booleans == 0
                && formatted == usize::from(value.formatted.is_some())
                && languages == value.text.keys().collect()
        }
    };
    if valid { Ok(()) } else { Err(failure()) }
}

/// Serialization uses only current typed values; unknown XML is retained exactly in surrounding fragments.
pub(super) fn render(
    stencil: &PropertyStencil,
    value: &FormPropertyValue,
) -> Result<String, Error> {
    validate_value(stencil, value)?;
    let mut output = String::new();
    for part in &stencil.parts {
        match (part, value) {
            (PropertyPart::Xml { value, .. }, _) => output.push_str(value),
            (PropertyPart::Boolean, FormPropertyValue::Bool(value)) => {
                output.push_str(if *value { "true" } else { "false" });
            }
            (PropertyPart::Formatted, FormPropertyValue::Localized(value)) => {
                output.push_str(if value.formatted.ok_or_else(failure)? {
                    "true"
                } else {
                    "false"
                });
            }
            (PropertyPart::Text { language, frame }, FormPropertyValue::Localized(value)) => {
                let text = value.text.get(language).ok_or_else(failure)?;
                if text.is_empty()
                    && let Some(empty) = &frame.empty
                {
                    output.push_str(empty);
                    continue;
                }
                output.push_str(&frame.open);
                output.push_str(&native_writer::escape(text));
                output.push_str(&frame.close);
            }
            _ => return Err(failure()),
        }
    }
    Ok(output)
}

/// Empty native content can expand without adding a new language/item slot or changing its envelope.
fn text_frame(node: Node<'_, '_>, input: &str) -> Option<TextFrame> {
    let range = node.range();
    let value = input.get(range.clone())?;
    let (qname, _) = decode::opening_name(input, range.start).ok()?;
    if let Some(prefix) = value.strip_suffix("/>") {
        return Some(TextFrame {
            open: format!("{prefix}>"),
            close: format!("</{qname}>"),
            empty: Some(value.into()),
        });
    }
    let start = decode::opening_end(input, range.clone()).ok()?;
    let end = range.start + value.rfind("</")?;
    Some(TextFrame {
        open: input.get(range.start..start)?.into(),
        close: input.get(end..range.end)?.into(),
        empty: None,
    })
}

/// Ordered nonoverlapping UTF-8 ranges remove promoted scalar contents from all opaque fragments.
fn split(
    input: &str,
    range: Range<usize>,
    mut holes: Vec<(Range<usize>, PropertyPart)>,
) -> Option<Vec<PropertyPart>> {
    holes.sort_by_key(|(range, _)| range.start);
    let mut cursor = range.start;
    let mut parts = Vec::new();
    for (hole, part) in holes {
        if hole.start < cursor || hole.end > range.end {
            return None;
        }
        if cursor < hole.start {
            parts.push(PropertyPart::Xml {
                value: input.get(cursor..hole.start)?.into(),
            });
        }
        cursor = hole.end;
        parts.push(part);
    }
    if cursor < range.end {
        parts.push(PropertyPart::Xml {
            value: input.get(cursor..range.end)?.into(),
        });
    }
    Some(parts)
}

/// Rendered localized values and booleans agree with the sole authoring values in their exact node scope.
pub(super) fn check_output(source: &ManagedFormSource, xml: &str) -> Result<(), Error> {
    let document = Document::parse(xml).map_err(|_| failure())?;
    let root = document.root_element();
    check_node(root, &source.properties)?;
    for node in root.descendants().filter(Node::is_element) {
        if let Some(id) = decode::ui_id(node)? {
            check_node(
                node,
                &source.elements.get(&id).ok_or_else(failure)?.properties,
            )?;
        }
    }
    Ok(())
}

/// A promoted property is direct and unique; a coincident name in another scope has no authority.
fn check_node(
    node: Node<'_, '_>,
    values: &BTreeMap<FormPropertyName, FormPropertyValue>,
) -> Result<(), Error> {
    for (name, value) in values {
        let mut nodes = node
            .children()
            .filter(|node| FormPropertyName::from_node(*node) == Some(*name));
        let node = nodes.next().ok_or_else(failure)?;
        if nodes.next().is_some() || read_value(node, *name).as_ref() != Some(value) {
            return Err(failure());
        }
    }
    Ok(())
}

/// Property bindings never authorize unknown lexical/type/item transformations.
fn failure() -> Error {
    Error::root("source_form_property_binding_invalid")
}
