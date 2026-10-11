//! Role resources have current verbatim bytes, editor authority and the same host revision as YAML.

#[cfg(test)]
mod tests;

use std::{collections::BTreeMap, fmt::Write as _, sync::Arc};

use eska::{
    analysis_core::{InputFailure, InputOrigin, InputRequest},
    project_core::source_format::{SourcePath, SourceReader},
};
use sha2::{Digest, Sha256};

use super::AnalysisSession;
use crate::{SessionRequestError, SessionRevision};

/// Readiness describes current bytes; native XML validity belongs to the owning role projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoleRightsState {
    Pending,
    Ready(Arc<str>),
    Unavailable(InputFailure),
}

/// Exact resource bytes and their source fingerprint never inherit the YAML editor's origin.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoleRightsInput {
    pub revision: SessionRevision,
    pub origin: InputOrigin,
    pub source_sha256: Option<String>,
    pub state: RoleRightsState,
}

impl RoleRightsInput {
    /// Available text, including malformed XML, occupies its exact source-byte reservation.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        match &self.state {
            RoleRightsState::Ready(text) => text.len() as u64,
            RoleRightsState::Pending | RoleRightsState::Unavailable(_) => 0,
        }
    }
}

pub(super) struct RoleRightsEntry {
    owner: SourcePath,
    pub(super) input: RoleRightsInput,
}

/// Adopt only the source reader's declared identities without reading any role or resource payload.
pub(super) fn register(reader: &SourceReader) -> BTreeMap<SourcePath, RoleRightsEntry> {
    reader
        .role_rights_paths()
        .filter_map(|path| {
            reader.role_rights_owner(path).map(|owner| {
                (
                    path.clone(),
                    RoleRightsEntry {
                        owner: owner.clone(),
                        input: RoleRightsInput {
                            revision: SessionRevision::default(),
                            origin: InputOrigin::Saved,
                            source_sha256: None,
                            state: RoleRightsState::Pending,
                        },
                    },
                )
            })
        })
        .collect()
}

impl AnalysisSession {
    /// Enumerate current rights resources from the same frozen composition as metadata and BSL.
    pub fn role_rights_paths(&self) -> impl Iterator<Item = &SourcePath> {
        self.role_rights.keys()
    }

    /// Inspect the exact current resource snapshot without reading or projecting other inputs.
    #[must_use]
    pub fn role_rights(&self, path: &SourcePath) -> Option<&RoleRightsInput> {
        self.role_rights.get(path).map(|entry| &entry.input)
    }

    /// Link a declared resource to the role whose current model depends on its exact bytes.
    #[must_use]
    pub fn role_rights_owner(&self, path: &SourcePath) -> Option<&SourcePath> {
        self.role_rights.get(path).map(|entry| &entry.owner)
    }

    /// A metadata read prepares its one role dependency without scanning or refreshing other inputs.
    #[cfg(feature = "analyzer-cli")]
    pub(crate) fn load_pending_role_rights_for_owner(
        &mut self,
        owner: &SourcePath,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        let Some(path) = self.inputs.source_reader().role_rights_path(owner).cloned() else {
            return Ok(());
        };
        if self.role_rights(&path).is_some_and(|input| {
            input.origin == InputOrigin::Saved
                && matches!(
                    input.state,
                    RoleRightsState::Pending
                        | RoleRightsState::Unavailable(InputFailure::Cancelled)
                )
        }) {
            self.load_role_rights_saved(&path, self.revision(), request)?;
        }
        Ok(())
    }

    /// Load current saved bytes; an editor-owned resource cannot be replaced by a disk refresh.
    ///
    /// # Errors
    /// Rejects stale/unknown input, an active overlay or an exhausted clock before any source read.
    pub fn load_role_rights_saved(
        &mut self,
        path: &SourcePath,
        expected: SessionRevision,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        let (next, current) = self.validate_role_rights_update(path, expected)?;
        if let InputOrigin::Overlay { document_version } = current.origin {
            return Err(SessionRequestError::OverlayActive { document_version });
        }
        let request = self.source_budget_request(current.bytes(), request);
        self.read_role_rights_saved(path, next, &request)
    }

    /// Replace only this resource with a newer current editor document and reproject its owner.
    ///
    /// # Errors
    /// Rejects stale/unknown input, an old document version or an exhausted clock before mutation.
    pub fn apply_role_rights_overlay(
        &mut self,
        path: &SourcePath,
        expected: SessionRevision,
        document_version: i64,
        text: &str,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        let (next, current) = self.validate_role_rights_update(path, expected)?;
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
        self.publish_role_rights(
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
    pub fn close_role_rights_overlay(
        &mut self,
        path: &SourcePath,
        expected: SessionRevision,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        let (next, current) = self.validate_role_rights_update(path, expected)?;
        let request = self.source_budget_request(current.bytes(), request);
        self.read_role_rights_saved(path, next, &request)
    }

    /// Invalidate saved resource text without IO; callers preflight batches before this publication.
    pub(super) fn release_role_rights_saved(
        &mut self,
        path: &SourcePath,
        expected: SessionRevision,
    ) -> Result<(), SessionRequestError> {
        let (next, current) = self.validate_role_rights_update(path, expected)?;
        if let InputOrigin::Overlay { document_version } = current.origin {
            return Err(SessionRequestError::OverlayActive { document_version });
        }
        let entry = self
            .role_rights
            .get_mut(path)
            .ok_or_else(|| SessionRequestError::UnknownRoleRights(path.clone()))?;
        self.loaded_role_rights_bytes -= entry.input.bytes();
        entry.input = RoleRightsInput {
            revision: next,
            origin: InputOrigin::Saved,
            source_sha256: None,
            state: RoleRightsState::Pending,
        };
        let owner = entry.owner.clone();
        self.publish(&owner, next)
    }

    /// Shared preparation retries only pending or transiently cancelled saved resources.
    pub(super) fn load_pending_role_rights(
        &mut self,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        let paths = self
            .role_rights
            .iter()
            .filter(|(_, entry)| {
                entry.input.origin == InputOrigin::Saved
                    && matches!(
                        entry.input.state,
                        RoleRightsState::Pending
                            | RoleRightsState::Unavailable(InputFailure::Cancelled)
                    )
            })
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>();
        for path in paths {
            if request.cancellation.is_cancelled() {
                break;
            }
            self.load_role_rights_saved(&path, self.revision(), request)?;
        }
        Ok(())
    }

    /// Reserve the clock and validate a declared identity before bytes, hashes or models can change.
    fn validate_role_rights_update(
        &self,
        path: &SourcePath,
        expected: SessionRevision,
    ) -> Result<(SessionRevision, &RoleRightsInput), SessionRequestError> {
        self.validate_revision(expected)?;
        let input = self
            .role_rights(path)
            .ok_or_else(|| SessionRequestError::UnknownRoleRights(path.clone()))?;
        Ok((self.revision.next()?, input))
    }

    /// The common source reader enforces declared paths, current encoding and the request's remaining quota.
    fn read_role_rights_saved(
        &mut self,
        path: &SourcePath,
        next: SessionRevision,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        let value = if request.cancellation.is_cancelled() {
            Err(InputFailure::Cancelled)
        } else {
            match self
                .inputs
                .source_reader()
                .read_role_rights_text(path, request.source_limit())
            {
                Ok(text) => Ok(text),
                Err(_) if request.cancellation.is_cancelled() => Err(InputFailure::Cancelled),
                Err(error) if error.code == "source_document_too_large" => {
                    Err(request.limit_failure())
                }
                Err(error) => Err(InputFailure::Source(error)),
            }
        };
        self.publish_role_rights(
            path,
            next,
            InputOrigin::Saved,
            value.as_deref().map_err(Clone::clone),
            request,
        )
    }

    /// Publish bytes, origin, source reservation and the role slot under one host revision.
    fn publish_role_rights(
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
            .role_rights
            .get_mut(path)
            .ok_or_else(|| SessionRequestError::UnknownRoleRights(path.clone()))?;
        self.loaded_role_rights_bytes -= entry.input.bytes();
        let (source_sha256, state) = match value {
            Ok((text, hash)) => (Some(hash), RoleRightsState::Ready(text)),
            Err(failure) => (None, RoleRightsState::Unavailable(failure)),
        };
        entry.input = RoleRightsInput {
            revision: next,
            origin,
            source_sha256,
            state,
        };
        self.loaded_role_rights_bytes += entry.input.bytes();
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
