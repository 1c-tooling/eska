//! Contextual standalone queries share metadata visibility, the host clock and native cancellation.

use eska::{analysis_core::InputRequest, project_core::source_format::SourcePath};
use ide_db::query_resolver::FileQueryResolver;
use salsa::Database;

use crate::{
    AnalysisSession, DiagnosticsConfig, QueryAnalysisResult, QueryCancellation, QueryError,
    QuerySyntaxError, SessionRequestError, SessionRevision,
    composition::{RootId, SourceKey},
    query_analysis_enabled,
};

/// Current source and metadata belong to this one complete host generation, not a cached baseline.
#[derive(Clone, Debug)]
pub struct QueryAnalysisDiagnostics {
    pub revision: SessionRevision,
    pub root: RootId,
    pub composition_sha256: Option<String>,
    pub result: QueryAnalysisResult,
}

/// Missing context and current input are refusals rather than a successful empty validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueryAnalysisRequestError {
    Request(SessionRequestError),
    Input(QuerySyntaxError),
    Native(QueryError),
    Disabled,
}

impl AnalysisSession {
    /// Prepare only visible current metadata and declared resources; no BSL body or query is read.
    ///
    /// # Errors
    /// Rejects stale or absent configuration context; current failed editor input is never replaced.
    pub fn prepare_query_analysis(
        &mut self,
        expected: SessionRevision,
        request: &InputRequest,
    ) -> Result<(), SessionRequestError> {
        self.require_configuration_context(expected)?;
        for root in self.visible_root_ids(self.selected_root_id()) {
            if request.cancellation.is_cancelled() {
                break;
            }
            self.load_root_metadata_inputs(root, request)?;
        }
        Ok(())
    }

    /// Check a declared query against current selected-root metadata under an explicit native token.
    ///
    /// # Errors
    /// Refuses disabled rules, stale/unread input, incomplete metadata, cancellation or invalid ranges.
    pub fn query_analysis_diagnostics(
        &self,
        path: &SourcePath,
        expected: SessionRevision,
        config: &DiagnosticsConfig,
        request: &InputRequest,
        cancellation: &QueryCancellation,
    ) -> Result<QueryAnalysisDiagnostics, QueryAnalysisRequestError> {
        self.require_configuration_context(expected)
            .map_err(QueryAnalysisRequestError::Request)?;
        if !query_analysis_enabled(config) {
            return Err(QueryAnalysisRequestError::Disabled);
        }
        if request.cancellation.is_cancelled() || cancellation.is_cancelled() {
            return Err(QueryAnalysisRequestError::Native(QueryError::Cancelled));
        }
        let root = &self.selected_root;
        let input = root.query_inputs.get(path).ok_or_else(|| {
            QueryAnalysisRequestError::Request(SessionRequestError::UnknownQuery(path.clone()))
        })?;
        let owner = root.owners.get(&input.declaration().owner).ok_or({
            QueryAnalysisRequestError::Request(SessionRequestError::QueryReaderMismatch)
        })?;
        self.require_query_metadata()
            .map_err(QueryAnalysisRequestError::Request)?;
        self.require_common_attribute_membership(owner.binding.main)
            .map_err(|error| match error {
                QueryError::Request(error) => QueryAnalysisRequestError::Request(error),
                error => QueryAnalysisRequestError::Native(error),
            })?;
        let analysis = ide::Analysis::from_database(self.database.clone());
        let result = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
            analysis.attached(|analysis| {
                let _registration = cancellation
                    .register(analysis.database().cancellation_token())
                    .map_err(QueryAnalysisRequestError::Native)?;
                analysis.database().unwind_if_revision_cancelled();
                let resolver = FileQueryResolver::new(analysis.database(), owner.binding.main);
                let result = input
                    .query_analysis(config, &resolver, request)
                    .map_err(QueryAnalysisRequestError::Input)?;
                analysis.database().unwind_if_revision_cancelled();
                Ok(result)
            })
        }))
        .map_err(|_| QueryAnalysisRequestError::Native(QueryError::Cancelled))??;
        if cancellation.is_cancelled() || request.cancellation.is_cancelled() {
            return Err(QueryAnalysisRequestError::Native(QueryError::Cancelled));
        }
        Ok(QueryAnalysisDiagnostics {
            revision: expected,
            root: root.id,
            composition_sha256: self.composition_sha256().map(str::to_owned),
            result,
        })
    }

    /// A declared unresolved contribution cannot become a missing-table or missing-field diagnostic.
    pub(crate) fn require_query_metadata(&self) -> Result<(), SessionRequestError> {
        for id in self.visible_root_ids(self.selected_root_id()) {
            let root = self.root_inputs(id)?;
            if let Some((source, _)) = root
                .owners
                .iter()
                .find(|(_, entry)| !entry.model.descriptors_complete())
            {
                return Err(SessionRequestError::CompositionInputIncomplete(SourceKey {
                    root: id,
                    source: source.clone(),
                }));
            }
        }
        Ok(())
    }
}
