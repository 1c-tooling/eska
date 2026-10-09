//! Defined types preserve identity and use the same type description as attributes and registers.

use bsl_metadata::DefinedType;
use eska::project_core::{MetadataKind, metadata_parser::ParsedObject};

use crate::{AdapterError, AdapterIssue, attribute_type, properties::Properties};

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
    let fields = Properties::new(object)?;
    let uuid = crate::identity::uuid(object)?;
    let field = fields.field("Type")?.ok_or_else(|| AdapterError {
        issue: AdapterIssue::MissingProperty {
            name: "Type".into(),
        },
        range: object.range.clone(),
    })?;
    Ok(DefinedType::builder()
        .uuid(uuid)
        .name(object.metadata.name())
        .underlying_type(attribute_type(field)?)
        .build())
}
