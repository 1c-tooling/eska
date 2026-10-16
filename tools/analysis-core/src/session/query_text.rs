//! Declared standalone queries share the host clock, source reserve and saved/editor authority.

use std::collections::BTreeMap;

use eska::{
    analysis_core::InputRequest,
    project_core::source_format::{Error, SourcePath, SourceReader},
};

use crate::{
    AnalysisSession, DiagnosticsConfig, QuerySyntaxError, QuerySyntaxResult, QueryTextChange,
    QueryTextDeclaration, QueryTextError, QueryTextInput, QueryTextSnapshot, QueryTextState,
    SessionError, SessionRequestError, SessionRevision,
};

#[cfg(test)]
mod tests;

mod analysis;
#[cfg(test)]
mod analysis_tests;
mod consumers;
mod context;
mod rooted;
pub use analysis::{QueryAnalysisDiagnostics, QueryAnalysisRequestError};
pub use consumers::{
    QueryConsumerAuthoringLocation, QueryConsumerCheckCoverage, QueryConsumerCoverageReport,
    QueryConsumerDiagnosticReport, QueryConsumerDiagnostics, QueryConsumerFinding,
    QueryConsumerIncomplete, QueryConsumerIncompleteReason, QueryConsumerIncompleteReport,
    QueryConsumerLocation, QueryConsumerLocationReport, QueryConsumerReport,
    QueryConsumerReportContext, QueryConsumerReportStatus, QueryConsumerRequestError,
    QueryConsumerTarget,
};
pub use context::{QueryConsumerContext, QueryContextRequestError, QueryContextSource};

/// Current text readiness is separate from parse validity and metadata/consumer coverage.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize)]
pub struct QueryTextSummary {
    pub queries: usize,
    pub pending: usize,
    pub ready: usize,
    pub unavailable: usize,
    pub loaded_bytes: u64,
    pub query_input_complete: bool,
}

/// The host revision covers every accepted input, while the result retains its exact query publication.
#[derive(Clone, Debug)]
pub struct QuerySyntaxDiagnostics {
    pub revision: SessionRevision,
    pub result: QuerySyntaxResult,
}

/// Rejected host requests and unread/cancelled query text cannot be successful empty syntax results.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QuerySyntaxRequestError {
    Request(SessionRequestError),
    Syntax(QuerySyntaxError),
}

impl AnalysisSession {
    /// Query locations come only from the same frozen verified source declarations as other inputs.
    pub fn query_paths(&self) -> impl Iterator<Item = &SourcePath> {
        self.selected_root.query_inputs.keys()
    }

    /// Read current bytes/origin/hash without loading a saved owner or a neighboring resource.
    #[must_use]
    pub fn query_input(&self, path: &SourcePath) -> Option<&QueryTextSnapshot> {
        self.selected_root
            .query_inputs
            .get(path)
            .map(QueryTextInput::snapshot)
    }

    /// Ownership is captured from the manifest; syntax alone does not approve current metadata context.
    #[must_use]
    pub fn query_declaration(&self, path: &SourcePath) -> Option<&QueryTextDeclaration> {
        self.selected_root
            .query_inputs
            .get(path)
            .map(QueryTextInput::declaration)
    }

    /// Read one saved query under the host revision and the shared replacement reserve.
    ///
    /// # Errors
    /// Rejects stale revisions, undeclared inputs and active editor authority before publication.
    pub fn load_query_saved(
        &mut self,
        path: &SourcePath,
        expected: SessionRevision,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        self.load_query_saved_at(&self.selected_source(path), expected, request)
    }

    /// A strictly newer editor version remains authoritative even when accepted input fails its quota.
    ///
    /// # Errors
    /// Rejects stale revisions/versions and undeclared paths without changing input or source usage.
    pub fn apply_query_overlay(
        &mut self,
        path: &SourcePath,
        expected: SessionRevision,
        document_version: i64,
        text: &str,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        self.apply_query_overlay_at(
            &self.selected_source(path),
            expected,
            document_version,
            text,
            request,
        )
    }

    /// Close restores current disk bytes or their current failure, without a saved snapshot fallback.
    ///
    /// # Errors
    /// Rejects stale revisions and undeclared sources before reading or dropping editor authority.
    pub fn close_query_overlay(
        &mut self,
        path: &SourcePath,
        expected: SessionRevision,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        self.close_query_overlay_at(&self.selected_source(path), expected, request)
    }

    /// A syntax batch or saved watcher can release current saved text without evicting an editor buffer.
    ///
    /// # Errors
    /// Rejects stale revisions, undeclared inputs and active overlays without a partial release.
    pub fn release_query_saved(
        &mut self,
        path: &SourcePath,
        expected: SessionRevision,
    ) -> Result<(), SessionRequestError> {
        self.release_query_saved_at(&self.selected_source(path), expected)
    }

    /// Native parse diagnostics require no `FileId`, canonical configuration or materialized XML.
    ///
    /// Cancellation is checked around the native call; the pinned parser has no internal stop hook.
    /// # Errors
    /// Rejects stale/unknown input, pending/unavailable current text, cancellation or invalid ranges.
    pub fn query_syntax_diagnostics(
        &self,
        path: &SourcePath,
        expected: SessionRevision,
        config: &DiagnosticsConfig,
        request: &InputRequest,
    ) -> Result<QuerySyntaxDiagnostics, QuerySyntaxRequestError> {
        self.query_syntax_diagnostics_at(&self.selected_source(path), expected, config, request)
    }

    /// Report retained query bytes/readiness without treating an empty diagnostic set as completeness.
    #[must_use]
    pub fn query_summary(&self) -> QueryTextSummary {
        let mut result = QueryTextSummary {
            queries: self.selected_root.query_inputs.len(),
            loaded_bytes: self
                .selected_root
                .query_inputs
                .values()
                .map(QueryTextInput::bytes)
                .sum(),
            ..QueryTextSummary::default()
        };
        for input in self.selected_root.query_inputs.values() {
            match input.snapshot().state {
                QueryTextState::Pending => result.pending += 1,
                QueryTextState::Ready(_) => result.ready += 1,
                QueryTextState::Unavailable(_) => result.unavailable += 1,
            }
        }
        result.query_input_complete = result.ready == result.queries;
        result
    }

    /// All accepted leaf outcomes publish their source reservation with the same global host clock.
    const fn publish_query_change(&mut self, next: SessionRevision, change: QueryTextChange) {
        self.loaded_query_bytes =
            self.loaded_query_bytes - change.previous_bytes + change.current_bytes;
        self.commit_input(next, change.current_bytes);
    }
}

/// Pending leaf registration reads no plain query, resource, owner or protected history.
pub(super) fn register(
    reader: &SourceReader,
) -> Result<BTreeMap<SourcePath, QueryTextInput>, SessionError> {
    reader
        .query_paths()
        .map(|path| {
            QueryTextInput::declare(reader, path)
                .map(|input| (path.clone(), input))
                .map_err(|_| {
                    SessionError::Source(Error {
                        code: "source_query_map_invalid",
                        location: vec![path.as_str().into()],
                    })
                })
        })
        .collect()
}

/// Shared editor/version errors preserve the existing host protocol; a reader mismatch remains explicit.
fn input_error(error: QueryTextError) -> SessionRequestError {
    match error {
        QueryTextError::Undeclared(path) => SessionRequestError::UnknownQuery(path),
        QueryTextError::ReaderMismatch => SessionRequestError::QueryReaderMismatch,
        QueryTextError::OverlayActive { document_version } => {
            SessionRequestError::OverlayActive { document_version }
        }
        QueryTextError::StaleDocumentVersion { current, received } => {
            SessionRequestError::StaleDocumentVersion { current, received }
        }
    }
}
