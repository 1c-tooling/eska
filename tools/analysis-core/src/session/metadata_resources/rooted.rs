//! Root-qualified resource updates project only their own current owner.

use super::*;
use crate::composition::SourceKey;

impl AnalysisSession {
    /// Resource ownership remains local to the root whose current bytes were changed.
    #[must_use]
    pub fn resource_owner_at(&self, key: &SourceKey) -> Option<&SourcePath> {
        self.root_inputs(key.root)
            .ok()?
            .metadata_resources
            .get(&key.source)
            .map(|entry| &entry.owner)
    }

    /// Current resource authority and bounds are shared across all root-qualified inputs.
    ///
    /// # Errors
    /// Rejects stale or unknown input and conflicting editor versions before publication.
    pub fn load_resource_saved_at(
        &mut self,
        key: &SourceKey,
        expected: SessionRevision,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        let (next, current) = self.validate_resource_update_at(key, expected)?;
        if let InputOrigin::Overlay { document_version } = current.origin {
            return Err(SessionRequestError::OverlayActive { document_version });
        }
        let request = self.source_budget_request(current.bytes(), request);
        self.read_resource_saved_at(key, next, &request)
    }

    /// Current resource authority and bounds are shared across all root-qualified inputs.
    ///
    /// # Errors
    /// Rejects stale or unknown input and conflicting editor versions before publication.
    pub fn apply_resource_overlay_at(
        &mut self,
        key: &SourceKey,
        expected: SessionRevision,
        document_version: i64,
        text: &str,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        let (next, current) = self.validate_resource_update_at(key, expected)?;
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
        self.publish_resource_at(
            key,
            next,
            InputOrigin::Overlay { document_version },
            value,
            &request,
        )
    }

    /// Current resource authority and bounds are shared across all root-qualified inputs.
    ///
    /// # Errors
    /// Rejects stale or unknown input and conflicting editor versions before publication.
    pub fn close_resource_overlay_at(
        &mut self,
        key: &SourceKey,
        expected: SessionRevision,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        let (next, current) = self.validate_resource_update_at(key, expected)?;
        let request = self.source_budget_request(current.bytes(), request);
        self.read_resource_saved_at(key, next, &request)
    }

    /// Keep this resource and its canonical owner in the same root and host publication.
    pub(crate) fn release_resource_saved_at(
        &mut self,
        key: &SourceKey,
        expected: SessionRevision,
    ) -> Result<(), SessionRequestError> {
        let (next, current) = self.validate_resource_update_at(key, expected)?;
        if let InputOrigin::Overlay { document_version } = current.origin {
            return Err(SessionRequestError::OverlayActive { document_version });
        }
        let root = self.root_inputs_mut(key.root)?;
        let entry = root
            .metadata_resources
            .get_mut(&key.source)
            .ok_or_else(|| SessionRequestError::UnknownMetadataResource(key.source.clone()))?;
        let previous = entry.input.bytes();
        entry.input = MetadataResourceInput {
            kind: entry.input.kind,
            revision: next,
            origin: InputOrigin::Saved,
            source_sha256: None,
            state: MetadataResourceState::Pending,
        };
        let owner = SourceKey {
            root: key.root,
            source: entry.owner.clone(),
        };
        self.loaded_resource_bytes -= previous;
        self.publish_at(&owner, next)
    }

    /// Keep this resource and its canonical owner in the same root and host publication.
    fn validate_resource_update_at(
        &self,
        key: &SourceKey,
        expected: SessionRevision,
    ) -> Result<(SessionRevision, &MetadataResourceInput), SessionRequestError> {
        let path = &key.source;
        self.validate_revision(expected)?;
        let input = self
            .metadata_resource_at(key)
            .ok_or_else(|| SessionRequestError::UnknownMetadataResource(path.clone()))?;
        Ok((self.revision.next()?, input))
    }

    /// Keep this resource and its canonical owner in the same root and host publication.
    fn read_resource_saved_at(
        &mut self,
        key: &SourceKey,
        next: SessionRevision,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        let path = &key.source;
        let value = if request.cancellation.is_cancelled() {
            Err(InputFailure::Cancelled)
        } else {
            let reader = self.root_inputs(key.root)?.inputs.source_reader();
            let kind = self
                .metadata_resource_at(key)
                .ok_or_else(|| SessionRequestError::UnknownMetadataResource(path.clone()))?
                .kind;
            let text = match kind {
                MetadataResourceKind::RoleRights => {
                    reader.read_role_rights_text(path, request.source_limit())
                }
                MetadataResourceKind::FormDialog => {
                    reader.read_form_dialog_text(path, request.source_limit())
                }
                MetadataResourceKind::PredefinedData => {
                    reader.read_predefined_text(path, request.source_limit())
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
        self.publish_resource_at(
            key,
            next,
            InputOrigin::Saved,
            value.as_deref().map_err(Clone::clone),
            request,
        )
    }

    /// Keep this resource and its canonical owner in the same root and host publication.
    fn publish_resource_at(
        &mut self,
        key: &SourceKey,
        next: SessionRevision,
        origin: InputOrigin,
        value: Result<&str, InputFailure>,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        let path = &key.source;
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
        let root = self.root_inputs_mut(key.root)?;
        let entry = root
            .metadata_resources
            .get_mut(path)
            .ok_or_else(|| SessionRequestError::UnknownMetadataResource(path.clone()))?;
        let previous = entry.input.bytes();
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
        let current = entry.input.bytes();
        let owner = SourceKey {
            root: key.root,
            source: entry.owner.clone(),
        };
        self.loaded_resource_bytes = self.loaded_resource_bytes - previous + current;
        self.publish_at(&owner, next)
    }
}
