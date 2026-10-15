//! Resident BSL inputs are explicit snapshots; kernel queries never reread untracked disk text.

#[cfg(test)]
#[path = "modules/cache_trim_tests.rs"]
mod cache_trim_tests;

use std::fmt::Write as _;

use base_db::SourceDatabase;
use eska::{
    analysis_core::{InputFailure, InputOrigin, InputRequest},
    project_core::source_format::SourcePath,
};
use sha2::{Digest, Sha256};
use vfs::FileId;

use crate::{AnalysisSession, SessionRequestError, SessionRevision};

mod rooted;

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
    pub native_path: SourcePath,
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
        self.selected_root.modules.keys()
    }

    /// Return the current source fingerprint, origin and explicit readability state.
    #[must_use]
    pub fn module(&self, path: &SourcePath) -> Option<&ModuleInput> {
        self.selected_root
            .modules
            .get(path)
            .map(|entry| &entry.model)
    }

    /// Bind a BSL body to its actual owner YAML, including nested form/command descriptors.
    #[must_use]
    pub fn module_owner(&self, path: &SourcePath) -> Option<&SourcePath> {
        self.selected_root
            .modules
            .get(path)
            .map(|entry| &entry.owner)
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
        self.load_module_saved_at(&self.selected_source(path), expected, request)
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
        self.apply_module_overlay_at(
            &self.selected_source(path),
            expected,
            document_version,
            text,
            request,
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
        self.close_module_overlay_at(&self.selected_source(path), expected, request)
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
        self.release_module_saved_at(&self.selected_source(path), expected)
    }

    /// Trim computed memos and saved reparse hints after short-lived query handles dropped.
    pub fn trim_batch_caches(&mut self) {
        ide::sweep_lru_deep(&mut self.database);
        let editor_files = std::iter::once(&self.selected_root)
            .chain(self.other_roots.values())
            .flat_map(|root| root.modules.values())
            .filter_map(|entry| {
                matches!(entry.model.origin, InputOrigin::Overlay { .. }).then_some(entry.file)
            })
            .collect::<Vec<_>>();
        self.database.retain_parse_snapshots(&editor_files);
    }

    /// Count readable, protected and unknown current input independently of semantic diagnostics.
    #[must_use]
    pub fn module_summary(&self) -> ModuleSummary {
        let mut result = ModuleSummary {
            modules: self.selected_root.modules.len(),
            loaded_bytes: self
                .selected_root
                .modules
                .values()
                .map(|entry| entry.model.bytes())
                .sum(),
            max_loaded_bytes: self.options.max_loaded_module_bytes,
            ..ModuleSummary::default()
        };
        for entry in self.selected_root.modules.values() {
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

    /// A replacement may reuse its previous reservation; accepted failure later releases that space.
    fn module_limit(&self, previous_bytes: u64, request: &InputRequest) -> (u64, bool) {
        let remaining = self
            .options
            .max_loaded_module_bytes
            .saturating_sub(self.loaded_module_bytes.saturating_sub(previous_bytes));
        let document_limit = request.source_limit();
        (remaining.min(document_limit), remaining < document_limit)
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
}
