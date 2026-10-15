//! Session parameters retain their configured types without becoming fake metadata managers.

#[cfg(test)]
mod tests;

use bsl_metadata::{ObjectBelonging, SessionParameter};
use eska::project_core::{MetadataKind, metadata_parser::ParsedObject};

use crate::{AdapterError, AdapterIssue, properties::Properties};

/// Project the exact current identity, declared type and checked extension ownership.
///
/// # Errors
/// Returns invalid kind, identity, ownership or incomplete/unsupported typed properties.
pub fn session_parameter(object: &ParsedObject) -> Result<SessionParameter, AdapterError> {
    if object.metadata.kind() != MetadataKind::SessionParameter {
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
    let (belonging, extends) = crate::identity::ownership(&fields)?;
    if belonging == ObjectBelonging::Adopted && extends.is_none() {
        return Err(AdapterError {
            issue: AdapterIssue::MissingProperty {
                name: "ExtendedConfigurationObject".into(),
            },
            range: object.range.clone(),
        });
    }
    if belonging != ObjectBelonging::Adopted && extends.is_some() {
        return Err(AdapterError {
            issue: AdapterIssue::InvalidProperty {
                name: "ExtendedConfigurationObject".into(),
            },
            range: object.range.clone(),
        });
    }
    Ok(SessionParameter::from_parts(
        crate::identity::uuid(object)?,
        object.metadata.name().into(),
        crate::type_description::required_type(object)?,
        belonging,
        extends,
    ))
}
