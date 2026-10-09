//! Builder defaults must never create random identities for source declarations.

use bsl_metadata::Uuid;
use eska::project_core::metadata_parser::ParsedObject;

use crate::{AdapterError, AdapterIssue};

/// Parse the actual declaration identity before invoking a kernel model builder.
pub fn uuid(object: &ParsedObject) -> Result<Uuid, AdapterError> {
    object.metadata.uuid().parse().map_err(|_| AdapterError {
        issue: AdapterIssue::InvalidUuid,
        range: object.range.clone(),
    })
}
