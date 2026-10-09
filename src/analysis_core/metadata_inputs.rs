//! Saved and unsaved metadata share one parser; invalid current inputs never expose old models.

#[cfg(test)]
mod tests;

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use crate::project_core::{
    metadata_parser::PropertiesMode,
    source_format::{Error, MetadataSnapshot, SourcePath, SourceReader},
};
use sha2::{Digest, Sha256};

const MAX_DOCUMENT_BYTES: u64 = 64 * 1024 * 1024;

/// A token belongs to one live input controller; external sessions must also carry their host ID.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize)]
pub struct InputRevision(u64);

/// Cancellation is shared with the request's caller and checked before and after bounded work.
#[derive(Clone, Debug, Default)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    /// Cancel this request and all holders of the same token; cancellation cannot be reset.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// Read a cancellation flag; it does not publish any other memory between threads.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Limits apply to one source document, not to process RSS or the kernel's retained caches.
#[derive(Clone, Debug)]
pub struct InputRequest {
    pub max_document_bytes: u64,
    pub cancellation: Cancellation,
    pub source_budget: Option<SourceBudget>,
}

/// A host reserves current source bytes, which are not a measurement of parsed heap or RSS.
#[derive(Clone, Copy, Debug)]
pub struct SourceBudget {
    pub available_bytes: u64,
    pub max_bytes: u64,
}

impl Default for InputRequest {
    /// Match the source format's hard bound unless the consumer requests a smaller limit.
    fn default() -> Self {
        Self {
            max_document_bytes: MAX_DOCUMENT_BYTES,
            cancellation: Cancellation::default(),
            source_budget: None,
        }
    }
}

impl InputRequest {
    /// Preserve a caller's stricter reservation while imposing the host's current remaining quota.
    #[must_use]
    pub fn with_source_budget(&self, available_bytes: u64, max_bytes: u64) -> Self {
        let budget = SourceBudget {
            available_bytes: available_bytes.min(max_bytes),
            max_bytes,
        };
        let mut request = self.clone();
        if request.source_budget.is_none_or(|current| {
            budget.available_bytes < current.available_bytes.min(current.max_bytes)
        }) {
            request.source_budget = Some(budget);
        }
        request
    }

    /// Per-document and remaining-source bounds apply before reading or parsing exact UTF-8 bytes.
    #[must_use]
    pub fn source_limit(&self) -> u64 {
        let limit = self.max_document_bytes.min(MAX_DOCUMENT_BYTES);
        self.source_budget.map_or(limit, |budget| {
            limit.min(budget.available_bytes.min(budget.max_bytes))
        })
    }

    /// Classify the stricter bound without turning source accounting into a process memory claim.
    #[must_use]
    pub fn limit_failure(&self) -> InputFailure {
        let document_limit = self.max_document_bytes.min(MAX_DOCUMENT_BYTES);
        if let Some(budget) = self.source_budget
            && budget.available_bytes.min(budget.max_bytes) < document_limit
        {
            InputFailure::SourceBudgetExceeded {
                limit: budget.max_bytes,
            }
        } else {
            InputFailure::DocumentTooLarge {
                limit: document_limit,
            }
        }
    }
}

/// Disk refreshes must not silently replace an open editor's current text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputOrigin {
    Saved,
    Overlay { document_version: i64 },
}

/// Failure describes the current revision; it is never a successful empty metadata model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InputFailure {
    Source(Error),
    Cancelled,
    DocumentTooLarge { limit: u64 },
    SourceBudgetExceeded { limit: u64 },
}

/// Ready means parsed input, not full BSL analysis; recoverable parser issues remain in the model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InputState {
    Pending,
    Ready(Arc<MetadataSnapshot>),
    Unavailable(InputFailure),
}

/// A model's exact source fingerprint and coordinates travel together under one owner revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnerInput {
    pub revision: InputRevision,
    pub origin: InputOrigin,
    /// None means the current bytes were not read completely; invalid bounded YAML still has a hash.
    pub source_sha256: Option<String>,
    pub state: InputState,
}

/// Input completeness is separate from the adapter's type coverage and semantic check completeness.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize)]
pub struct InputSummary {
    pub revision: InputRevision,
    pub owners: usize,
    pub pending: usize,
    pub ready: usize,
    pub unavailable: usize,
    pub parse_issues: usize,
    pub metadata_input_complete: bool,
}

/// Rejected protocol updates leave the current state intact; accepted invalid text replaces it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UpdateError {
    StaleRevision {
        expected: InputRevision,
        actual: InputRevision,
    },
    UnknownOwner(SourcePath),
    StaleDocumentVersion {
        current: i64,
        received: i64,
    },
    OverlayActive {
        document_version: i64,
    },
    RevisionExhausted,
}

/// The declared structure is immutable until reopening; updates touch exactly one owner.
pub struct MetadataInputs {
    reader: SourceReader,
    revision: InputRevision,
    owners: BTreeMap<SourcePath, OwnerInput>,
}

impl MetadataInputs {
    /// Open only the manifest and configuration, registering every other owner as pending.
    ///
    /// # Errors
    /// Returns the common reader's opening failures. No platform, Git or generated XML is used.
    pub fn open(root: &Path) -> Result<Self, Error> {
        let reader = SourceReader::open(root)?;
        Ok(Self::from_reader(reader))
    }

    /// Adopt the already validated project composition instead of reopening mutable disk input.
    #[must_use]
    pub fn from_reader(reader: SourceReader) -> Self {
        let revision = InputRevision::default();
        let owners = reader
            .owner_paths()
            .map(|path| {
                (
                    path.clone(),
                    OwnerInput {
                        revision,
                        origin: InputOrigin::Saved,
                        source_sha256: None,
                        state: InputState::Pending,
                    },
                )
            })
            .collect();
        Self {
            reader,
            revision,
            owners,
        }
    }

    /// Return the current request token; a result is usable only for this controller revision.
    #[must_use]
    pub const fn revision(&self) -> InputRevision {
        self.revision
    }

    /// Bind all owner revisions to the same immutable manifest snapshot.
    #[must_use]
    pub fn manifest_sha256(&self) -> &str {
        &self.reader.configuration().manifest_sha256
    }

    /// Share the exact immutable structure used by this controller, avoiding a second open race.
    /// Current metadata values must still come from `owner`, which respects active overlays.
    #[must_use]
    pub const fn source_reader(&self) -> &SourceReader {
        &self.reader
    }

    /// Enumerate only manifest-declared owners; no directory naming heuristic activates files.
    pub fn owner_paths(&self) -> impl Iterator<Item = &SourcePath> {
        self.owners.keys()
    }

    /// Read one current input; retaining an Arc preserves an old model without making it current.
    #[must_use]
    pub fn owner(&self, path: &SourcePath) -> Option<&OwnerInput> {
        self.owners.get(path)
    }

    /// Report unread, failed and recoverably partial input instead of treating it as clean.
    #[must_use]
    pub fn summary(&self) -> InputSummary {
        let mut result = InputSummary {
            revision: self.revision,
            owners: self.owners.len(),
            ..InputSummary::default()
        };
        for owner in self.owners.values() {
            match &owner.state {
                InputState::Pending => result.pending += 1,
                InputState::Unavailable(_) => result.unavailable += 1,
                InputState::Ready(snapshot) => {
                    result.ready += 1;
                    result.parse_issues += snapshot
                        .documents
                        .iter()
                        .map(|document| document.descriptor.diagnostics.len())
                        .sum::<usize>();
                }
            }
        }
        result.metadata_input_complete =
            result.pending == 0 && result.unavailable == 0 && result.parse_issues == 0;
        result
    }

    /// Refresh saved bytes without discarding an active overlay; an I/O failure becomes current.
    ///
    /// # Errors
    /// Returns a stale request, unknown owner, active overlay or exhausted revision counter.
    pub fn load_saved(
        &mut self,
        path: &SourcePath,
        expected: InputRevision,
        request: &InputRequest,
    ) -> Result<(), UpdateError> {
        let owner = self.validate_update(path, expected)?;
        if let InputOrigin::Overlay { document_version } = owner.origin {
            return Err(UpdateError::OverlayActive { document_version });
        }
        self.read_saved(path, request)
    }

    /// Explicitly close an editor overlay and read current disk bytes, including disk failures.
    ///
    /// # Errors
    /// Returns a stale request, unknown owner or exhausted revision counter.
    pub fn close_overlay(
        &mut self,
        path: &SourcePath,
        expected: InputRevision,
        request: &InputRequest,
    ) -> Result<(), UpdateError> {
        self.validate_update(path, expected)?;
        self.read_saved(path, request)
    }

    /// Accept a newer document version, replacing the current model even when its text is invalid.
    ///
    /// # Errors
    /// Returns a stale request/document, unknown owner or exhausted revision counter.
    pub fn apply_overlay(
        &mut self,
        path: &SourcePath,
        expected: InputRevision,
        document_version: i64,
        text: &str,
        request: &InputRequest,
    ) -> Result<(), UpdateError> {
        let owner = self.validate_update(path, expected)?;
        if let InputOrigin::Overlay {
            document_version: current,
        } = owner.origin
            && document_version <= current
        {
            return Err(UpdateError::StaleDocumentVersion {
                current,
                received: document_version,
            });
        }
        let (hash, state) = self.parse(path, text, request);
        self.publish(path, InputOrigin::Overlay { document_version }, hash, state)
    }

    /// Validate protocol identity before doing I/O or accepting source bytes.
    fn validate_update(
        &self,
        path: &SourcePath,
        expected: InputRevision,
    ) -> Result<&OwnerInput, UpdateError> {
        if self.revision != expected {
            return Err(UpdateError::StaleRevision {
                expected,
                actual: self.revision,
            });
        }
        if self.revision.0 == u64::MAX {
            return Err(UpdateError::RevisionExhausted);
        }
        self.owners
            .get(path)
            .ok_or_else(|| UpdateError::UnknownOwner(path.clone()))
    }

    /// Bound the selected disk read before allocation; cancellation never returns an older model.
    fn read_saved(&mut self, path: &SourcePath, request: &InputRequest) -> Result<(), UpdateError> {
        let (hash, state) = if request.cancellation.is_cancelled() {
            (None, InputState::Unavailable(InputFailure::Cancelled))
        } else {
            match self.reader.read_owner_text(path, request.source_limit()) {
                Ok(text) => self.parse(path, &text, request),
                Err(error) => (
                    None,
                    InputState::Unavailable(Self::source_failure(error, request)),
                ),
            }
        };
        self.publish(path, InputOrigin::Saved, hash, state)
    }

    /// Both saved and overlay text pass through the same exact semantic projection.
    fn parse(
        &self,
        path: &SourcePath,
        text: &str,
        request: &InputRequest,
    ) -> (Option<String>, InputState) {
        if request.cancellation.is_cancelled() {
            return (None, InputState::Unavailable(InputFailure::Cancelled));
        }
        if text.len() as u64 > request.source_limit() {
            return (None, InputState::Unavailable(request.limit_failure()));
        }
        let hash = Some(Sha256::digest(text.as_bytes()).iter().fold(
            String::with_capacity(64),
            |mut hash, byte| {
                let _ = write!(hash, "{byte:02x}");
                hash
            },
        ));
        let parsed = self.reader.parse_metadata(path, text, PropertiesMode::All);
        if request.cancellation.is_cancelled() {
            return (hash, InputState::Unavailable(InputFailure::Cancelled));
        }
        let state = match parsed {
            Ok(snapshot) => InputState::Ready(Arc::new(snapshot)),
            Err(error) => InputState::Unavailable(Self::source_failure(error, request)),
        };
        (hash, state)
    }

    /// Disk and overlays report the same limit failure; cancellation takes precedence after I/O.
    fn source_failure(error: Error, request: &InputRequest) -> InputFailure {
        if request.cancellation.is_cancelled() {
            InputFailure::Cancelled
        } else if error.code == "source_document_too_large" {
            request.limit_failure()
        } else {
            InputFailure::Source(error)
        }
    }

    /// Publish a fully evaluated state atomically under the next controller revision.
    fn publish(
        &mut self,
        path: &SourcePath,
        origin: InputOrigin,
        source_sha256: Option<String>,
        state: InputState,
    ) -> Result<(), UpdateError> {
        let revision = InputRevision(
            self.revision
                .0
                .checked_add(1)
                .ok_or(UpdateError::RevisionExhausted)?,
        );
        let owner = self
            .owners
            .get_mut(path)
            .ok_or_else(|| UpdateError::UnknownOwner(path.clone()))?;
        *owner = OwnerInput {
            revision,
            origin,
            source_sha256,
            state,
        };
        self.revision = revision;
        Ok(())
    }
}
