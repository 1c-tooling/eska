//! Proven output names check current storage bindings without granting complete consumer semantics.

use eska::{analysis_core::InputRequest, project_core::source_format::SourcePath};
use salsa::Database;

use crate::{
    AnalysisSession, QueryCancellation, QueryConsumerContext, QueryContextRequestError, QueryError,
    QueryTextState, SessionRequestError, SessionRevision,
};

mod checks;
mod model;
mod parameters;
mod report;
#[cfg(test)]
mod tests;

pub use model::{
    QueryConsumerCheckCoverage, QueryConsumerDiagnostics, QueryConsumerFinding,
    QueryConsumerIncomplete, QueryConsumerIncompleteReason, QueryConsumerLocation,
    QueryConsumerTarget,
};
pub use report::{
    QueryConsumerAuthoringLocation, QueryConsumerCoverageReport, QueryConsumerDiagnosticReport,
    QueryConsumerIncompleteReport, QueryConsumerLocationReport, QueryConsumerReport,
    QueryConsumerReportContext, QueryConsumerReportStatus,
};

/// Missing current inputs and native cancellation are refusals, never clean consumer results.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueryConsumerRequestError {
    Request(SessionRequestError),
    Context(QueryContextRequestError),
    Native(QueryError),
}

impl AnalysisSession {
    /// Prepare current storage and visible canonical metadata without reading BSL bodies.
    ///
    /// # Errors
    /// Rejects absent configuration context, stale input and cancellation. Current failed overlays
    /// remain authoritative, and a subsequent diagnostic call reports their current refusal.
    pub fn prepare_query_consumers(
        &mut self,
        path: &SourcePath,
        expected: SessionRevision,
        request: &InputRequest,
    ) -> Result<(), QueryConsumerRequestError> {
        self.require_configuration_context(expected)
            .map_err(QueryConsumerRequestError::Request)?;
        self.prepare_query_context(path, expected, request)
            .map_err(QueryConsumerRequestError::Context)?;
        self.prepare_query_analysis(self.revision(), request)
            .map_err(QueryConsumerRequestError::Request)?;
        if request.cancellation.is_cancelled() {
            Err(QueryConsumerRequestError::Native(QueryError::Cancelled))
        } else {
            Ok(())
        }
    }

    /// Check explicit storage bindings against the owner's exact current standalone query model.
    ///
    /// The result retains current source stamps and native output/parameter occurrences. Unknown
    /// output shape or unsupported bindings remain explicit incompleteness instead of missing fields.
    /// # Errors
    /// Refuses stale, missing or incomplete canonical inputs, failed storage, bounds and cancellation.
    pub fn query_consumer_diagnostics(
        &self,
        path: &SourcePath,
        expected: SessionRevision,
        request: &InputRequest,
        cancellation: &QueryCancellation,
    ) -> Result<QueryConsumerDiagnostics, QueryConsumerRequestError> {
        self.require_configuration_context(expected)
            .map_err(QueryConsumerRequestError::Request)?;
        check_cancelled(request, cancellation)?;
        let context = self
            .query_context(path, expected, request)
            .map_err(QueryConsumerRequestError::Context)?;
        self.require_query_metadata()
            .map_err(QueryConsumerRequestError::Request)?;
        let input = self.query_input(path).ok_or_else(mismatch)?;
        let QueryTextState::Ready(text) = &input.state else {
            return Err(mismatch());
        };
        let owner = self
            .selected_root
            .owners
            .get(&context.owner.source)
            .ok_or_else(mismatch)?;
        if owner.binding.native_path != *context.consumer.native_descriptor() {
            return Err(mismatch());
        }
        let parameter_types = parameters::types(&context, request, cancellation)?;
        let analysis = ide::Analysis::from_database(self.database.clone());
        let model = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
            analysis.attached(|analysis| -> Result<_, QueryConsumerRequestError> {
                let _registration = cancellation
                    .register(analysis.database().cancellation_token())
                    .map_err(QueryConsumerRequestError::Native)?;
                Ok(analysis.query_text_model(owner.binding.main, text, &parameter_types))
            })
        }))
        .map_err(|_| QueryConsumerRequestError::Native(QueryError::Cancelled))??;
        check_cancelled(request, cancellation)?;
        let result = checks::check(context, model, request, cancellation)?;
        check_cancelled(request, cancellation)?;
        Ok(result)
    }
}

/// Every early and final boundary observes the same caller-owned cancellation tokens.
fn check_cancelled(
    request: &InputRequest,
    cancellation: &QueryCancellation,
) -> Result<(), QueryConsumerRequestError> {
    if request.cancellation.is_cancelled() || cancellation.is_cancelled() {
        Err(QueryConsumerRequestError::Native(QueryError::Cancelled))
    } else {
        Ok(())
    }
}

/// No metadata slot or descriptor anchor may be guessed from a query's physical filename.
const fn mismatch() -> QueryConsumerRequestError {
    QueryConsumerRequestError::Request(SessionRequestError::QueryReaderMismatch)
}
