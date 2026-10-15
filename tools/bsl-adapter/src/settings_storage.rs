//! Settings-storage identity projects independently of UI form defaults and native manager methods.

use crate::{AdapterError, AdapterIssue, identity, properties::Properties};
use bsl_metadata::{ObjectBelonging, SettingsStorage};
use eska::project_core::{MetadataKind, metadata_parser::ParsedObject};

#[cfg(test)]
mod tests;

/// Preserve a current settings-storage owner without fabricating record or reference types.
///
/// # Errors
/// Rejects wrong kinds, nested roots, uncaptured properties, invalid UUIDs and inconsistent ownership.
pub fn settings_storage(object: &ParsedObject) -> Result<SettingsStorage, AdapterError> {
    if object.metadata.kind() != MetadataKind::SettingsStorage {
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
    Ok(SettingsStorage::from_parts(
        identity::uuid(object)?,
        object.metadata.name().into(),
        belonging,
        extends,
    ))
}
