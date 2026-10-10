//! Diagnostics and IDE requests query the same current bytes and canonical metadata database.

#[cfg(test)]
#[path = "queries/cancellation_tests.rs"]
mod cancellation_tests;
pub mod definitions;
mod document_symbols;
pub mod local_rename;
mod metadata_definitions;
pub mod references;
pub mod signature_help;
pub mod symbol_hover;

use std::collections::BTreeMap;

use base_db::{RootQueryDb, SourceDatabase};
use eska::project_core::source_format::SourcePath;
use salsa::Database;

use crate::{
    AnalysisSession, CompletionItem, Diagnostic, DiagnosticCode, DiagnosticsConfig, Locale,
    ModuleState, QueryCancellation, SessionRequestError, SessionRevision,
};

/// A result belongs to one verbatim source snapshot and one complete session generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleResult<T> {
    pub revision: SessionRevision,
    pub source: SourcePath,
    pub source_sha256: String,
    pub metadata_models_complete: bool,
    pub module_inputs_complete: bool,
    pub value: T,
}

/// Unavailable input is a failed query, never an empty successful diagnostic/completion list.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueryError {
    Request(SessionRequestError),
    ModuleUnavailable {
        path: SourcePath,
        state: ModuleState,
    },
    InvalidOffset {
        offset: u32,
    },
    InconsistentDiagnostic {
        start_byte: u32,
        end_byte: u32,
    },
    UnsupportedNavigationTarget,
    InconsistentNavigation {
        start_byte: u32,
        end_byte: u32,
    },
    InconsistentDocumentSymbol {
        start_byte: u32,
        end_byte: u32,
        selection_start_byte: u32,
        selection_end_byte: u32,
    },
    InconsistentHover {
        start_byte: u32,
        end_byte: u32,
    },
    UnsupportedHoverSymbol {
        kind: &'static str,
    },
    InconsistentSignatureHelp {
        signature: usize,
        parameter: Option<usize>,
    },
    UnsupportedReferenceSymbol {
        category: &'static str,
    },
    InconsistentReferences,
    Cancelled,
    CancellationTokenInUse,
}

impl AnalysisSession {
    /// Project borrowed current bytes for transport coordinates without rereading or copying text.
    ///
    /// # Errors
    /// Returns the same stale/unknown/unread input and cancellation failures as an analysis query.
    pub fn with_module_text<T>(
        &self,
        path: &SourcePath,
        expected: SessionRevision,
        cancellation: &QueryCancellation,
        project: impl FnOnce(&str) -> T,
    ) -> Result<ModuleResult<T>, QueryError> {
        self.query_module(path, expected, cancellation, |analysis, file| {
            analysis
                .database()
                .try_file_text_input(file)
                .map(|input| project(input.text(analysis.database())))
                .ok_or_else(|| QueryError::ModuleUnavailable {
                    path: path.clone(),
                    state: ModuleState::Pending,
                })
        })
    }

    /// Run the shared BSL diagnostics profile over this exact saved or editor module snapshot.
    ///
    /// # Errors
    /// Returns stale/unknown/unread input or cancellation; parser errors are ordinary diagnostics.
    pub fn module_diagnostics(
        &self,
        path: &SourcePath,
        expected: SessionRevision,
        config: &DiagnosticsConfig,
        cancellation: &QueryCancellation,
    ) -> Result<ModuleResult<Vec<Diagnostic>>, QueryError> {
        self.query_module(path, expected, cancellation, |analysis, file| {
            let mut diagnostics = analysis.diagnostics(file, config);
            if config.locale == Locale::En
                && diagnostics
                    .iter()
                    .any(|item| item.code == DiagnosticCode::ParseError)
            {
                // The pinned handler ignores locale. Reuse its cached structured parse errors;
                // range plus the original rendering identifies errors after suppression/sorting.
                let parse = analysis.database().parse(file);
                let messages: BTreeMap<_, _> = parse
                    .errors()
                    .iter()
                    .map(|error| {
                        (
                            (
                                u32::from(error.range().start()),
                                u32::from(error.range().end()),
                                error.structured().format_ru(),
                            ),
                            error.structured().format_en(),
                        )
                    })
                    .collect();
                for item in diagnostics
                    .iter_mut()
                    .filter(|item| item.code == DiagnosticCode::ParseError)
                {
                    let message = messages
                        .get(&(
                            u32::from(item.range.start()),
                            u32::from(item.range.end()),
                            item.message.clone(),
                        ))
                        .ok_or_else(|| QueryError::InconsistentDiagnostic {
                            start_byte: item.range.start().into(),
                            end_byte: item.range.end().into(),
                        })?;
                    item.message.clone_from(message);
                }
            }
            Ok(diagnostics)
        })
    }

    /// Complete at a UTF-8 byte boundary, resolving metadata from current YAML canonical slots.
    ///
    /// # Errors
    /// Returns stale/unknown/unread input, an invalid offset or cancellation.
    pub fn module_completions(
        &self,
        path: &SourcePath,
        expected: SessionRevision,
        offset: u32,
        locale: Locale,
        cancellation: &QueryCancellation,
    ) -> Result<ModuleResult<Vec<CompletionItem>>, QueryError> {
        self.query_module(path, expected, cancellation, |analysis, file| {
            let input = analysis
                .database()
                .try_file_text_input(file)
                .ok_or_else(|| QueryError::ModuleUnavailable {
                    path: path.clone(),
                    state: ModuleState::Pending,
                })?;
            if !input
                .text(analysis.database())
                .is_char_boundary(offset as usize)
            {
                return Err(QueryError::InvalidOffset { offset });
            }
            Ok(analysis.completions(file, offset, Some(self.root().to_owned()), locale))
        })
    }

    /// Validate the exact source and clock before cloning a short-lived kernel handle.
    fn query_module<T>(
        &self,
        path: &SourcePath,
        expected: SessionRevision,
        cancellation: &QueryCancellation,
        query: impl FnOnce(&ide::Analysis, vfs::FileId) -> Result<T, QueryError>,
    ) -> Result<ModuleResult<T>, QueryError> {
        self.validate_revision(expected)
            .map_err(QueryError::Request)?;
        let entry = self
            .modules
            .get(path)
            .ok_or_else(|| QueryError::Request(SessionRequestError::UnknownModule(path.clone())))?;
        if !matches!(entry.model.state, ModuleState::Ready { .. }) {
            return Err(QueryError::ModuleUnavailable {
                path: path.clone(),
                state: entry.model.state.clone(),
            });
        }
        let source_sha256 =
            entry
                .model
                .source_sha256
                .clone()
                .ok_or_else(|| QueryError::ModuleUnavailable {
                    path: path.clone(),
                    state: ModuleState::Pending,
                })?;
        if cancellation.is_cancelled() {
            return Err(QueryError::Cancelled);
        }
        let analysis = ide::Analysis::from_database(self.database.clone());
        // Salsa handles cancellation unwind; an unrelated panic is rethrown by `catch`.
        let value = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
            analysis.attached(|analysis| {
                let _registration =
                    cancellation.register(analysis.database().cancellation_token())?;
                analysis.database().unwind_if_revision_cancelled();
                let value = query(analysis, entry.file)?;
                analysis.database().unwind_if_revision_cancelled();
                Ok(value)
            })
        }))
        .map_err(|_| QueryError::Cancelled)??;
        if cancellation.is_cancelled() {
            return Err(QueryError::Cancelled);
        }
        Ok(ModuleResult {
            revision: expected,
            source: path.clone(),
            source_sha256,
            metadata_models_complete: self.incomplete_metadata == 0,
            module_inputs_complete: self.incomplete_modules == 0,
            value,
        })
    }
}
