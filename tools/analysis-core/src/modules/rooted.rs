//! Root-qualified BSL publications retain one database, quota and host revision.

use super::*;
use crate::composition::SourceKey;

impl AnalysisSession {
    /// Root qualification preserves current text authority under the shared host quota and clock.
    ///
    /// # Errors
    /// Rejects stale, undeclared or binary input and conflicting editor authority before publication.
    pub fn load_module_saved_at(
        &mut self,
        key: &SourceKey,
        expected: SessionRevision,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        let (next, current) = self.validate_module_update_at(key, expected)?;
        if let InputOrigin::Overlay { document_version } = current.origin {
            return Err(SessionRequestError::OverlayActive { document_version });
        }
        let request = self.source_budget_request(current.bytes(), request);
        let limits = self.module_limit(current.bytes(), &request);
        self.read_module_saved_at(key, next, &request, limits)
    }

    /// Root qualification preserves current text authority under the shared host quota and clock.
    ///
    /// # Errors
    /// Rejects stale, undeclared or binary input and conflicting editor authority before publication.
    pub fn apply_module_overlay_at(
        &mut self,
        key: &SourceKey,
        expected: SessionRevision,
        document_version: i64,
        text: &str,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        let (next, current) = self.validate_module_update_at(key, expected)?;
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
        let limits = self.module_limit(current.bytes(), &request);
        let value = self.check_module_text(text, &request, limits);
        self.publish_module_at(
            key,
            next,
            InputOrigin::Overlay { document_version },
            value,
            &request,
        )
    }

    /// Root qualification preserves current text authority under the shared host quota and clock.
    ///
    /// # Errors
    /// Rejects stale, undeclared or binary input and conflicting editor authority before publication.
    pub fn close_module_overlay_at(
        &mut self,
        key: &SourceKey,
        expected: SessionRevision,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        let (next, current) = self.validate_module_update_at(key, expected)?;
        let request = self.source_budget_request(current.bytes(), request);
        let limits = self.module_limit(current.bytes(), &request);
        self.read_module_saved_at(key, next, &request, limits)
    }

    /// Release one saved body without evicting an editor buffer in any admitted root.
    ///
    /// # Errors
    /// Rejects stale/unknown/binary input and active editor authority without mutation.
    pub fn release_module_saved_at(
        &mut self,
        key: &SourceKey,
        expected: SessionRevision,
    ) -> Result<(), SessionRequestError> {
        let (next, current) = self.validate_module_update_at(key, expected)?;
        if let InputOrigin::Overlay { document_version } = current.origin {
            return Err(SessionRequestError::OverlayActive { document_version });
        }
        let (root, database) = self.root_database_mut(key.root)?;
        let entry = root
            .modules
            .get_mut(&key.source)
            .ok_or_else(|| SessionRequestError::UnknownModule(key.source.clone()))?;
        let previous = entry.model.bytes();
        let was_ready = matches!(entry.model.state, ModuleState::Ready { .. });
        database.set_file_unreadable(entry.file);
        entry.model = ModuleInput {
            revision: next,
            origin: InputOrigin::Saved,
            source_sha256: None,
            state: ModuleState::Pending,
        };
        if was_ready {
            root.incomplete_modules += 1;
        }
        self.loaded_module_bytes -= previous;
        self.revision = next;
        Ok(())
    }

    /// Root qualification preserves the existing current-input and shared publication invariants.
    fn validate_module_update_at(
        &self,
        key: &SourceKey,
        expected: SessionRevision,
    ) -> Result<(SessionRevision, &ModuleInput), SessionRequestError> {
        let path = &key.source;
        self.validate_revision(expected)?;
        let entry = self
            .root_inputs(key.root)?
            .modules
            .get(path)
            .ok_or_else(|| SessionRequestError::UnknownModule(path.clone()))?;
        if entry.model.state == ModuleState::Binary {
            return Err(SessionRequestError::BinaryModule(path.clone()));
        }
        Ok((self.revision.next()?, &entry.model))
    }

    /// Root qualification preserves the existing current-input and shared publication invariants.
    fn read_module_saved_at(
        &mut self,
        key: &SourceKey,
        next: SessionRevision,
        request: &InputRequest,
        limits: (u64, bool),
    ) -> Result<(), SessionRequestError> {
        let path = &key.source;
        let (limit, budget_limited) = limits;
        let value = if request.cancellation.is_cancelled() {
            Err(ModuleFailure::Input(InputFailure::Cancelled))
        } else {
            match self
                .root_inputs(key.root)?
                .inputs
                .source_reader()
                .read_module_text(path, limit)
            {
                Ok(text) => match self.check_module_text(&text, request, limits) {
                    Ok(_) => Ok(text),
                    Err(failure) => Err(failure),
                },
                Err(_) if request.cancellation.is_cancelled() => {
                    Err(ModuleFailure::Input(InputFailure::Cancelled))
                }
                Err(error) if error.code == "source_document_too_large" => Err(if budget_limited {
                    ModuleFailure::LoadedTextBudgetExceeded {
                        limit: self.options.max_loaded_module_bytes,
                    }
                } else {
                    ModuleFailure::Input(request.limit_failure())
                }),
                Err(error) => Err(ModuleFailure::Input(InputFailure::Source(error))),
            }
        };
        self.publish_module_at(
            key,
            next,
            InputOrigin::Saved,
            value.as_deref().map_err(Clone::clone),
            request,
        )
    }

    /// Root qualification preserves the existing current-input and shared publication invariants.
    fn publish_module_at(
        &mut self,
        key: &SourceKey,
        next: SessionRevision,
        origin: InputOrigin,
        value: Result<&str, ModuleFailure>,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        let path = &key.source;
        // Fingerprint work happens before tracked writes; cancellation removes the old snapshot.
        let value = value.and_then(|text| {
            let hash = Sha256::digest(text.as_bytes()).iter().fold(
                String::with_capacity(64),
                |mut hash, byte| {
                    let _ = write!(hash, "{byte:02x}");
                    hash
                },
            );
            if request.cancellation.is_cancelled() {
                Err(ModuleFailure::Input(InputFailure::Cancelled))
            } else {
                Ok((text, hash))
            }
        });
        let (root, database) = self.root_database_mut(key.root)?;
        let entry = root
            .modules
            .get_mut(path)
            .ok_or_else(|| SessionRequestError::UnknownModule(path.clone()))?;
        let previous = entry.model.bytes();
        let was_ready = matches!(entry.model.state, ModuleState::Ready { .. });
        let (source_sha256, state) = match value {
            Ok((text, hash)) => {
                database.set_file_text(entry.file, text);
                (
                    Some(hash),
                    ModuleState::Ready {
                        bytes: text.len() as u64,
                    },
                )
            }
            Err(failure) => {
                database.set_file_unreadable(entry.file);
                (None, ModuleState::Unavailable(failure))
            }
        };
        entry.model = ModuleInput {
            revision: next,
            origin,
            source_sha256,
            state,
        };
        let current = entry.model.bytes();
        let is_ready = matches!(entry.model.state, ModuleState::Ready { .. });
        if was_ready && !is_ready {
            root.incomplete_modules += 1;
        } else if !was_ready && is_ready {
            root.incomplete_modules -= 1;
        }
        self.loaded_module_bytes = self.loaded_module_bytes - previous + current;
        self.revision = next;
        Ok(())
    }
}
