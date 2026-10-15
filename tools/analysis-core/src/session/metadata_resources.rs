//! Declared metadata resources share current bytes, editor authority and one host revision.

#[cfg(test)]
mod form_tests;
#[cfg(test)]
mod predefined_tests;
#[cfg(test)]
mod role_tests;

use std::{collections::BTreeMap, fmt::Write as _, sync::Arc};

use eska::{
    analysis_core::{InputFailure, InputOrigin, InputRequest},
    project_core::source_format::{FileRole, SourcePath, SourceReader},
};
use sha2::{Digest, Sha256};

mod rooted;

use super::AnalysisSession;
use crate::{SessionRequestError, SessionRevision};

/// Only resource families consumed by current canonical projections are registered.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataResourceKind {
    RoleRights,
    FormDialog,
    PredefinedData,
    QueryComposition,
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
            reader.form_dialog_owner(path).and_then(|owner| {
                (owner != path).then_some((path, owner, MetadataResourceKind::FormDialog))
            })
        }))
        .chain(reader.predefined_paths().filter_map(|path| {
            reader
                .predefined_owner(path)
                .map(|owner| (path, owner, MetadataResourceKind::PredefinedData))
        }))
        .chain(composition_resources(reader))
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

/// Distinct DCS authoring XML is declared by the manifest; unified forms use their owner snapshot.
fn composition_resources(
    reader: &SourceReader,
) -> impl Iterator<Item = (&SourcePath, &SourcePath, MetadataResourceKind)> {
    reader
        .manifest()
        .query_resources
        .iter()
        .filter_map(|resource| {
            let file = reader
                .manifest()
                .files
                .iter()
                .find(|file| file.path == resource.path)?;
            if matches!(file.role, FileRole::ManagedForm { .. }) {
                return None;
            }
            let owner = reader.owner_for_native_main(&resource.descriptor)?;
            Some((
                file.source_path(),
                owner,
                MetadataResourceKind::QueryComposition,
            ))
        })
}

/// The primary slot consumes its declared rights, main dialog or predefined-data resource.
pub(super) fn dependency<'a>(
    reader: &'a SourceReader,
    owner: &SourcePath,
) -> Option<&'a SourcePath> {
    reader
        .role_rights_path(owner)
        .or_else(|| reader.form_dialog_path(owner))
        .or_else(|| reader.predefined_path(owner))
}

impl AnalysisSession {
    /// Enumerate resources from the same frozen composition as metadata and BSL.
    pub fn resource_paths(&self) -> impl Iterator<Item = &SourcePath> {
        self.selected_root.metadata_resources.keys()
    }

    /// Inspect the exact current resource snapshot without reading or projecting other inputs.
    #[must_use]
    pub fn metadata_resource(&self, path: &SourcePath) -> Option<&MetadataResourceInput> {
        self.selected_root
            .metadata_resources
            .get(path)
            .map(|entry| &entry.input)
    }

    /// Link a resource to the owner whose current model depends on its exact bytes.
    #[must_use]
    pub fn resource_owner(&self, path: &SourcePath) -> Option<&SourcePath> {
        self.selected_root
            .metadata_resources
            .get(path)
            .map(|entry| &entry.owner)
    }

    /// Prepare the selected owner's dependency without scanning or refreshing other inputs.
    #[cfg(feature = "analyzer-cli")]
    pub(crate) fn load_pending_resource_for_owner(
        &mut self,
        owner: &SourcePath,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        let paths = dependency(self.selected_root.inputs.source_reader(), owner)
            .into_iter()
            .chain(
                self.selected_root
                    .owners
                    .get(owner)
                    .into_iter()
                    .flat_map(|entry| {
                        entry
                            .forms
                            .values()
                            .filter_map(|entry| entry.model.dialog.as_ref())
                    }),
            )
            .cloned()
            .collect::<Vec<_>>();
        for path in paths {
            if request.cancellation.is_cancelled() {
                break;
            }
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
        self.load_resource_saved_at(&self.selected_source(path), expected, request)
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
        self.apply_resource_overlay_at(
            &self.selected_source(path),
            expected,
            document_version,
            text,
            request,
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
        self.close_resource_overlay_at(&self.selected_source(path), expected, request)
    }

    /// Shared preparation retries only pending or transiently cancelled saved resources.
    pub(super) fn load_pending_resources(
        &mut self,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        let paths = self
            .selected_root
            .metadata_resources
            .iter()
            .filter(|(_, entry)| {
                entry.input.kind != MetadataResourceKind::QueryComposition
                    && entry.input.origin == InputOrigin::Saved
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
