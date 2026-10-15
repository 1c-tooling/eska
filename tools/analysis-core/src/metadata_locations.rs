//! Metadata navigation selects captured object/property spans from the authoritative current owner.

#[cfg(test)]
mod tests;

use std::ops::Range;

use eska::{
    analysis_core::{InputFailure, InputOrigin, InputState},
    project_core::{
        metadata_model::PropertyKey,
        source_format::{CoordinateError, NativeUuid, SourcePath, TextRange},
    },
};

use crate::{AnalysisSession, SessionRequestError, SessionRevision};

/// One object or qualified property belongs to one physical YAML input, including nested documents.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataLocation {
    pub revision: SessionRevision,
    pub source: SourcePath,
    pub root: crate::composition::RootId,
    pub source_sha256: String,
    pub origin: InputOrigin,
    pub uuid: String,
    pub native_path: SourcePath,
    pub byte_range: Range<usize>,
    pub range: TextRange,
}

/// Unread or ambiguous current metadata never becomes a guessed file-level success.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MetadataLocationError {
    Request(SessionRequestError),
    Pending,
    Unavailable(InputFailure),
    ObjectNotFound,
    AmbiguousObject,
    PropertiesNotLoaded,
    PropertyNotFound,
    AmbiguousProperty,
    InconsistentCoordinates(CoordinateError),
}

impl AnalysisSession {
    /// Locate a native UUID and optional `QName` in the ready current snapshot without any I/O.
    ///
    /// Unsupported BSL projection does not prevent navigation to valid metadata in the tree.
    /// Byte and UTF-16 ranges come from the same parse; raw text is neither retained nor reread.
    ///
    /// # Errors
    /// Returns stale/unknown input, unread metadata, absent/ambiguous identity/property or bad spans.
    pub fn metadata_location(
        &self,
        path: &SourcePath,
        expected: SessionRevision,
        uuid: &NativeUuid,
        property: Option<&PropertyKey>,
    ) -> Result<MetadataLocation, MetadataLocationError> {
        self.metadata_location_at(&self.selected_source(path), expected, uuid, property)
    }

    /// Locate a UUID/property only in this root's current captured YAML coordinates.
    ///
    /// # Errors
    /// Returns stale or unavailable input, absent/ambiguous metadata and inconsistent captured ranges.
    pub fn metadata_location_at(
        &self,
        key: &crate::composition::SourceKey,
        expected: SessionRevision,
        uuid: &NativeUuid,
        property: Option<&PropertyKey>,
    ) -> Result<MetadataLocation, MetadataLocationError> {
        let view = self
            .metadata_at(key, expected)
            .map_err(MetadataLocationError::Request)?;
        let snapshot = match &view.input {
            InputState::Ready(snapshot) => snapshot,
            InputState::Pending => return Err(MetadataLocationError::Pending),
            InputState::Unavailable(failure) => {
                return Err(MetadataLocationError::Unavailable(failure.clone()));
            }
        };
        let mut matches = snapshot.documents.iter().flat_map(|document| {
            document
                .descriptor
                .objects
                .iter()
                .filter(|object| object.metadata.uuid().eq_ignore_ascii_case(uuid.as_str()))
                .map(move |object| (document, object))
        });
        let (document, object) = matches
            .next()
            .ok_or(MetadataLocationError::ObjectNotFound)?;
        if matches.next().is_some() {
            return Err(MetadataLocationError::AmbiguousObject);
        }
        let byte_range = if let Some(key) = property {
            let properties = object
                .properties
                .as_ref()
                .ok_or(MetadataLocationError::PropertiesNotLoaded)?;
            let mut matches = properties.iter().filter(|item| &item.property.key == key);
            let candidate = matches
                .next()
                .ok_or(MetadataLocationError::PropertyNotFound)?;
            if matches.next().is_some() {
                return Err(MetadataLocationError::AmbiguousProperty);
            }
            candidate.range.clone()
        } else {
            object.range.clone()
        };
        let range = snapshot
            .coordinates
            .range(byte_range.clone())
            .map_err(MetadataLocationError::InconsistentCoordinates)?;
        Ok(MetadataLocation {
            revision: expected,
            source: key.source.clone(),
            root: key.root,
            source_sha256: snapshot.source_sha256.clone(),
            origin: view.origin,
            uuid: object.metadata.uuid().to_owned(),
            native_path: document.path.clone(),
            byte_range,
            range,
        })
    }
}
