//! Parameter Use reuses qualified reference-list parsing and preserves borrowed-property absence.

use crate::{AdapterError, AdapterIssue, identity, properties::Properties};
use bsl_metadata::{FunctionalOptionParameterUse, FunctionalOptionsParameter, ObjectBelonging};
use eska::project_core::{MetadataKind, metadata_parser::ParsedObject};

#[cfg(test)]
mod tests;

/// Project current catalog/dimension targets without inventing interface parameter values.
///
/// # Errors
/// Rejects wrong identity, incomplete own Use, unknown targets, duplicates and inconsistent ownership.
pub fn functional_option_parameter(
    object: &ParsedObject,
) -> Result<FunctionalOptionsParameter, AdapterError> {
    if object.metadata.kind() != MetadataKind::FunctionalOptionsParameter {
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
    let field = fields.field("Use")?;
    if belonging != ObjectBelonging::Adopted && field.is_none() {
        return Err(AdapterError {
            issue: AdapterIssue::MissingProperty { name: "Use".into() },
            range: object.range.clone(),
        });
    }
    let uses = field
        .map(|field| {
            let mut result = Vec::new();
            for path in fields.references("Use", "Item")? {
                let target = match path.split('.').collect::<Vec<_>>().as_slice() {
                    ["Catalog", name] if !name.is_empty() => {
                        FunctionalOptionParameterUse::Catalog {
                            name: (*name).into(),
                        }
                    }
                    ["InformationRegister", name, "Dimension", member]
                        if !name.is_empty() && !member.is_empty() =>
                    {
                        FunctionalOptionParameterUse::InformationRegisterDimension {
                            register: (*name).into(),
                            dimension: (*member).into(),
                        }
                    }
                    _ => {
                        return Err(AdapterError {
                            issue: AdapterIssue::UnsupportedPropertyValue {
                                name: "Use".into(),
                                value: path.into(),
                            },
                            range: field.range.clone(),
                        });
                    }
                };
                if result.iter().any(|other| target.same_target(other)) {
                    return Err(Properties::invalid(field));
                }
                result.push(target);
            }
            Ok(result)
        })
        .transpose()?;
    Ok(FunctionalOptionsParameter::from_parts(
        identity::uuid(object)?,
        object.metadata.name().into(),
        uses,
        belonging,
        extends,
    ))
}
