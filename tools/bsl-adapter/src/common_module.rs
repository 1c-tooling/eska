//! Construct real BSL-kernel module flags from canonical properties, preserving absence and UUID.

#[cfg(test)]
mod tests;

use bsl_metadata::{CommonModule, ObjectBelonging, ReturnValueReuse};
use eska::project_core::{MetadataKind, metadata_parser::ParsedObject};

use crate::{AdapterError, AdapterIssue, properties::Properties};

/// Project one common module without interpreting source text as Designer XML.
///
/// # Errors
/// Returns missing properties, wrong kind, invalid UUID, duplicate keys or invalid flag values.
pub fn common_module(object: &ParsedObject) -> Result<CommonModule, AdapterError> {
    if object.metadata.kind() != MetadataKind::CommonModule {
        return Err(AdapterError {
            issue: AdapterIssue::WrongKind {
                actual: object.metadata.kind(),
            },
            range: object.range.clone(),
        });
    }
    let fields = Properties::new(object)?;
    let uuid = crate::identity::uuid(object)?;
    let mut builder = CommonModule::builder()
        .uuid(uuid)
        .name(object.metadata.name())
        .object_belonging(ObjectBelonging::Unknown);
    // Each setter is called only when present; absence is required for borrowed-module inheritance.
    if let Some(value) = fields.boolean("Server")? {
        builder = builder.server(value);
    }
    if let Some(value) = fields.boolean("Global")? {
        builder = builder.global(value);
    }
    if let Some(value) = fields.boolean("ClientManagedApplication")? {
        builder = builder.client_managed_application(value);
    }
    if let Some(value) = fields.boolean("ClientOrdinaryApplication")? {
        builder = builder.client_ordinary_application(value);
    }
    if let Some(value) = fields.boolean("ExternalConnection")? {
        builder = builder.external_connection(value);
    }
    if let Some(value) = fields.boolean("ServerCall")? {
        builder = builder.server_call(value);
    }
    if let Some(value) = fields.boolean("Privileged")? {
        builder = builder.privileged(value);
    }
    if let Some(value) = fields.text("ReturnValuesReuse")? {
        let reuse = ReturnValueReuse::from_name(value);
        if reuse == ReturnValueReuse::Unknown {
            return Err(AdapterError {
                issue: AdapterIssue::InvalidProperty {
                    name: "ReturnValuesReuse".into(),
                },
                range: fields
                    .field("ReturnValuesReuse")?
                    .map_or_else(|| object.range.clone(), |field| field.range.clone()),
            });
        }
        builder = builder.return_values_reuse(reuse);
    }
    if let Some(value) = fields.text("ObjectBelonging")? {
        let belonging = match value {
            "Own" => ObjectBelonging::Own,
            "Adopted" => ObjectBelonging::Adopted,
            _ => {
                return Err(AdapterError {
                    issue: AdapterIssue::UnsupportedOwnership {
                        value: value.into(),
                    },
                    range: fields
                        .field("ObjectBelonging")?
                        .map_or_else(|| object.range.clone(), |field| field.range.clone()),
                });
            }
        };
        builder = builder.object_belonging(belonging);
    }
    if let Some(field) = fields.field("ExtendedConfigurationObject")? {
        let Some(value) = fields.text("ExtendedConfigurationObject")? else {
            return Err(Properties::invalid(field));
        };
        if !value.is_empty() {
            builder = builder.extends_uuid(value.parse().map_err(|_| Properties::invalid(field))?);
        }
    }
    Ok(builder.build())
}
