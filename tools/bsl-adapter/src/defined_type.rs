//! Defined types preserve identity and use the same type description as attributes and registers.

use bsl_metadata::DefinedType;
use eska::project_core::{MetadataKind, metadata_parser::ParsedObject};

use crate::{AdapterError, AdapterIssue, type_description::required_type};

/// Supply every required builder value explicitly; incomplete input never reaches its panic paths.
///
/// # Errors
/// Returns wrong kind, missing properties, invalid UUID or an invalid/unsupported type description.
pub fn defined_type(object: &ParsedObject) -> Result<DefinedType, AdapterError> {
    if object.metadata.kind() != MetadataKind::DefinedType {
        return Err(AdapterError {
            issue: AdapterIssue::WrongKind {
                actual: object.metadata.kind(),
            },
            range: object.range.clone(),
        });
    }
    let underlying_type = required_type(object)?;
    let uuid = crate::identity::uuid(object)?;
    Ok(DefinedType::builder()
        .uuid(uuid)
        .name(object.metadata.name())
        .underlying_type(underlying_type)
        .build())
}
