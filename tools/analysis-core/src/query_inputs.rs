//! Declared standalone query text owns current bytes; the host owns revisions and aggregate budgets.

use std::{fmt::Write as _, path::PathBuf, sync::Arc};

use eska::{
    analysis_core::{InputFailure, InputOrigin, InputRequest},
    project_core::source_format::{NativeUuid, SourcePath, SourceReader},
};
use sha2::{Digest, Sha256};

use crate::SessionRevision;

#[path = "query_inputs/syntax.rs"]
mod syntax;
pub use syntax::{QuerySyntaxError, QuerySyntaxResult, QuerySyntaxScope};
mod analysis;
pub use analysis::{QueryAnalysisResult, query_analysis_enabled};

#[cfg(test)]
#[path = "query_inputs/tests.rs"]
mod tests;

/// Identity comes from the reader's frozen declarations, never from a filename or a directory scan.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct QueryTextDeclaration {
    pub source: SourcePath,
    pub owner: SourcePath,
    pub native_descriptor: SourcePath,
    pub native_resource: SourcePath,
    pub uuid: NativeUuid,
}

/// Readable malformed SDBL remains Ready; language errors are diagnostics rather than input failures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueryTextState {
    Pending,
    Ready(Arc<str>),
    Unavailable(InputFailure),
}

/// One exact text, fingerprint and origin belongs to the host revision supplied for its publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryTextSnapshot {
    pub revision: SessionRevision,
    pub origin: InputOrigin,
    pub source_sha256: Option<String>,
    pub state: QueryTextState,
}

impl QueryTextSnapshot {
    /// Only the current readable text reserves source bytes; parser heap and retained results are separate.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        match &self.state {
            QueryTextState::Ready(text) => text.len() as u64,
            QueryTextState::Pending | QueryTextState::Unavailable(_) => 0,
        }
    }
}

/// Hosts account accepted replacement reservations under the same publication as their global revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueryTextChange {
    pub previous_bytes: u64,
    pub current_bytes: u64,
}

/// Validation refusals preserve current text; accepted read, limit and cancellation failures replace it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueryTextError {
    Undeclared(SourcePath),
    ReaderMismatch,
    OverlayActive { document_version: i64 },
    StaleDocumentVersion { current: i64, received: i64 },
}

/// This leaf input registers no `FileId`, BSL wrapper, metadata index, clock or independent cache.
pub struct QueryTextInput {
    root: PathBuf,
    manifest_sha256: String,
    declaration: QueryTextDeclaration,
    snapshot: QueryTextSnapshot,
}

impl QueryTextInput {
    /// Capture declared ownership without reading the query, owner or reference resource.
    ///
    /// # Errors
    /// Rejects a source absent from the verified reader's query map.
    pub fn declare(reader: &SourceReader, source: &SourcePath) -> Result<Self, QueryTextError> {
        Ok(Self {
            root: reader.root().to_path_buf(),
            manifest_sha256: reader.configuration().manifest_sha256.clone(),
            declaration: declaration(reader, source)?,
            snapshot: QueryTextSnapshot {
                revision: SessionRevision::default(),
                origin: InputOrigin::Saved,
                source_sha256: None,
                state: QueryTextState::Pending,
            },
        })
    }

    /// Return frozen ownership independently of whether the physical query currently exists.
    #[must_use]
    pub const fn declaration(&self) -> &QueryTextDeclaration {
        &self.declaration
    }

    /// Inspect the current saved/editor authority and its explicit readability state.
    #[must_use]
    pub const fn snapshot(&self) -> &QueryTextSnapshot {
        &self.snapshot
    }

    /// Expose the old reservation so a host can exclude it when deriving the next shared source budget.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.snapshot.bytes()
    }

    /// Read only the declared query through the caller's same reader and bounded `InputRequest`.
    ///
    /// The caller validates/reserves its host revision and shared budget before this accepted update.
    /// # Errors
    /// Rejects a different composition or active editor buffer without mutating current input.
    pub fn load_saved(
        &mut self,
        reader: &SourceReader,
        revision: SessionRevision,
        request: &InputRequest,
    ) -> Result<QueryTextChange, QueryTextError> {
        self.require_reader(reader)?;
        self.require_saved()?;
        Ok(self.read_saved(reader, revision, request))
    }

    /// Replace current text with a strictly newer editor version, retaining failed editor authority.
    ///
    /// The host owns global revision validation; accepted limit/cancellation failures clear old text.
    /// # Errors
    /// Rejects an old or repeated editor document version before changing bytes, hash or origin.
    pub fn apply_overlay(
        &mut self,
        revision: SessionRevision,
        document_version: i64,
        text: &str,
        request: &InputRequest,
    ) -> Result<QueryTextChange, QueryTextError> {
        if let InputOrigin::Overlay {
            document_version: current,
        } = self.snapshot.origin
            && document_version <= current
        {
            return Err(QueryTextError::StaleDocumentVersion {
                current,
                received: document_version,
            });
        }
        Ok(self.publish(
            revision,
            InputOrigin::Overlay { document_version },
            checked_text(text, request),
            request,
        ))
    }

    /// Restore current saved bytes, including deletion or failure, instead of reviving a saved cache.
    ///
    /// # Errors
    /// Rejects a reader from another root/composition before dropping the editor-owned snapshot.
    pub fn close_overlay(
        &mut self,
        reader: &SourceReader,
        revision: SessionRevision,
        request: &InputRequest,
    ) -> Result<QueryTextChange, QueryTextError> {
        self.require_reader(reader)?;
        Ok(self.read_saved(reader, revision, request))
    }

    /// Release saved text after a syntax batch; current editor input cannot be evicted by disk work.
    ///
    /// # Errors
    /// Rejects an active editor snapshot without changing its reservation or fingerprint.
    pub fn release_saved(
        &mut self,
        revision: SessionRevision,
    ) -> Result<QueryTextChange, QueryTextError> {
        self.require_saved()?;
        let previous_bytes = self.bytes();
        self.snapshot = QueryTextSnapshot {
            revision,
            origin: InputOrigin::Saved,
            source_sha256: None,
            state: QueryTextState::Pending,
        };
        Ok(QueryTextChange {
            previous_bytes,
            current_bytes: 0,
        })
    }

    /// Reopening a composition creates a new input; a borrowed reader cannot silently retarget an old one.
    fn require_reader(&self, reader: &SourceReader) -> Result<(), QueryTextError> {
        if reader.root() != self.root.as_path()
            || reader.configuration().manifest_sha256 != self.manifest_sha256
            || declaration(reader, &self.declaration.source).as_ref() != Ok(&self.declaration)
        {
            return Err(QueryTextError::ReaderMismatch);
        }
        Ok(())
    }

    /// Even an unavailable overlay remains authoritative until the host closes that document.
    const fn require_saved(&self) -> Result<(), QueryTextError> {
        if let InputOrigin::Overlay { document_version } = self.snapshot.origin {
            return Err(QueryTextError::OverlayActive { document_version });
        }
        Ok(())
    }

    /// Current IO failure is accepted input state; no owner or neighboring query is opened by this read.
    fn read_saved(
        &mut self,
        reader: &SourceReader,
        revision: SessionRevision,
        request: &InputRequest,
    ) -> QueryTextChange {
        let value = if request.cancellation.is_cancelled() {
            Err(InputFailure::Cancelled)
        } else {
            match reader.read_query_text(&self.declaration.source, request.source_limit()) {
                Ok(text) => match checked_text(&text, request) {
                    Ok(_) => Ok(text),
                    Err(error) => Err(error),
                },
                Err(_) if request.cancellation.is_cancelled() => Err(InputFailure::Cancelled),
                Err(error) if error.code == "source_document_too_large" => {
                    Err(request.limit_failure())
                }
                Err(error) => Err(InputFailure::Source(error)),
            }
        };
        self.publish(
            revision,
            InputOrigin::Saved,
            value.as_deref().map_err(Clone::clone),
            request,
        )
    }

    /// Hashing and the final cancellation check precede a single replacement of all current fields.
    fn publish(
        &mut self,
        revision: SessionRevision,
        origin: InputOrigin,
        value: Result<&str, InputFailure>,
        request: &InputRequest,
    ) -> QueryTextChange {
        let previous_bytes = self.bytes();
        let value = value.and_then(|text| {
            let hash = Sha256::digest(text.as_bytes()).iter().fold(
                String::with_capacity(64),
                |mut hash, byte| {
                    let _ = write!(hash, "{byte:02x}");
                    hash
                },
            );
            if request.cancellation.is_cancelled() {
                Err(InputFailure::Cancelled)
            } else {
                Ok((Arc::<str>::from(text), hash))
            }
        });
        let (source_sha256, state) = match value {
            Ok((text, hash)) => (Some(hash), QueryTextState::Ready(text)),
            Err(error) => (None, QueryTextState::Unavailable(error)),
        };
        self.snapshot = QueryTextSnapshot {
            revision,
            origin,
            source_sha256,
            state,
        };
        QueryTextChange {
            previous_bytes,
            current_bytes: self.bytes(),
        }
    }
}

/// A declaration records the exact descriptor/resource identities independently of query readability.
fn declaration(
    reader: &SourceReader,
    source: &SourcePath,
) -> Result<QueryTextDeclaration, QueryTextError> {
    let declared = reader
        .declared_query(source)
        .ok_or_else(|| QueryTextError::Undeclared(source.clone()))?;
    Ok(QueryTextDeclaration {
        source: source.clone(),
        owner: declared.owner.clone(),
        native_descriptor: declared.native_descriptor.clone(),
        native_resource: declared.native_resource.clone(),
        uuid: declared.uuid.clone(),
    })
}

/// Saved and editor text obey the same tightened document/source quota and cooperative cancellation.
fn checked_text<'a>(text: &'a str, request: &InputRequest) -> Result<&'a str, InputFailure> {
    if request.cancellation.is_cancelled() {
        Err(InputFailure::Cancelled)
    } else if text.len() as u64 > request.source_limit() {
        Err(request.limit_failure())
    } else {
        Ok(text)
    }
}
