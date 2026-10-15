//! Functional options reuse the shared qualified-property model and native storage identities.

use crate::{AdapterError, AdapterIssue, identity, properties::Properties};
use bsl_metadata::{FunctionalOption, FunctionalOptionLocation, ObjectBelonging};
use eska::project_core::{
    MetadataKind,
    metadata_parser::{LocatedProperty, ParsedObject},
};

#[cfg(test)]
mod tests;

/// Project explicit current storage and content, leaving inherited borrowed properties absent.
///
/// # Errors
/// Rejects wrong identities, missing own properties, malformed storage, unsupported ownership and flags.
pub fn functional_option(object: &ParsedObject) -> Result<FunctionalOption, AdapterError> {
    if object.metadata.kind() != MetadataKind::FunctionalOption {
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
    if belonging != ObjectBelonging::Adopted {
        for name in ["Location", "PrivilegedGetMode", "Content"] {
            if fields.field(name)?.is_none() {
                return Err(AdapterError {
                    issue: AdapterIssue::MissingProperty { name: name.into() },
                    range: object.range.clone(),
                });
            }
        }
    }
    let location = fields
        .field("Location")?
        .map(|field| location(&fields, field))
        .transpose()?;
    let content = fields
        .field("Content")?
        .map(|field| {
            let mut result: Vec<String> = Vec::new();
            for path in fields.references("Content", "Object")? {
                if path.split('.').any(str::is_empty)
                    || !path.contains('.')
                    || result
                        .iter()
                        .any(|value| value.to_lowercase() == path.to_lowercase())
                {
                    return Err(Properties::invalid(field));
                }
                result.push(path.into());
            }
            Ok(result)
        })
        .transpose()?;
    Ok(FunctionalOption::from_parts(
        identity::uuid(object)?,
        object.metadata.name().into(),
        location,
        fields.boolean("PrivilegedGetMode")?,
        content,
        belonging,
        extends,
    ))
}

/// An explicit empty storage has no inferred result; unsupported paths cannot become a Boolean default.
fn location(
    fields: &Properties<'_>,
    field: &LocatedProperty,
) -> Result<FunctionalOptionLocation, AdapterError> {
    let Some(path) = fields.text("Location")? else {
        return Ok(FunctionalOptionLocation::Empty);
    };
    if path.is_empty() {
        return Ok(FunctionalOptionLocation::Empty);
    }
    let parts = path.split('.').collect::<Vec<_>>();
    match parts.as_slice() {
        ["Constant", name] if !name.is_empty() => Ok(FunctionalOptionLocation::Constant {
            name: (*name).into(),
        }),
        ["Catalog", name, "Attribute", member] if !name.is_empty() && !member.is_empty() => {
            Ok(FunctionalOptionLocation::CatalogAttribute {
                catalog: (*name).into(),
                attribute: (*member).into(),
            })
        }
        ["InformationRegister", name, "Resource", member]
            if !name.is_empty() && !member.is_empty() =>
        {
            Ok(FunctionalOptionLocation::InformationRegisterResource {
                register: (*name).into(),
                resource: (*member).into(),
            })
        }
        _ => Err(AdapterError {
            issue: AdapterIssue::UnsupportedPropertyValue {
                name: "Location".into(),
                value: path.into(),
            },
            range: field.range.clone(),
        }),
    }
}
