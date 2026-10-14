//! A session token covers accepted YAML, role rights and BSL updates, including current failures.

use eska::{analysis_core::UpdateError, project_core::source_format::SourcePath};
use std::fmt;

/// A transport must pair this generation with its own session identity after a reopen.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize)]
pub struct SessionRevision(pub(super) u64);

impl fmt::Display for SessionRevision {
    /// A decimal token can round-trip through clients without losing a 64-bit JSON number.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

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
    ContextRequired,
    ConfigurationContextRequired,
    StaleRevision {
        expected: SessionRevision,
        actual: SessionRevision,
    },
    Metadata(UpdateError),
    UnknownModule(SourcePath),
    UnknownMetadataResource(SourcePath),
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
