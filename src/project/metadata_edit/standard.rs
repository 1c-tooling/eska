//! Implicit standard-attribute types come from their owner, not from the saved filling value.

use roxmltree::{Document, Node};

use super::{
    EditError, EditableValueType, ValueConstraints,
    schema::{MD, READABLE, XS},
    types, value_schema,
};
use crate::project::metadata_model::PropertyKey;

/// Restrict this writer to reviewed `StandardAttributeProxyRule` contexts from the installed EDT model.
pub(super) fn value_types(node: Node<'_, '_>) -> Option<Vec<EditableValueType>> {
    let attribute = node
        .parent()
        .filter(|node| node.has_tag_name((READABLE, "StandardAttribute")))?;
    let group = attribute
        .parent()
        .filter(|node| node.has_tag_name((MD, "StandardAttributes")))?;
    let properties = group
        .parent()
        .filter(|node| node.has_tag_name((MD, "Properties")))?;
    let owner = properties
        .parent()
        .filter(|node| node.tag_name().namespace() == Some(MD))?;
    let class = owner.tag_name().name();
    let name = attribute.attribute("name")?;
    let choice = match (class, name) {
        ("Catalog" | "ChartOfCalculationTypes", "Code") => {
            code(properties, "Code", child_text(properties, "CodeType")?)?
        }
        ("ExchangePlan" | "ChartOfAccounts" | "ChartOfCharacteristicTypes", "Code") => {
            code(properties, "Code", "String")?
        }
        ("Document" | "BusinessProcess" | "Task", "Number") => {
            code(properties, "Number", child_text(properties, "NumberType")?)?
        }
        (
            "Catalog"
            | "ExchangePlan"
            | "ChartOfAccounts"
            | "ChartOfCharacteristicTypes"
            | "ChartOfCalculationTypes"
            | "Task",
            "Description",
        ) => primitive(
            "string",
            ValueConstraints::String {
                max_length: length(properties, "DescriptionLength")?,
            },
        ),
        (
            "Catalog"
            | "Document"
            | "BusinessProcess"
            | "Task"
            | "ExchangePlan"
            | "ChartOfAccounts"
            | "ChartOfCharacteristicTypes"
            | "ChartOfCalculationTypes",
            "DeletionMark",
        )
        | ("Document", "Posted")
        | ("BusinessProcess", "Started" | "Completed")
        | ("Task", "Executed")
        | ("ChartOfCalculationTypes", "ActionPeriodIsBasic") => {
            primitive("boolean", ValueConstraints::Boolean)
        }
        ("Document" | "BusinessProcess" | "Task", "Date") | ("ExchangePlan", "ExchangeDate") => {
            primitive(
                "dateTime",
                ValueConstraints::Date {
                    fractions: "DateTime".to_owned(),
                },
            )
        }
        ("Catalog" | "ChartOfCharacteristicTypes" | "ChartOfAccounts", "Parent") => {
            if class != "ChartOfAccounts"
                && !child_text(properties, "Hierarchical")
                    .is_some_and(|value| matches!(value, "true" | "1"))
            {
                return None;
            }
            EditableValueType {
                key: PropertyKey {
                    namespace: Some(types::CFG.to_owned()),
                    name: format!("{class}Ref.{}", child_text(properties, "Name")?),
                },
                constraints: ValueConstraints::Reference,
            }
        }
        _ => return None,
    };
    Some(vec![choice])
}

/// A parent selector offers only forms of its own concrete reference type.
pub(super) fn reference_type(node: Node<'_, '_>) -> Option<PropertyKey> {
    let mut choices = value_types(node)?.into_iter();
    let first = choices.next()?;
    (choices.next().is_none() && matches!(first.constraints, ValueConstraints::Reference))
        .then_some(first.key)
}

/// Changes to owner qualifiers must not silently truncate or reinterpret an existing standard value.
pub(super) fn validate_dependents(
    node: Node<'_, '_>,
    candidate: &Document<'_>,
) -> Result<(), EditError> {
    let dependent = match (node.tag_name().namespace(), node.tag_name().name()) {
        (Some(MD), "CodeLength" | "CodeType") => "Code",
        (Some(MD), "NumberLength" | "NumberType") => "Number",
        (Some(MD), "DescriptionLength") => "Description",
        (Some(MD), "Hierarchical") => "Parent",
        _ => return Ok(()),
    };
    let Some(properties) = node
        .parent()
        .filter(|node| node.has_tag_name((MD, "Properties")))
    else {
        return Ok(());
    };
    let properties = candidate
        .descendants()
        .find(|node| {
            node.has_tag_name((MD, "Properties")) && node.range().start == properties.range().start
        })
        .ok_or(EditError::InvalidXml)?;
    for attribute in properties
        .children()
        .filter(|node| node.has_tag_name((MD, "StandardAttributes")))
        .flat_map(|node| node.children())
        .filter(|node| {
            node.has_tag_name((READABLE, "StandardAttribute"))
                && node.attribute("name") == Some(dependent)
        })
    {
        for field in attribute.children().filter(Node::is_element) {
            let compatible = if field.has_tag_name((READABLE, "FillValue")) {
                value_schema::compatible(field)
            } else if field.has_tag_name((READABLE, "ChoiceForm"))
                && field.text().is_some_and(|text| !text.trim().is_empty())
            {
                reference_type(field)
                    .and_then(|key| value_schema::reference_prefix(&key))
                    .is_some_and(|prefix| {
                        field
                            .text()
                            .unwrap_or_default()
                            .starts_with(&format!("{prefix}Form."))
                    })
            } else {
                true
            };
            if !compatible {
                return Err(EditError::IncompatibleProperty(PropertyKey {
                    namespace: Some(READABLE.to_owned()),
                    name: field.tag_name().name().to_owned(),
                }));
            }
        }
    }
    Ok(())
}

/// Code/number type and width belong to the owner; absent or disabled fields have no writable value domain.
fn code(properties: Node<'_, '_>, field: &str, kind: &str) -> Option<EditableValueType> {
    let width = length(properties, &format!("{field}Length"))?;
    let (name, constraints) = match kind {
        "String" => ("string", ValueConstraints::String { max_length: width }),
        "Number" if width <= 38 => (
            "decimal",
            ValueConstraints::Number {
                digits: width,
                fraction_digits: 0,
                nonnegative: true,
            },
        ),
        _ => return None,
    };
    Some(primitive(name, constraints))
}

/// A zero-width code or description is disabled; it must not become an unlimited string editor.
fn length(properties: Node<'_, '_>, name: &str) -> Option<u32> {
    child_text(properties, name)?
        .parse::<u32>()
        .ok()
        .filter(|value| *value > 0)
}

/// Read only direct metadata properties, never a similarly named nested or foreign field.
fn child_text<'a>(properties: Node<'a, '_>, name: &str) -> Option<&'a str> {
    properties
        .children()
        .find(|node| node.has_tag_name((MD, name)))?
        .text()
}

/// Standard primitive domains reuse the same validators and serializers as explicit Type descriptions.
fn primitive(name: &str, constraints: ValueConstraints) -> EditableValueType {
    EditableValueType {
        key: PropertyKey {
            namespace: Some(XS.to_owned()),
            name: name.to_owned(),
        },
        constraints,
    }
}

#[cfg(test)]
mod tests;
