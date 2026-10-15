//! Semantic batches share the resident query API and never substitute disk for editor authority.

use eska::analysis_core::{InputFailure, InputOrigin, InputRequest};
use eska::project_core::source_format::SourcePath;

use super::{
    BatchError, BatchQueryContext, BatchQueryFile, BatchQueryScope, FileState, consumers, findings,
    unavailable,
};
use crate::{
    AnalysisSession, DiagnosticsConfig, QueryAnalysisRequestError, QueryCancellation, QueryError,
    QuerySyntaxError, QuerySyntaxRequestError, QueryTextState, SessionRequestError,
};

/// Current metadata failures are unchecked query files; invariant failures retain the batch error route.
pub(super) fn check_one(
    session: &mut AnalysisSession,
    path: &SourcePath,
    config: &DiagnosticsConfig,
    request: &InputRequest,
    cancellation: &QueryCancellation,
) -> Result<Option<BatchQueryFile>, BatchError> {
    let input = session
        .query_input(path)
        .ok_or_else(|| BatchError::Request(SessionRequestError::UnknownQuery(path.clone())))?;
    if matches!(input.state, QueryTextState::Pending)
        || (input.origin == InputOrigin::Saved
            && matches!(
                input.state,
                QueryTextState::Unavailable(InputFailure::Cancelled)
            ))
    {
        session
            .load_query_saved(path, session.revision(), request)
            .map_err(BatchError::Request)?;
    }
    session
        .prepare_query_analysis(session.revision(), request)
        .map_err(BatchError::Request)?;
    let Some(consumers) = consumers::check(session, path, request, cancellation)? else {
        if session
            .query_input(path)
            .is_some_and(|input| input.origin == InputOrigin::Saved)
        {
            session
                .release_query_saved(path, session.revision())
                .map_err(BatchError::Request)?;
        }
        return Ok(None);
    };
    let input = session
        .query_input(path)
        .ok_or_else(|| BatchError::Request(SessionRequestError::UnknownQuery(path.clone())))?;
    let mut file = unavailable(path, input);
    file.scope = BatchQueryScope::CurrentCanonicalMetadata;
    file.consumers = Some(consumers);
    file.context = Some(BatchQueryContext {
        root: session.selected_root_id(),
        input_revision: input.revision,
        composition_sha256: session.composition_sha256().map(str::to_owned),
        metadata_complete: false,
    });
    let current = input.origin;
    let result = if let QueryTextState::Ready(text) = &input.state {
        let text = std::sync::Arc::clone(text);
        match session.query_analysis_diagnostics(
            path,
            session.revision(),
            config,
            request,
            cancellation,
        ) {
            Ok(result) => {
                file.revision = result.revision;
                file.state = FileState::Checked;
                file.findings = findings(path, &text, &result.result.diagnostics)?;
                if let Some(context) = &mut file.context {
                    context.metadata_complete = true;
                }
                Ok(Some(file))
            }
            Err(QueryAnalysisRequestError::Request(
                SessionRequestError::CompositionInputIncomplete(key),
            )) => {
                file.state = FileState::Unavailable {
                    code: "analysis_composition_incomplete".into(),
                    location: vec![key.root.index().to_string(), key.source.as_str().into()],
                };
                Ok(Some(file))
            }
            Err(
                QueryAnalysisRequestError::Native(QueryError::Cancelled)
                | QueryAnalysisRequestError::Input(QuerySyntaxError::Cancelled),
            ) => Ok(None),
            Err(QueryAnalysisRequestError::Request(error)) => Err(BatchError::Request(error)),
            Err(QueryAnalysisRequestError::Native(error)) => Err(BatchError::Query(error)),
            Err(QueryAnalysisRequestError::Input(error)) => Err(BatchError::QuerySyntax(
                QuerySyntaxRequestError::Syntax(error),
            )),
            Err(QueryAnalysisRequestError::Disabled) => {
                file.state = FileState::Unavailable {
                    code: "analysis_profile_disabled".into(),
                    location: vec![],
                };
                Ok(Some(file))
            }
        }
    } else {
        Ok(Some(file))
    };
    if current == InputOrigin::Saved {
        session
            .release_query_saved(path, session.revision())
            .map_err(BatchError::Request)?;
    }
    result
}
