//! Scheduled-job callbacks and flags come from the same current logical properties as IDE metadata.

#[cfg(test)]
mod tests;

use bsl_metadata::ScheduledJob;
use eska::project_core::{MetadataKind, metadata_parser::ParsedObject};

use crate::{AdapterError, AdapterIssue, properties::Properties};

/// Project only fields represented by the native job model, keeping explicit flags strict.
///
/// # Errors
/// Returns a wrong kind, invalid UUID, missing property capture, duplicate fields or malformed values.
pub fn scheduled_job(object: &ParsedObject) -> Result<ScheduledJob, AdapterError> {
    if object.metadata.kind() != MetadataKind::ScheduledJob {
        return Err(AdapterError {
            issue: AdapterIssue::WrongKind {
                actual: object.metadata.kind(),
            },
            range: object.range.clone(),
        });
    }
    let fields = Properties::new(object)?;
    Ok(ScheduledJob::from_parts(
        crate::identity::uuid(object)?,
        object.metadata.name().into(),
        fields.text("MethodName")?.unwrap_or_default().into(),
        fields.boolean("Predefined")?.unwrap_or(false),
        fields.boolean("Use")?.unwrap_or(false),
    ))
}
