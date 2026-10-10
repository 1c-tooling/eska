//! Property previews bind supplied editor bytes to the same current metadata input and policy.

use eska::{
    analysis_core::{InputOrigin, InputState},
    project_core::source_format::{
        AcceptedBase, Error, NativeUuid, PropertyName, PropertyValue, SourcePath,
        SourcePropertyEdit, prepare_property_edit_text,
    },
};
use sha2::{Digest, Sha256};
use std::fmt::Write;

use crate::{AnalysisSession, SessionRequestError, SessionRevision};

/// A rejected preview never changes current input, source quota or kernel state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PropertyEditError {
    Request(SessionRequestError),
    InputUnavailable,
    DocumentNotOpen,
    DocumentVersion { current: i64, received: i64 },
    SourceConflict,
    Source(Error),
}

/// Validated current bytes are borrowed only for one preparation, not retained in a plan cache.
#[derive(Debug)]
pub struct PropertyEditInput<'a> {
    source: SourcePath,
    revision: SessionRevision,
    document_version: i64,
    source_sha256: String,
    text: &'a str,
}

/// Permission and source preconditions travel with a lossless preview, without write authority.
#[derive(Debug)]
pub struct PropertyEditPlan {
    pub revision: SessionRevision,
    pub document_version: i64,
    pub source_sha256: String,
    pub preview: SourcePropertyEdit,
}

impl AnalysisSession {
    /// Validate an open current YAML document before a consumer opens costly baseline storage.
    ///
    /// # Errors
    /// Returns stale/unknown input, unread/invalid input, closed document, wrong version or hash.
    pub fn property_edit_input<'a>(
        &self,
        source: &SourcePath,
        expected: SessionRevision,
        document_version: i64,
        text: &'a str,
    ) -> Result<PropertyEditInput<'a>, PropertyEditError> {
        let view = self
            .metadata(source, expected)
            .map_err(PropertyEditError::Request)?;
        let InputState::Ready(snapshot) = &view.input else {
            return Err(PropertyEditError::InputUnavailable);
        };
        let InputOrigin::Overlay {
            document_version: current,
        } = view.origin
        else {
            return Err(PropertyEditError::DocumentNotOpen);
        };
        if current != document_version {
            return Err(PropertyEditError::DocumentVersion {
                current,
                received: document_version,
            });
        }
        if u64::try_from(text.len()).ok() != Some(snapshot.source_bytes) {
            return Err(PropertyEditError::SourceConflict);
        }
        let hash = Sha256::digest(text.as_bytes()).iter().fold(
            String::with_capacity(64),
            |mut hash, byte| {
                let _ = write!(hash, "{byte:02x}");
                hash
            },
        );
        if view.source_sha256.as_deref() != Some(hash.as_str()) {
            return Err(PropertyEditError::SourceConflict);
        }
        Ok(PropertyEditInput {
            source: source.clone(),
            revision: expected,
            document_version,
            source_sha256: hash,
            text,
        })
    }
}

impl PropertyEditInput<'_> {
    /// Resolve an accepted identity over these validated bytes using the shared CST editor.
    ///
    /// # Errors
    /// Returns policy, identity, value or owner mismatch failures without publishing the preview.
    pub fn prepare(
        self,
        base: &AcceptedBase,
        uuid: &NativeUuid,
        property: PropertyName,
        value: PropertyValue,
    ) -> Result<PropertyEditPlan, PropertyEditError> {
        let preview = prepare_property_edit_text(base, uuid, property, value, self.text)
            .map_err(PropertyEditError::Source)?;
        if preview.path() != &self.source {
            return Err(PropertyEditError::Source(Error {
                code: "source_edit_owner_mismatch",
                location: Vec::new(),
            }));
        }
        Ok(PropertyEditPlan {
            revision: self.revision,
            document_version: self.document_version,
            source_sha256: self.source_sha256,
            preview,
        })
    }
}
