//! A session token covers every accepted YAML and BSL update, including current failures.

use eska::{analysis_core::UpdateError, project_core::source_format::SourcePath};

/// A transport must pair this generation with its own session identity after a reopen.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize)]
pub struct SessionRevision(pub(super) u64);

impl SessionRevision {
    /// Reserve the next revision before accepting text or performing a disk read.
    pub(super) fn next(self) -> Result<Self, SessionRequestError> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or(SessionRequestError::RevisionExhausted)
    }
}

/// Rejected requests leave current bytes, input origins and kernel state intact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionRequestError {
    StaleRevision {
        expected: SessionRevision,
        actual: SessionRevision,
    },
    Metadata(UpdateError),
    UnknownModule(SourcePath),
    BinaryModule(SourcePath),
    StaleDocumentVersion {
        current: i64,
        received: i64,
    },
    OverlayActive {
        document_version: i64,
    },
    RevisionExhausted,
}

impl From<UpdateError> for SessionRequestError {
    /// Retain structured controller failures without turning them into localized strings.
    fn from(value: UpdateError) -> Self {
        Self::Metadata(value)
    }
}
