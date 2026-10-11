//! Declared metadata resources share current bytes, editor authority and one host revision.

#[cfg(test)]
mod form_tests;
#[cfg(test)]
mod role_tests;

use std::{collections::BTreeMap, fmt::Write as _, sync::Arc};

use eska::{
    analysis_core::{InputFailure, InputOrigin, InputRequest},
    project_core::source_format::{SourcePath, SourceReader},
};
use sha2::{Digest, Sha256};

use super::AnalysisSession;
use crate::{SessionRequestError, SessionRevision};

/// Only resource families consumed by current canonical projections are registered.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataResourceKind {
    RoleRights,
    FormDialog,
}

/// Readiness describes current bytes; native XML validity belongs to the owning projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MetadataResourceState {
    Pending,
    Ready(Arc<str>),
    Unavailable(InputFailure),
}

/// Exact resource bytes and their source fingerprint never inherit the YAML editor's origin.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataResourceInput {
    pub kind: MetadataResourceKind,
    pub revision: SessionRevision,
    pub origin: InputOrigin,
    pub source_sha256: Option<String>,
    pub state: MetadataResourceState,
}

impl MetadataResourceInput {
    /// Available text, including malformed XML, occupies its exact source-byte reservation.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        match &self.state {
            MetadataResourceState::Ready(text) => text.len() as u64,
            MetadataResourceState::Pending | MetadataResourceState::Unavailable(_) => 0,
        }
    }
}

pub(super) struct MetadataResourceEntry {
    owner: SourcePath,
    pub(super) input: MetadataResourceInput,
}

/// Adopt the reader's declared identities without reading metadata or resource payloads.
pub(super) fn register(reader: &SourceReader) -> BTreeMap<SourcePath, MetadataResourceEntry> {
    reader
        .role_rights_paths()
        .filter_map(|path| {
            reader
                .role_rights_owner(path)
                .map(|owner| (path, owner, MetadataResourceKind::RoleRights))
        })
        .chain(reader.form_dialog_paths().filter_map(|path| {
            reader
                .form_dialog_owner(path)
                .filter(|owner| reader.form_dialog_path(owner) == Some(path))
                .map(|owner| (path, owner, MetadataResourceKind::FormDialog))
        }))
        .map(|(path, owner, kind)| {
            (
                path.clone(),
                MetadataResourceEntry {
                    owner: owner.clone(),
                    input: MetadataResourceInput {
                        kind,
                        revision: SessionRevision::default(),
                        origin: InputOrigin::Saved,
                        source_sha256: None,
                        state: MetadataResourceState::Pending,
                    },
                },
            )
        })
        .collect()
}

/// An owner currently consumes at most one declared role or common-form resource.
pub(super) fn dependency<'a>(
    reader: &'a SourceReader,
    owner: &SourcePath,
) -> Option<&'a SourcePath> {
    reader
        .role_rights_path(owner)
        .or_else(|| reader.form_dialog_path(owner))
}

impl AnalysisSession {
    /// Enumerate resources from the same frozen composition as metadata and BSL.
    pub fn resource_paths(&self) -> impl Iterator<Item = &SourcePath> {
        self.metadata_resources.keys()
    }

    /// Inspect the exact current resource snapshot without reading or projecting other inputs.
    #[must_use]
    pub fn metadata_resource(&self, path: &SourcePath) -> Option<&MetadataResourceInput> {
        self.metadata_resources.get(path).map(|entry| &entry.input)
    }

    /// Link a resource to the owner whose current model depends on its exact bytes.
    #[must_use]
    pub fn resource_owner(&self, path: &SourcePath) -> Option<&SourcePath> {
        self.metadata_resources.get(path).map(|entry| &entry.owner)
    }

    /// Prepare the selected owner's dependency without scanning or refreshing other inputs.
    #[cfg(feature = "analyzer-cli")]
    pub(crate) fn load_pending_resource_for_owner(
        &mut self,
        owner: &SourcePath,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        let Some(path) = dependency(self.inputs.source_reader(), owner).cloned() else {
            return Ok(());
        };
        if self.metadata_resource(&path).is_some_and(|input| {
            input.origin == InputOrigin::Saved
                && matches!(
                    input.state,
                    MetadataResourceState::Pending
                        | MetadataResourceState::Unavailable(InputFailure::Cancelled)
                )
        }) {
            self.load_resource_saved(&path, self.revision(), request)?;
        }
        Ok(())
    }

    /// Load current saved bytes; an editor-owned resource cannot be replaced by a disk refresh.
    ///
    /// # Errors
    /// Rejects stale/unknown input, an active overlay or an exhausted clock before any source read.
    pub fn load_resource_saved(
        &mut self,
        path: &SourcePath,
        expected: SessionRevision,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        let (next, current) = self.validate_resource_update(path, expected)?;
        if let InputOrigin::Overlay { document_version } = current.origin {
            return Err(SessionRequestError::OverlayActive { document_version });
        }
        let request = self.source_budget_request(current.bytes(), request);
        self.read_resource_saved(path, next, &request)
    }

    /// Replace only this resource with a newer current editor document and reproject its owner.
    ///
    /// # Errors
    /// Rejects stale/unknown input, an old document version or an exhausted clock before mutation.
    pub fn apply_resource_overlay(
        &mut self,
        path: &SourcePath,
        expected: SessionRevision,
        document_version: i64,
        text: &str,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        let (next, current) = self.validate_resource_update(path, expected)?;
        if let InputOrigin::Overlay {
            document_version: current,
        } = current.origin
            && document_version <= current
        {
            return Err(SessionRequestError::StaleDocumentVersion {
                current,
                received: document_version,
            });
        }
        let request = self.source_budget_request(current.bytes(), request);
        let value = checked_text(text, &request);
        self.publish_resource(
            path,
            next,
            InputOrigin::Overlay { document_version },
            value,
            &request,
        )
    }

    /// Restore current disk bytes, including deletion or failure, instead of reviving a saved cache.
    ///
    /// # Errors
    /// Rejects stale/unknown input or an exhausted clock before the bounded saved read.
    pub fn close_resource_overlay(
        &mut self,
        path: &SourcePath,
        expected: SessionRevision,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        let (next, current) = self.validate_resource_update(path, expected)?;
        let request = self.source_budget_request(current.bytes(), request);
        self.read_resource_saved(path, next, &request)
    }

    /// Invalidate saved resource text without IO; callers preflight batches before this publication.
    pub(super) fn release_resource_saved(
        &mut self,
        path: &SourcePath,
        expected: SessionRevision,
    ) -> Result<(), SessionRequestError> {
        let (next, current) = self.validate_resource_update(path, expected)?;
        if let InputOrigin::Overlay { document_version } = current.origin {
            return Err(SessionRequestError::OverlayActive { document_version });
        }
        let entry = self
            .metadata_resources
            .get_mut(path)
            .ok_or_else(|| SessionRequestError::UnknownMetadataResource(path.clone()))?;
        self.loaded_resource_bytes -= entry.input.bytes();
        entry.input = MetadataResourceInput {
            kind: entry.input.kind,
            revision: next,
            origin: InputOrigin::Saved,
            source_sha256: None,
            state: MetadataResourceState::Pending,
        };
        let owner = entry.owner.clone();
        self.publish(&owner, next)
    }

    /// Shared preparation retries only pending or transiently cancelled saved resources.
    pub(super) fn load_pending_resources(
        &mut self,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        let paths = self
            .metadata_resources
            .iter()
            .filter(|(_, entry)| {
                entry.input.origin == InputOrigin::Saved
                    && matches!(
                        entry.input.state,
                        MetadataResourceState::Pending
                            | MetadataResourceState::Unavailable(InputFailure::Cancelled)
                    )
            })
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>();
        for path in paths {
            if request.cancellation.is_cancelled() {
                break;
            }
            self.load_resource_saved(&path, self.revision(), request)?;
        }
        Ok(())
    }

    /// Reserve the clock and validate a declared identity before bytes, hashes or models can change.
    fn validate_resource_update(
        &self,
        path: &SourcePath,
        expected: SessionRevision,
    ) -> Result<(SessionRevision, &MetadataResourceInput), SessionRequestError> {
        self.validate_revision(expected)?;
        let input = self
            .metadata_resource(path)
            .ok_or_else(|| SessionRequestError::UnknownMetadataResource(path.clone()))?;
        Ok((self.revision.next()?, input))
    }

    /// The common source reader enforces declared paths, current encoding and the request's remaining quota.
    fn read_resource_saved(
        &mut self,
        path: &SourcePath,
        next: SessionRevision,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        let value = if request.cancellation.is_cancelled() {
            Err(InputFailure::Cancelled)
        } else {
            let reader = self.inputs.source_reader();
            let kind = self
                .metadata_resource(path)
                .ok_or_else(|| SessionRequestError::UnknownMetadataResource(path.clone()))?
                .kind;
            let text = match kind {
                MetadataResourceKind::RoleRights => {
                    reader.read_role_rights_text(path, request.source_limit())
                }
                MetadataResourceKind::FormDialog => {
                    reader.read_form_dialog_text(path, request.source_limit())
                }
            };
            match text {
                Ok(text) => Ok(text),
                Err(_) if request.cancellation.is_cancelled() => Err(InputFailure::Cancelled),
                Err(error) if error.code == "source_document_too_large" => {
                    Err(request.limit_failure())
                }
                Err(error) => Err(InputFailure::Source(error)),
            }
        };
        self.publish_resource(
            path,
            next,
            InputOrigin::Saved,
            value.as_deref().map_err(Clone::clone),
            request,
        )
    }

    /// Publish bytes, origin, source reservation and the owner slot under one host revision.
    fn publish_resource(
        &mut self,
        path: &SourcePath,
        next: SessionRevision,
        origin: InputOrigin,
        value: Result<&str, InputFailure>,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
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
        let entry = self
            .metadata_resources
            .get_mut(path)
            .ok_or_else(|| SessionRequestError::UnknownMetadataResource(path.clone()))?;
        self.loaded_resource_bytes -= entry.input.bytes();
        let (source_sha256, state) = match value {
            Ok((text, hash)) => (Some(hash), MetadataResourceState::Ready(text)),
            Err(failure) => (None, MetadataResourceState::Unavailable(failure)),
        };
        entry.input = MetadataResourceInput {
            kind: entry.input.kind,
            revision: next,
            origin,
            source_sha256,
            state,
        };
        self.loaded_resource_bytes += entry.input.bytes();
        let owner = entry.owner.clone();
        self.publish(&owner, next)
    }
}

/// Both saved and editor text obey cancellation and the exact same current-source bounds.
fn checked_text<'a>(text: &'a str, request: &InputRequest) -> Result<&'a str, InputFailure> {
    if request.cancellation.is_cancelled() {
        Err(InputFailure::Cancelled)
    } else if text.len() as u64 > request.source_limit() {
        Err(request.limit_failure())
    } else {
        Ok(text)
    }
}
