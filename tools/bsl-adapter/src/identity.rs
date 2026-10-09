//! Builder defaults must never create random identities for source declarations.

use bsl_metadata::{ObjectBelonging, Uuid};
use eska::project_core::metadata_parser::{ParsedDescriptor, ParsedObject};

use crate::{AdapterError, AdapterIssue, properties::Properties};

/// Parse the actual declaration identity before invoking a kernel model builder.
pub fn uuid(object: &ParsedObject) -> Result<Uuid, AdapterError> {
    object.metadata.uuid().parse().map_err(|_| AdapterError {
        issue: AdapterIssue::InvalidUuid,
        range: object.range.clone(),
    })
}

/// Recoverable parse issues must stay visible rather than disappear behind a complete root model.
pub fn descriptor_root(descriptor: &ParsedDescriptor) -> Result<&ParsedObject, AdapterError> {
    if let Some(issue) = descriptor.diagnostics.first() {
        return Err(AdapterError {
            issue: AdapterIssue::IncompleteDescriptor,
            range: issue.range.clone(),
        });
    }
    descriptor.objects.first().ok_or(AdapterError {
        issue: AdapterIssue::MissingRoot,
        range: 0..0,
    })
}

/// Preserve absent ownership and validate explicit base references without dropping malformed UUIDs.
pub fn ownership(fields: &Properties<'_>) -> Result<(ObjectBelonging, Option<Uuid>), AdapterError> {
    let belonging = if let Some(field) = fields.field("ObjectBelonging")? {
        match fields.text("ObjectBelonging")? {
            Some("Own") => ObjectBelonging::Own,
            Some("Adopted") => ObjectBelonging::Adopted,
            Some(value) => {
                return Err(AdapterError {
                    issue: AdapterIssue::UnsupportedOwnership {
                        value: value.into(),
                    },
                    range: field.range.clone(),
                });
            }
            None => return Err(Properties::invalid(field)),
        }
    } else {
        ObjectBelonging::Unknown
    };
    let extends = fields
        .field("ExtendedConfigurationObject")?
        .map(|field| match fields.text("ExtendedConfigurationObject")? {
            Some("") => Ok(None),
            Some(value) => value
                .parse()
                .map(Some)
                .map_err(|_| Properties::invalid(field)),
            None => Err(Properties::invalid(field)),
        })
        .transpose()?
        .flatten();
    Ok((belonging, extends))
}
