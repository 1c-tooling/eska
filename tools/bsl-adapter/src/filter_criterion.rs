//! Filter criteria use the shared QName-aware properties and `TypeDescription` without a second parser.

use crate::{AdapterError, AdapterIssue, identity, properties::Properties};
use bsl_metadata::{FilterCriterion, FilterCriterionTarget, MdoType, ObjectBelonging};
use eska::project_core::{MetadataKind, metadata_parser::ParsedObject};

/// Project current search type and explicit field membership, retaining missing borrowed properties.
///
/// # Errors
/// Rejects missing own fields, malformed references, duplicate content, unsupported targets and ownership.
pub fn filter_criterion(object: &ParsedObject) -> Result<FilterCriterion, AdapterError> {
    if object.metadata.kind() != MetadataKind::FilterCriterion {
        return Err(AdapterError {
            issue: AdapterIssue::WrongKind {
                actual: object.metadata.kind(),
            },
            range: object.range.clone(),
        });
    }
    if object.metadata.parent().is_some() {
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
    let type_field = fields.field("Type")?;
    let content_field = fields.field("Content")?;
    if belonging != ObjectBelonging::Adopted {
        for (name, field) in [("Type", type_field), ("Content", content_field)] {
            if field.is_none() {
                return Err(AdapterError {
                    issue: AdapterIssue::MissingProperty { name: name.into() },
                    range: object.range.clone(),
                });
            }
        }
    }
    fields.boolean("UseStandardCommands")?;
    let search_type = type_field.map(crate::attribute_type).transpose()?;
    let content = content_field
        .map(|field| content(&fields, field))
        .transpose()?;
    Ok(FilterCriterion::from_parts(
        identity::uuid(object)?,
        object.metadata.name().into(),
        search_type,
        content,
        belonging,
        extends,
    ))
}

/// Keep field-path validation separate from identity and borrowed-property admission.
fn content(
    fields: &Properties<'_>,
    field: &eska::project_core::metadata_parser::LocatedProperty,
) -> Result<Vec<FilterCriterionTarget>, AdapterError> {
    let mut result: Vec<FilterCriterionTarget> = Vec::new();
    for path in fields.references("Content")? {
        let parts = path.split('.').collect::<Vec<_>>();
        let (family, name) = match parts.as_slice() {
            [family, name, "Attribute" | "StandardAttribute", member] if !member.is_empty() => {
                (*family, *name)
            }
            [family, name, "TabularSection", section, "Attribute", member]
                if !section.is_empty() && !member.is_empty() =>
            {
                (*family, *name)
            }
            _ => return Err(Properties::invalid(field)),
        };
        let kind = family
            .parse::<MdoType>()
            .ok()
            .filter(|kind| {
                matches!(
                    kind,
                    MdoType::Catalog
                        | MdoType::Document
                        | MdoType::BusinessProcess
                        | MdoType::Task
                        | MdoType::ExchangePlan
                        | MdoType::ChartOfAccounts
                        | MdoType::ChartOfCharacteristicTypes
                        | MdoType::ChartOfCalculationTypes
                )
            })
            .ok_or_else(|| AdapterError {
                issue: AdapterIssue::UnsupportedPropertyValue {
                    name: "Content".into(),
                    value: path.into(),
                },
                range: field.range.clone(),
            })?;
        if name.is_empty()
            || result.iter().any(|value| {
                value
                    .path
                    .chars()
                    .flat_map(char::to_lowercase)
                    .eq(path.chars().flat_map(char::to_lowercase))
            })
        {
            return Err(Properties::invalid(field));
        }
        result.push(FilterCriterionTarget {
            kind,
            name: name.into(),
            path: path.into(),
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
