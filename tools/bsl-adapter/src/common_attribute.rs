//! Ordinary common attributes use the shared property tree and explicit readable membership.

#[cfg(test)]
mod tests;

use bsl_metadata::{
    CommonAttribute, CommonAttributeTarget, CommonAttributeUse, MdoType, ObjectBelonging,
};
use eska::project_core::{
    MetadataKind,
    metadata_model::{MetadataProperty, MetadataValue},
    metadata_parser::{LocatedProperty, ParsedObject},
};

use crate::{AdapterError, AdapterIssue, identity, properties::Properties};

const READABLE: &str = "http://v8.1c.ru/8.3/xcf/readable";

/// Admit current ordinary explicit membership without guessing automatic-use or separation semantics.
///
/// # Errors
/// Rejects incomplete identity/type, ambiguous content, automatic use, data separation or unknown shapes.
pub fn common_attribute(object: &ParsedObject) -> Result<CommonAttribute, AdapterError> {
    if object.metadata.kind() != MetadataKind::CommonAttribute {
        return Err(AdapterError {
            issue: AdapterIssue::WrongKind {
                actual: object.metadata.kind(),
            },
            range: object.range.clone(),
        });
    }
    if !object.children.is_empty() || object.metadata.parent().is_some() {
        return Err(AdapterError {
            issue: AdapterIssue::IncompleteDescriptor,
            range: object.range.clone(),
        });
    }
    let fields = Properties::new(object)?;
    let (belonging, extends) = identity::ownership(&fields)?;
    if (belonging == ObjectBelonging::Adopted) != extends.is_some() {
        return Err(AdapterError {
            issue: AdapterIssue::InvalidProperty {
                name: "ExtendedConfigurationObject".into(),
            },
            range: object.range.clone(),
        });
    }
    for name in ["AutoUse", "DataSeparation"] {
        let field = required(&fields, object, name)?;
        let Some(value) = fields.text(name)? else {
            return Err(Properties::invalid(field));
        };
        if value != "DontUse" {
            return Err(AdapterError {
                issue: AdapterIssue::UnsupportedPropertyValue {
                    name: name.into(),
                    value: value.into(),
                },
                range: field.range.clone(),
            });
        }
    }
    let content = required(&fields, object, "Content")?;
    Ok(CommonAttribute::from_parts(
        identity::uuid(object)?,
        object.metadata.name().into(),
        crate::type_description::required_type(object)?,
        targets(content)?,
        belonging,
        extends,
    ))
}

/// Absence is not an implicit platform default for fields controlling membership.
fn required<'a>(
    fields: &Properties<'a>,
    object: &ParsedObject,
    name: &str,
) -> Result<&'a LocatedProperty, AdapterError> {
    fields.field(name)?.ok_or_else(|| AdapterError {
        issue: AdapterIssue::MissingProperty { name: name.into() },
        range: object.range.clone(),
    })
}

/// Content records keep namespace identity, reject duplicate targets and never reinterpret another family's list.
fn targets(field: &LocatedProperty) -> Result<Vec<CommonAttributeTarget>, AdapterError> {
    let entries = match &field.property.value {
        MetadataValue::Text(value) if value.is_empty() => return Ok(Vec::new()),
        MetadataValue::Record(entries) => entries,
        _ => return Err(Properties::invalid(field)),
    };
    let mut result: Vec<CommonAttributeTarget> = Vec::new();
    for entry in entries {
        if entry.key.namespace.as_deref() != Some(READABLE) || entry.key.name != "Item" {
            return Err(Properties::invalid(field));
        }
        let MetadataValue::Record(parts) = &entry.value else {
            return Err(Properties::invalid(field));
        };
        if parts.iter().any(|part| {
            part.key.namespace.as_deref() != Some(READABLE)
                || !matches!(
                    part.key.name.as_str(),
                    "Metadata" | "Use" | "ConditionalSeparation"
                )
        }) {
            return Err(Properties::invalid(field));
        }
        let text = part(parts, "Metadata", field)?.ok_or_else(|| Properties::invalid(field))?;
        let (family, name) = text
            .split_once('.')
            .ok_or_else(|| Properties::invalid(field))?;
        let kind = family
            .parse::<MdoType>()
            .ok()
            .filter(|kind| {
                matches!(
                    kind,
                    MdoType::Catalog
                        | MdoType::Document
                        | MdoType::ExchangePlan
                        | MdoType::BusinessProcess
                        | MdoType::Task
                        | MdoType::ChartOfAccounts
                        | MdoType::ChartOfCalculationTypes
                        | MdoType::ChartOfCharacteristicTypes
                        | MdoType::InformationRegister
                        | MdoType::AccumulationRegister
                        | MdoType::AccountingRegister
                        | MdoType::CalculationRegister
                )
            })
            .ok_or_else(|| AdapterError {
                issue: AdapterIssue::UnsupportedPropertyValue {
                    name: "Content".into(),
                    value: text.into(),
                },
                range: field.range.clone(),
            })?;
        if name.is_empty() || name.contains('.') {
            return Err(Properties::invalid(field));
        }
        let usage = match part(parts, "Use", field)? {
            Some("Use") => CommonAttributeUse::Use,
            Some("DontUse") => CommonAttributeUse::DontUse,
            Some("Auto") => CommonAttributeUse::Auto,
            _ => return Err(Properties::invalid(field)),
        };
        if part(parts, "ConditionalSeparation", field)?.is_some_and(|value| !value.is_empty())
            || result.iter().any(|previous| {
                previous.kind == kind
                    && previous
                        .name
                        .chars()
                        .flat_map(char::to_lowercase)
                        .eq(name.chars().flat_map(char::to_lowercase))
            })
        {
            return Err(Properties::invalid(field));
        }
        result.push(CommonAttributeTarget {
            kind,
            name: name.into(),
            usage,
        });
    }
    Ok(result)
}

/// Missing optional leaves differ from empty leaves; repeated or structured scalars remain invalid.
fn part<'a>(
    parts: &'a [MetadataProperty],
    name: &str,
    field: &LocatedProperty,
) -> Result<Option<&'a str>, AdapterError> {
    let mut found = parts.iter().filter(|part| part.key.name == name);
    let value = found.next();
    if found.next().is_some() {
        return Err(Properties::invalid(field));
    }
    value
        .map(|part| match &part.value {
            MetadataValue::Text(text) => Ok(text.as_str()),
            _ => Err(Properties::invalid(field)),
        })
        .transpose()
}
