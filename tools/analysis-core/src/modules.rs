//! Resident BSL inputs are explicit snapshots; kernel queries never reread untracked disk text.

use std::fmt::Write as _;

use base_db::SourceDatabase;
use eska::{
    analysis_core::{InputFailure, InputOrigin, InputRequest},
    project_core::source_format::SourcePath,
};
use sha2::{Digest, Sha256};
use vfs::FileId;

use crate::{AnalysisSession, SessionRequestError, SessionRevision};

const MAX_DOCUMENT_BYTES: u64 = 64 * 1024 * 1024;

/// These caps count current source sizes, not parsed model heap, Salsa memos or process RSS.
#[derive(Clone, Debug)]
pub struct SessionOptions {
    pub max_loaded_module_bytes: u64,
    pub max_retained_source_bytes: u64,
}

impl Default for SessionOptions {
    /// Keep module snapshots bounded independently of a future global analysis memory budget.
    fn default() -> Self {
        Self {
            max_loaded_module_bytes: MAX_DOCUMENT_BYTES,
            max_retained_source_bytes: 256 * 1024 * 1024,
        }
    }
}

/// Readability is distinct from syntax validity and from the existence of a protected binary body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModuleState {
    Pending,
    Ready { bytes: u64 },
    Binary,
    Unavailable(ModuleFailure),
}

/// A failed accepted read clears previous text instead of exposing a successful empty module.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModuleFailure {
    Input(InputFailure),
    LoadedTextBudgetExceeded { limit: u64 },
}

/// Coordinates and origin refer to exactly these verbatim UTF-8 bytes, including BOM and CRLF.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleInput {
    pub revision: SessionRevision,
    pub origin: InputOrigin,
    pub source_sha256: Option<String>,
    pub state: ModuleState,
}

impl ModuleInput {
    /// Only available current text consumes the declared text budget.
    #[must_use]
    pub const fn bytes(&self) -> u64 {
        match self.state {
            ModuleState::Ready { bytes } => bytes,
            _ => 0,
        }
    }
}

/// A stable `FileId` and native descriptor owner connect text to canonical metadata without XML.
pub struct ModuleEntry {
    pub file: FileId,
    pub owner: SourcePath,
    pub model: ModuleInput,
}

/// Binary and unread bodies are explicit missing input, never proof of a clean project.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize)]
pub struct ModuleSummary {
    pub modules: usize,
    pub pending: usize,
    pub ready: usize,
    pub binary: usize,
    pub unavailable: usize,
    pub loaded_bytes: u64,
    pub max_loaded_bytes: u64,
    pub module_input_complete: bool,
}

impl AnalysisSession {
    /// Enumerate module locations from the same validated composition as metadata owners.
    pub fn module_paths(&self) -> impl Iterator<Item = &SourcePath> {
        self.modules.keys()
    }

    /// Return the current source fingerprint, origin and explicit readability state.
    #[must_use]
    pub fn module(&self, path: &SourcePath) -> Option<&ModuleInput> {
        self.modules.get(path).map(|entry| &entry.model)
    }

    /// Bind a BSL body to its actual owner YAML, including nested form/command descriptors.
    #[must_use]
    pub fn module_owner(&self, path: &SourcePath) -> Option<&SourcePath> {
        self.modules.get(path).map(|entry| &entry.owner)
    }

    /// Read current disk bytes; an active editor buffer rejects a background disk refresh.
    ///
    /// # Errors
    /// Returns stale/undeclared/binary input, an active overlay or exhausted revision counter.
    pub fn load_module_saved(
        &mut self,
        path: &SourcePath,
        expected: SessionRevision,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        let (next, current) = self.validate_module_update(path, expected)?;
        if let InputOrigin::Overlay { document_version } = current.origin {
            return Err(SessionRequestError::OverlayActive { document_version });
        }
        let request = self.source_budget_request(current.bytes(), request);
        let limits = self.module_limit(current.bytes(), &request);
        self.read_module_saved(path, next, &request, limits)
    }

    /// Replace the module's current bytes with a newer verbatim editor document.
    ///
    /// # Errors
    /// Returns stale/undeclared/binary input, an old document version or exhausted revision counter.
    pub fn apply_module_overlay(
        &mut self,
        path: &SourcePath,
        expected: SessionRevision,
        document_version: i64,
        text: &str,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        let (next, current) = self.validate_module_update(path, expected)?;
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
        self.publish_module(
            path,
            next,
            InputOrigin::Overlay { document_version },
            value,
            &request,
        )
    }

    /// Restore the current disk bytes, including deletion/unreadability instead of a saved cache.
    ///
    /// # Errors
    /// Returns stale/undeclared/binary input or exhausted revision counter.
    pub fn close_module_overlay(
        &mut self,
        path: &SourcePath,
        expected: SessionRevision,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        let (next, current) = self.validate_module_update(path, expected)?;
        let request = self.source_budget_request(current.bytes(), request);
        let limits = self.module_limit(current.bytes(), &request);
        self.read_module_saved(path, next, &request, limits)
    }

    /// Release a saved body after a syntax batch check; editor-owned snapshots cannot be evicted.
    ///
    /// # Errors
    /// Returns stale/undeclared/binary input, an active overlay or exhausted revision counter.
    pub fn release_module_saved(
        &mut self,
        path: &SourcePath,
        expected: SessionRevision,
    ) -> Result<(), SessionRequestError> {
        let (next, current) = self.validate_module_update(path, expected)?;
        if let InputOrigin::Overlay { document_version } = current.origin {
            return Err(SessionRequestError::OverlayActive { document_version });
        }
        let entry = self
            .modules
            .get_mut(path)
            .ok_or_else(|| SessionRequestError::UnknownModule(path.clone()))?;
        self.loaded_module_bytes -= entry.model.bytes();
        if matches!(entry.model.state, ModuleState::Ready { .. }) {
            self.incomplete_modules += 1;
        }
        self.database.set_file_unreadable(entry.file);
        entry.model = ModuleInput {
            revision: next,
            origin: InputOrigin::Saved,
            source_sha256: None,
            state: ModuleState::Pending,
        };
        self.revision = next;
        Ok(())
    }

    /// Trim heavy computed memos between batch chunks after all short-lived query handles dropped.
    pub fn trim_batch_caches(&mut self) {
        self.database.enforce_lru_deep();
    }

    /// Count readable, protected and unknown current input independently of semantic diagnostics.
    #[must_use]
    pub fn module_summary(&self) -> ModuleSummary {
        let mut result = ModuleSummary {
            modules: self.modules.len(),
            loaded_bytes: self.loaded_module_bytes,
            max_loaded_bytes: self.options.max_loaded_module_bytes,
            ..ModuleSummary::default()
        };
        for entry in self.modules.values() {
            match entry.model.state {
                ModuleState::Pending => result.pending += 1,
                ModuleState::Ready { .. } => result.ready += 1,
                ModuleState::Binary => result.binary += 1,
                ModuleState::Unavailable(_) => result.unavailable += 1,
            }
        }
        result.module_input_complete =
            result.pending == 0 && result.binary == 0 && result.unavailable == 0;
        result
    }

    /// Validate every request before reads, allocations or tracked writes.
    fn validate_module_update(
        &self,
        path: &SourcePath,
        expected: SessionRevision,
    ) -> Result<(SessionRevision, &ModuleInput), SessionRequestError> {
        self.validate_revision(expected)?;
        let entry = self
            .modules
            .get(path)
            .ok_or_else(|| SessionRequestError::UnknownModule(path.clone()))?;
        if entry.model.state == ModuleState::Binary {
            return Err(SessionRequestError::BinaryModule(path.clone()));
        }
        Ok((self.revision.next()?, &entry.model))
    }

    /// A replacement may reuse its previous reservation; accepted failure later releases that space.
    fn module_limit(&self, previous_bytes: u64, request: &InputRequest) -> (u64, bool) {
        let remaining = self
            .options
            .max_loaded_module_bytes
            .saturating_sub(self.loaded_module_bytes.saturating_sub(previous_bytes));
        let document_limit = request.source_limit();
        (remaining.min(document_limit), remaining < document_limit)
    }

    /// Saved input is read through the same bounded, symlink-rejecting project reader as YAML.
    fn read_module_saved(
        &mut self,
        path: &SourcePath,
        next: SessionRevision,
        request: &InputRequest,
        limits: (u64, bool),
    ) -> Result<(), SessionRequestError> {
        let (limit, budget_limited) = limits;
        let value = if request.cancellation.is_cancelled() {
            Err(ModuleFailure::Input(InputFailure::Cancelled))
        } else {
            match self.inputs.source_reader().read_module_text(path, limit) {
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
        self.publish_module(
            path,
            next,
            InputOrigin::Saved,
            value.as_deref().map_err(Clone::clone),
            request,
        )
    }

    /// Saved and editor input share cancellation, per-document and aggregate text bounds.
    fn check_module_text<'a>(
        &self,
        text: &'a str,
        request: &InputRequest,
        limits: (u64, bool),
    ) -> Result<&'a str, ModuleFailure> {
        if request.cancellation.is_cancelled() {
            return Err(ModuleFailure::Input(InputFailure::Cancelled));
        }
        let (limit, budget_limited) = limits;
        if text.len() as u64 > limit {
            return Err(if budget_limited {
                ModuleFailure::LoadedTextBudgetExceeded {
                    limit: self.options.max_loaded_module_bytes,
                }
            } else {
                ModuleFailure::Input(request.limit_failure())
            });
        }
        Ok(text)
    }

    /// Commit text, fingerprint, origin and host revision as one synchronous accepted update.
    fn publish_module(
        &mut self,
        path: &SourcePath,
        next: SessionRevision,
        origin: InputOrigin,
        value: Result<&str, ModuleFailure>,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
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
        let entry = self
            .modules
            .get_mut(path)
            .ok_or_else(|| SessionRequestError::UnknownModule(path.clone()))?;
        self.loaded_module_bytes -= entry.model.bytes();
        let was_ready = matches!(entry.model.state, ModuleState::Ready { .. });
        let (source_sha256, state) = match value {
            Ok((text, hash)) => {
                self.database.set_file_text(entry.file, text);
                self.loaded_module_bytes += text.len() as u64;
                (
                    Some(hash),
                    ModuleState::Ready {
                        bytes: text.len() as u64,
                    },
                )
            }
            Err(failure) => {
                self.database.set_file_unreadable(entry.file);
                (None, ModuleState::Unavailable(failure))
            }
        };
        entry.model = ModuleInput {
            revision: next,
            origin,
            source_sha256,
            state,
        };
        let is_ready = matches!(entry.model.state, ModuleState::Ready { .. });
        if was_ready && !is_ready {
            self.incomplete_modules += 1;
        } else if !was_ready && is_ready {
            self.incomplete_modules -= 1;
        }
        self.revision = next;
        Ok(())
    }
}
