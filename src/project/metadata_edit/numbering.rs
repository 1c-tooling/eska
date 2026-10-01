//! A document's shared numbering settings are copied only from a verified numerator descriptor.

use roxmltree::{Document, Node};

use super::{EditError, EditPlan, FieldStep, ScalarSchema, patch, schema};
use crate::project::{
    metadata_model::{MetadataKind, ObjectId},
    metadata_parser::{self, PropertiesMode},
};

pub const FIELDS: [&str; 5] = [
    "NumberType",
    "NumberLength",
    "NumberAllowedLength",
    "NumberPeriodicity",
    "CheckUnique",
];

/// This domain contains only the five parameters shared by documents and their numerator.
pub struct Numbering {
    values: Vec<(&'static str, String)>,
}

impl Numbering {
    /// Unknown or incomplete writer shapes cannot invent configuration defaults or missing fields.
    pub fn read(input: &str, modern: bool) -> Result<Self, EditError> {
        metadata_parser::check_envelope(input).map_err(|_| EditError::InvalidXml)?;
        let document = Document::parse(input).map_err(|_| EditError::InvalidXml)?;
        let properties = properties(&document, "DocumentNumerator")?;
        let values = FIELDS
            .into_iter()
            .map(|name| {
                let node = field(properties, name)?;
                let value = scalar_text(node)?;
                let model = schema::field_type("DocumentNumerator", node, true, modern)
                    .ok_or(EditError::UnsupportedValue)?;
                schema::schema(model, modern)
                    .ok_or(EditError::UnsupportedValue)?
                    .validate(value)?;
                Ok((name, value.to_owned()))
            })
            .collect::<Result<Vec<_>, EditError>>()?;
        let result = Self { values };
        let max = if result.value("NumberType") == Some("Number") {
            38
        } else {
            50
        };
        ScalarSchema::Integer { min: 0, max }.validate(
            result
                .value("NumberLength")
                .ok_or(EditError::UnsupportedValue)?,
        )?;
        Ok(result)
    }

    /// Change existing document fields together, then validate standard values against the final type.
    pub fn synchronize(
        &self,
        input: &str,
        id: &ObjectId,
        numerator: Option<&str>,
        property: Option<&str>,
    ) -> Result<EditPlan, EditError> {
        if property.is_some_and(|name| !FIELDS.contains(&name)) {
            return Err(EditError::UnsupportedValue);
        }
        let parsed = metadata_parser::parse(input, None, PropertiesMode::Summary)
            .map_err(|_| EditError::InvalidXml)?;
        if !parsed.objects.first().is_some_and(|object| {
            object.metadata.id() == id && object.metadata.kind() == MetadataKind::Document
        }) {
            return Err(EditError::UnsupportedValue);
        }
        let document = Document::parse(input).map_err(|_| EditError::InvalidXml)?;
        let properties = properties(&document, "Document")?;
        let mut replacements = Vec::new();
        let mut changed = Vec::new();
        for (name, value) in self
            .values
            .iter()
            .filter(|(name, _)| property.is_none_or(|selected| selected == *name))
            .map(|(name, value)| (*name, value.as_str()))
            .chain(numerator.map(|value| ("Numerator", value)))
        {
            let node = field(properties, name)?;
            let plan = patch::text_plan(input, node.range(), value)?;
            if !plan.is_empty() {
                changed.push(node);
                replacements.extend_from_slice(plan.replacements());
            }
        }
        let plan = patch::replacements_plan(input, replacements)?;
        for node in changed {
            super::document::validate_dependencies(node, plan.output())?;
        }
        Ok(plan)
    }

    /// Field names are a closed backend vocabulary, independent of localized captions.
    fn value(&self, name: &str) -> Option<&str> {
        self.values
            .iter()
            .find_map(|(key, value)| (*key == name).then_some(value.as_str()))
    }
}

/// Only direct document/numerator parameters participate in related-file edits.
pub fn linked_field(kind: MetadataKind, path: &[FieldStep]) -> bool {
    path.len() == 1
        && path[0].occurrence == 0
        && path[0].key.namespace.as_deref() == Some(schema::MD)
        && match kind {
            MetadataKind::Document => path[0].key.name == "Numerator",
            MetadataKind::DocumentNumerator => FIELDS.contains(&path[0].key.name.as_str()),
            _ => false,
        }
}

/// A missing numerator is unassigned; malformed nested values are never treated as empty.
pub fn document_numerator(input: &str) -> Result<Option<String>, EditError> {
    metadata_parser::check_envelope(input).map_err(|_| EditError::InvalidXml)?;
    let document = Document::parse(input).map_err(|_| EditError::InvalidXml)?;
    let properties = properties(&document, "Document")?;
    let mut nodes = properties
        .children()
        .filter(|node| node.has_tag_name((schema::MD, "Numerator")));
    let Some(node) = nodes.next() else {
        return Ok(None);
    };
    if nodes.next().is_some() {
        return Err(EditError::UnsupportedValue);
    }
    let value = scalar_text(node)?;
    Ok((!value.trim().is_empty()).then(|| value.to_owned()))
}

/// Related writers require the actual Designer namespace and one direct metadata owner.
fn properties<'a>(document: &'a Document<'_>, tag: &str) -> Result<Node<'a, 'a>, EditError> {
    let root = document.root_element();
    if !root.has_tag_name((schema::MD, "MetaDataObject")) {
        return Err(EditError::InvalidXml);
    }
    let mut children = root.children().filter(Node::is_element);
    let owner = children.next().ok_or(EditError::InvalidXml)?;
    if !owner.has_tag_name((schema::MD, tag)) || children.next().is_some() {
        return Err(EditError::UnsupportedValue);
    }
    field(owner, "Properties")
}

/// Duplicate properties are ambiguous even when they contain the same text.
fn field<'a>(properties: Node<'a, 'a>, name: &str) -> Result<Node<'a, 'a>, EditError> {
    let mut nodes = properties
        .children()
        .filter(|node| node.has_tag_name((schema::MD, name)));
    let result = nodes.next().ok_or(EditError::UnsupportedValue)?;
    if nodes.next().is_some() {
        return Err(EditError::UnsupportedValue);
    }
    Ok(result)
}

/// A nested child or split text is not a scalar metadata parameter.
fn scalar_text<'a>(node: Node<'a, 'a>) -> Result<&'a str, EditError> {
    if node
        .children()
        .any(|node| node.is_element() || node.is_pi())
        || node.children().filter(Node::is_text).count() > 1
    {
        return Err(EditError::UnsupportedValue);
    }
    Ok(node.text().unwrap_or_default())
}
