//! Designer file loading uses the shared, representation-independent metadata parser.

use std::path::PathBuf;

use super::{
    designer_source::{DesignerSource, SourceError},
    metadata_model::{MetadataKind, ObjectId},
};

pub(crate) use crate::project_core::metadata_parser::check_envelope;
pub use crate::project_core::metadata_parser::*;

/// File loading preserves both source and parse errors without guessing an object.
#[derive(Debug)]
pub enum LoadError {
    Source(SourceError),
    MissingDescriptor(ObjectId),
    Parse { path: PathBuf, source: ParseError },
    ObjectNotFound(ObjectId),
}

/// Read exactly one owning descriptor, including inline ancestors, without opening children.
///
/// # Errors
/// Returns a missing/unsafe source, descriptor parse failure or absent requested identity.
pub fn load(
    source: &DesignerSource,
    id: &ObjectId,
    mode: PropertiesMode,
) -> Result<ParsedDescriptor, LoadError> {
    let location = source
        .object_descriptor(id)
        .map_err(LoadError::Source)?
        .ok_or_else(|| LoadError::MissingDescriptor(id.clone()))?;
    let input = source
        .read_xml(&location.path)
        .map_err(LoadError::Source)?
        .ok_or_else(|| LoadError::MissingDescriptor(id.clone()))?;
    let mut descriptor_id = id.clone();
    for _ in &location.inline {
        descriptor_id = descriptor_id
            .parent()
            .ok_or_else(|| LoadError::ObjectNotFound(id.clone()))?;
    }
    let parsed = (if location
        .inline
        .first()
        .is_some_and(|item| item.kind == MetadataKind::PredefinedItem)
    {
        parse_predefined(&input, &descriptor_id, mode)
    } else {
        parse(&input, descriptor_id.parent(), mode)
    })
    .map_err(|source| LoadError::Parse {
        path: location.path,
        source,
    })?;
    if !parsed
        .objects
        .iter()
        .any(|object| object.metadata.id() == id)
    {
        return Err(LoadError::ObjectNotFound(id.clone()));
    }
    Ok(parsed)
}
