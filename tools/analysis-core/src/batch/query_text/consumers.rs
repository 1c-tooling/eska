//! Batch and IDE use one source-stamped consumer report, including explicit current-resource refusals.

use eska::{analysis_core::InputRequest, project_core::source_format::SourcePath};

use crate::{
    AnalysisSession, QueryCancellation, QueryConsumerReportStatus, QueryConsumerRequestError,
    QueryContextRequestError, QueryError, SessionRequestError,
};

use super::BatchError;

/// Active cancellation stops the batch; recoverable current input failures are serialized as unavailable.
pub(super) fn check(
    session: &mut AnalysisSession,
    path: &SourcePath,
    request: &InputRequest,
    cancellation: &QueryCancellation,
) -> Result<Option<QueryConsumerReportStatus>, BatchError> {
    match session.check_query_consumer_report(path, request, cancellation) {
        Ok(report) => Ok(Some(report)),
        Err(
            QueryConsumerRequestError::Native(QueryError::Cancelled)
            | QueryConsumerRequestError::Context(QueryContextRequestError::Cancelled),
        ) => Ok(None),
        Err(
            QueryConsumerRequestError::Request(error)
            | QueryConsumerRequestError::Context(QueryContextRequestError::Request(error)),
        ) => Err(BatchError::Request(error)),
        Err(QueryConsumerRequestError::Native(error)) => Err(BatchError::Query(error)),
        // The shared report driver has already classified every recoverable storage error.
        Err(QueryConsumerRequestError::Context(_)) => Err(BatchError::Request(
            SessionRequestError::QueryReaderMismatch,
        )),
    }
}
