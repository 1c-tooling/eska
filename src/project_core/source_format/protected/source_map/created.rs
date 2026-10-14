//! Created owners share coordinate validation while every resulting location remains working evidence.

use std::ops::Range;

use super::{
    CREATED_SOURCE_MAP_VERSION, FileOrigin, InputAuthority, MappedFile, OriginTexts,
    SourceLocation, SourceMap,
};
use crate::project_core::source_format::{Error, ObjectSource};

/// Working-only records never have accepted evidence regardless of their native metadata class.
pub(super) const fn is_created(origin: &FileOrigin) -> bool {
    matches!(
        origin,
        FileOrigin::CreatedMetadata { .. } | FileOrigin::CreatedManagedForm { .. }
    )
}

/// The new envelope permits working-only children while the original root must remain accepted.
pub(super) fn normalized(map: &SourceMap) -> Result<Option<SourceMap>, Error> {
    if !map.files.iter().any(|file| is_created(&file.origin)) {
        return Ok(None);
    }
    let root = map
        .external_root
        .as_ref()
        .map_or("Configuration.xml", |root| root.path.as_str());
    if map.schema_version != CREATED_SOURCE_MAP_VERSION
        || map
            .files
            .iter()
            .any(|file| file.path.as_str() == root && is_created(&file.origin))
    {
        return Err(Error::root("source_map_origin_invalid"));
    }
    let mut normalized = map.clone();
    for file in &mut normalized.files {
        if let Some(origin) = existing(&file.origin) {
            file.origin = origin;
        }
    }
    Ok(Some(normalized))
}

/// Reuse coordinate invariants internally without serializing a fictitious accepted input.
pub(super) fn existing(origin: &FileOrigin) -> Option<FileOrigin> {
    match origin {
        FileOrigin::CreatedMetadata { uuid, input } => Some(FileOrigin::Metadata {
            uuid: uuid.clone(),
            accepted: input.clone(),
            working: Some(input.clone()),
            uses_working_values: true,
        }),
        FileOrigin::CreatedManagedForm {
            uuid,
            descriptor,
            input,
            queries,
        } => Some(FileOrigin::ManagedForm {
            uuid: uuid.clone(),
            descriptor: descriptor.clone(),
            accepted: input.clone(),
            working: Some(input.clone()),
            uses_working_values: true,
            queries: queries.clone(),
        }),
        _ => None,
    }
}

/// New descriptor definitions and values originate in one exact captured working owner file.
pub(super) fn locate(
    file: &MappedFile,
    native: &[u8],
    range: Range<usize>,
    texts: Option<OriginTexts<'_>>,
) -> Result<SourceLocation, Error> {
    let text = texts
        .and_then(|texts| texts.working)
        .ok_or_else(|| Error::root("source_map_origin_text_missing"))?;
    if let FileOrigin::CreatedMetadata { uuid, input } = &file.origin {
        super::location::verify_bytes(text.as_bytes(), input.bytes, &input.sha256)?;
        let owner = ObjectSource::from_yaml(text)?;
        let document = std::iter::once(&owner.document)
            .chain(&owner.nested)
            .find(|document| document.path == file.path && document.descriptor.object.uuid == *uuid)
            .ok_or_else(|| Error::root("source_map_origin_mismatch"))?;
        if document.descriptor.to_xml()?.as_bytes() != native {
            return Err(Error::root("source_map_origin_mismatch"));
        }
    }
    let normalized = MappedFile {
        origin: existing(&file.origin).ok_or_else(|| Error::root("source_map_origin_invalid"))?,
        ..file.clone()
    };
    let mut location = normalized.locate(
        native,
        range,
        Some(OriginTexts {
            accepted: text,
            working: Some(text),
        }),
    )?;
    location.authority = InputAuthority::Working;
    Ok(location)
}
