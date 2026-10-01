//! Owner lengths constrain standard fields and data stored in the separate predefined payload.

use roxmltree::{Document, Node};

use super::{EditError, schema::MD};
use crate::project::metadata_model::PropertyKey;

const PREDEF: &str = "http://v8.1c.ru/8.3/xcf/predef";

/// EDT domains are narrowed by the actual Designer 8.3.27 import constraints.
pub(super) fn range(node: Node<'_, '_>) -> Option<(i64, i64)> {
    let properties = node
        .parent()
        .filter(|parent| parent.has_tag_name((MD, "Properties")))?;
    let owner = properties.parent()?;
    if node.tag_name().namespace() != Some(MD) || owner.tag_name().namespace() != Some(MD) {
        return None;
    }
    let max = match (owner.tag_name().name(), node.tag_name().name()) {
        ("Catalog" | "ChartOfCalculationTypes", "CodeLength") => {
            match text(properties, "CodeType")? {
                "String" if owner.has_tag_name((MD, "ChartOfCalculationTypes")) => 40,
                "String" => 50,
                "Number" => 38,
                _ => return None,
            }
        }
        ("Document" | "DocumentNumerator" | "Task" | "BusinessProcess", "NumberLength") => {
            match text(properties, "NumberType")? {
                "String" => 50,
                "Number" => 38,
                _ => return None,
            }
        }
        ("ChartOfAccounts" | "ChartOfCharacteristicTypes" | "ExchangePlan", "CodeLength") => 50,
        ("ChartOfAccounts", "DescriptionLength") => 628,
        ("ExchangePlan", "DescriptionLength") => 250,
        (
            "Catalog" | "ChartOfCalculationTypes" | "ChartOfCharacteristicTypes" | "Task",
            "DescriptionLength",
        ) => 150,
        _ => return None,
    };
    Some((0, max))
}

/// Designer disables document numbering parameters while a shared numerator is selected.
pub(super) fn inherited_from_numerator(node: Node<'_, '_>) -> bool {
    node.tag_name().namespace() == Some(MD)
        && matches!(
            node.tag_name().name(),
            "NumberLength"
                | "NumberType"
                | "NumberAllowedLength"
                | "NumberPeriodicity"
                | "CheckUnique"
        )
        && node.parent().is_some_and(|properties| {
            properties.has_tag_name((MD, "Properties"))
                && properties
                    .parent()
                    .is_some_and(|owner| owner.has_tag_name((MD, "Document")))
                && text(properties, "Numerator").is_some_and(|value| !value.trim().is_empty())
        })
}

/// A qualifier change never disables another existing property or rewrites its value implicitly.
pub(super) fn validate_dependents(
    node: Node<'_, '_>,
    candidate: &Document<'_>,
) -> Result<(), EditError> {
    let Some(properties) = node
        .parent()
        .filter(|parent| parent.has_tag_name((MD, "Properties")))
    else {
        return Ok(());
    };
    if node.tag_name().namespace() != Some(MD) {
        return Ok(());
    }
    let changed = node.tag_name().name();
    if !matches!(
        changed,
        "CodeLength"
            | "CodeType"
            | "NumberLength"
            | "NumberType"
            | "DescriptionLength"
            | "Autonumbering"
            | "CheckUnique"
            | "AutoOrderByCode"
    ) {
        return Ok(());
    }
    let properties = candidate
        .descendants()
        .find(|item| {
            item.has_tag_name((MD, "Properties")) && item.range().start == properties.range().start
        })
        .ok_or(EditError::InvalidXml)?;
    let owner = properties.parent().ok_or(EditError::InvalidXml)?;
    let class = owner.tag_name().name();
    if matches!(changed, "Autonumbering" | "CheckUnique" | "AutoOrderByCode")
        && !enabled(properties, changed)
    {
        return Ok(());
    }
    if !matches!(
        class,
        "Catalog"
            | "ChartOfCalculationTypes"
            | "ChartOfCharacteristicTypes"
            | "ChartOfAccounts"
            | "ExchangePlan"
            | "Task"
            | "Document"
            | "DocumentNumerator"
            | "BusinessProcess"
    ) {
        return Ok(());
    }
    let length_name = if changed == "DescriptionLength" {
        "DescriptionLength"
    } else if matches!(
        class,
        "Document" | "DocumentNumerator" | "Task" | "BusinessProcess"
    ) {
        "NumberLength"
    } else {
        "CodeLength"
    };
    let Some(length) = child(properties, length_name) else {
        return Ok(());
    };
    let value = length
        .text()
        .and_then(|text| text.parse::<i64>().ok())
        .ok_or_else(|| incompatible(length_name))?;
    if let Some((min, max)) = range(length)
        && !(min..=max).contains(&value)
    {
        return Err(incompatible(length_name));
    }
    if value == 0 {
        validate_disabled_field(properties, class, changed, length_name)?;
    }
    if class == "ChartOfAccounts"
        && length_name == "CodeLength"
        && enabled(properties, "AutoOrderByCode")
        && text(properties, "OrderLength")
            .and_then(|text| text.parse::<i64>().ok())
            .is_none_or(|order| order < value)
    {
        return Err(incompatible("OrderLength"));
    }
    Ok(())
}

/// A disabled standard field cannot remain an autocomplete key or an enabled numbering source.
fn validate_disabled_field(
    properties: Node<'_, '_>,
    class: &str,
    changed: &str,
    length_name: &str,
) -> Result<(), EditError> {
    if class == "DocumentNumerator" {
        return Ok(());
    }
    let field = match length_name {
        "CodeLength" => "Code",
        "NumberLength" => "Number",
        _ => "Description",
    };
    if (class == "Catalog" && field == "Code") || (class == "Document" && field == "Number") {
        for flag in ["Autonumbering", "CheckUnique"] {
            if enabled(properties, flag) {
                return Err(incompatible(if changed == flag {
                    length_name
                } else {
                    flag
                }));
            }
        }
    }
    if let Some(input) = child(properties, "InputByString") {
        let name = text(properties, "Name").ok_or(EditError::UnsupportedValue)?;
        let reference = format!("{class}.{name}.StandardAttribute.{field}").to_lowercase();
        if input.children().filter(Node::is_element).any(|node| {
            node.text()
                .is_some_and(|value| value.to_lowercase() == reference)
        }) {
            return Err(incompatible("InputByString"));
        }
    } else if class != "ExchangePlan" {
        // Designer supplies standard-field defaults when the list element is absent.
        return Err(incompatible("InputByString"));
    }
    Ok(())
}

/// A changed owner qualifier requires rechecking the current predefined file, including on undo.
pub struct PredefinedLengths {
    code: Option<(usize, bool)>,
    description: Option<usize>,
}

impl PredefinedLengths {
    /// Compare direct owner properties only; inline attributes never change the owner's domain.
    pub(crate) fn changed(before: &str, after: &str) -> Result<Option<Self>, EditError> {
        let before = Document::parse(before).map_err(|_| EditError::InvalidXml)?;
        let after = Document::parse(after).map_err(|_| EditError::InvalidXml)?;
        let Some(old) = owner_properties(&before) else {
            return Ok(None);
        };
        let Some(new) = owner_properties(&after) else {
            return Ok(None);
        };
        let class = new.parent().ok_or(EditError::InvalidXml)?.tag_name().name();
        if !matches!(
            class,
            "Catalog"
                | "ChartOfAccounts"
                | "ChartOfCalculationTypes"
                | "ChartOfCharacteristicTypes"
        ) {
            return Ok(None);
        }
        let code_changed = ["CodeLength", "CodeType"]
            .iter()
            .any(|name| text(old, name) != text(new, name));
        let description_changed = text(old, "DescriptionLength") != text(new, "DescriptionLength");
        if !code_changed && !description_changed {
            return Ok(None);
        }
        let code = if code_changed {
            let number = if matches!(class, "Catalog" | "ChartOfCalculationTypes") {
                match text(new, "CodeType") {
                    Some("Number") => true,
                    Some("String") => false,
                    _ => return Err(EditError::UnsupportedValue),
                }
            } else {
                false
            };
            Some((width(new, "CodeLength")?, number))
        } else {
            None
        };
        Ok(Some(Self {
            code,
            description: description_changed
                .then(|| width(new, "DescriptionLength"))
                .transpose()?,
        }))
    }

    /// Existing codes and descriptions cannot be truncated or reinterpreted by a qualifier change.
    pub(crate) fn validate(&self, input: &str) -> Result<(), EditError> {
        let document = Document::parse(input).map_err(|_| EditError::InvalidXml)?;
        if !document
            .root_element()
            .has_tag_name((PREDEF, "PredefinedData"))
        {
            return Err(EditError::InvalidXml);
        }
        for item in document
            .descendants()
            .filter(|node| node.has_tag_name((PREDEF, "Item")))
        {
            for node in item.children().filter(Node::is_element) {
                if node.tag_name().namespace() != Some(PREDEF) {
                    continue;
                }
                let value = node.text().unwrap_or_default();
                let valid = match node.tag_name().name() {
                    "Code" => self.code.is_none_or(|(width, number)| {
                        value.is_empty()
                            || (value.encode_utf16().count() <= width
                                && (!number || value.bytes().all(|byte| byte.is_ascii_digit())))
                    }),
                    "Description" => self
                        .description
                        .is_none_or(|width| value.encode_utf16().count() <= width),
                    _ => true,
                };
                if !valid {
                    return Err(incompatible("Predefined"));
                }
            }
        }
        Ok(())
    }
}

/// The predefined payload belongs to a top-level metadata object's descriptor.
fn owner_properties<'a>(document: &'a Document<'_>) -> Option<Node<'a, 'a>> {
    let root = document.root_element();
    if !root.has_tag_name((MD, "MetaDataObject")) {
        return None;
    }
    child(root.children().find(Node::is_element)?, "Properties")
}

/// Preserve namespace identity while resolving direct properties.
fn child<'a>(parent: Node<'a, 'a>, name: &str) -> Option<Node<'a, 'a>> {
    parent.children().find(|node| node.has_tag_name((MD, name)))
}

/// Missing scalar values remain unknown instead of acquiring an invented platform default.
fn text<'a>(parent: Node<'a, 'a>, name: &str) -> Option<&'a str> {
    child(parent, name).and_then(|node| node.text())
}

/// Domain widths must be present and nonnegative when changing a stored qualifier.
fn width(properties: Node<'_, '_>, name: &str) -> Result<usize, EditError> {
    text(properties, name)
        .and_then(|value| value.parse().ok())
        .ok_or(EditError::UnsupportedValue)
}

/// Only the two XML Schema true spellings activate a dependent flag.
fn enabled(properties: Node<'_, '_>, name: &str) -> bool {
    matches!(text(properties, name), Some("true" | "1"))
}

/// The existing error contract points both CLI and IDE at the conflicting property.
fn incompatible(name: &str) -> EditError {
    EditError::IncompatibleProperty(PropertyKey {
        namespace: Some(MD.to_owned()),
        name: name.to_owned(),
    })
}

#[cfg(test)]
mod tests;
