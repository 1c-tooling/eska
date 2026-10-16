//! Explicit contextual query pulls preserve the older negotiated syntax-only route.

use eska::{
    analysis_core::{InputFailure, InputOrigin},
    project_core::source_format::SourcePath,
    tool_cli::Failure,
};
use serde_json::{Value, json};

use super::{
    diagnostics, documents, dto, language, protocol, query_consumers, query_syntax,
    request_control::{self, Method},
};
use crate::{
    AnalysisScope, AnalysisSession, QueryAnalysisRequestError, QuerySyntaxRequestError,
    QueryTextState, analysis_settings::Settings, query_analysis_enabled,
};

pub const IDENTIFIER: &str = "eska-query-analysis";

/// A separate option prevents an existing syntax client from receiving semantic query findings.
pub fn negotiate(params: &Value) -> Result<bool, Value> {
    match params.pointer("/initializationOptions/eska/queryAnalysis") {
        None => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(protocol::error_value(
            -32602,
            "rpc_invalid_params",
            Value::Null,
        )),
    }
}

/// Metadata readiness is checked per request; consumers and parameter types are never implied.
pub fn capability(
    negotiated: bool,
    settings: &Result<Settings, Failure>,
    scope: AnalysisScope,
) -> Value {
    json!({"negotiated":negotiated,"enabled":negotiated && scope.has_configuration_context()
        && settings.as_ref().is_ok_and(|settings| query_analysis_enabled(&settings.diagnostics)),
        "languageId":"sdbl","identifier":IDENTIFIER,"scope":"current_canonical_metadata",
        "rules":ide::sdbl_query_codes().into_iter().map(|code|code.as_str()).collect::<Vec<_>>(),
        "metadataReadiness":"current-visible-inputs","parametersComplete":false,
        "consumerComplete":false,"semanticComplete":false,"positionEncoding":"utf-16",
        "nativeCancellation":"salsa-and-before-after-parser"})
}

/// Admission and rule/deadline preflight precede query reads or metadata preparation.
pub fn pull(
    session: &mut AnalysisSession,
    id: &str,
    path: &SourcePath,
    settings: &Result<Settings, Failure>,
    options: &diagnostics::Options<'_>,
) -> Result<Value, Value> {
    if !options.query_analysis {
        return Err(protocol::error_value(
            -32602,
            "query_analysis_not_negotiated",
            Value::Null,
        ));
    }
    let settings = settings.as_ref().map_err(|failure| {
        protocol::error_value(-32000, failure.code, json!({"location":failure.location}))
    })?;
    if !query_analysis_enabled(&settings.diagnostics) {
        return Err(protocol::error_value(
            -32000,
            "analysis_profile_disabled",
            json!({"profile":"query_analysis"}),
        ));
    }
    session
        .require_configuration_context(session.revision())
        .map_err(|error| documents::request_error(&error))?;
    let method = Method::Language(language::Method::Diagnostic);
    let deadline =
        request_control::start(session, id, method, options.cancellation, options.timeout)?;
    let result = analyze(session, id, path, settings, options);
    request_control::finish_language(session, id, method, &deadline, result)
}

/// Current query bytes and metadata use one clock, shared source quota and cancellation boundary.
fn analyze(
    session: &mut AnalysisSession,
    id: &str,
    path: &SourcePath,
    settings: &Settings,
    options: &diagnostics::Options<'_>,
) -> Result<Value, Value> {
    let input = session
        .query_input(path)
        .ok_or_else(|| protocol::error_value(-32603, "session_invariant", Value::Null))?;
    if matches!(input.state, QueryTextState::Pending)
        || (input.origin == InputOrigin::Saved
            && matches!(
                input.state,
                QueryTextState::Unavailable(InputFailure::Cancelled)
            ))
    {
        session
            .load_query_saved(path, session.revision(), options.request)
            .map_err(|error| documents::request_error(&error))?;
    }
    session
        .prepare_query_analysis(session.revision(), options.request)
        .map_err(|error| documents::request_error(&error))?;
    let consumers = options
        .query_consumers
        .then(|| query_consumers::check(session, id, path, options))
        .transpose()?;
    let result = session
        .query_analysis_diagnostics(
            path,
            session.revision(),
            &settings.diagnostics,
            options.request,
            &options.cancellation.query,
        )
        .map_err(|error| query_error(session, id, path, &error))?;
    let input = session
        .query_input(path)
        .ok_or_else(|| protocol::error_value(-32603, "session_invariant", Value::Null))?;
    let QueryTextState::Ready(text) = &input.state else {
        return Err(protocol::error_value(
            -32603,
            "session_invariant",
            Value::Null,
        ));
    };
    let items = diagnostics::items(
        &result.result.diagnostics,
        text,
        diagnostics::DiagnosticMessages::Localized,
    )?;
    let mut response = json!({"kind":"full","resultId":format!("{id}:{}:query-analysis:{}:{}",result.revision,settings.sha256,result.result.source_sha256),
        "items":items,"eska":{"snapshot":dto::snapshot(session,id),"profile":"query_analysis","scope":"current_canonical_metadata",
        "source":result.result.source,"root":result.root,"source_sha256":result.result.source_sha256,
        "input_revision":result.result.input_revision.to_string(),"origin":dto::origin(result.result.origin),
        "config_sha256":settings.sha256,"composition_sha256":result.composition_sha256,"analysis_scope":session.scope(),
        "enabled_rules":result.result.enabled_rules.into_iter().map(|code|code.as_str()).collect::<Vec<_>>(),
        "coverage":{"query_syntax_complete":!settings.diagnostics.is_disabled(crate::DiagnosticCode::QueryParseError),
            "metadata_complete":true,"parameters_complete":false,"consumer_complete":false,"semantic_complete":false},
        "resources":session.resources()}});
    if let Some(consumers) = consumers {
        response["eska"]["consumers"] = consumers;
    }
    Ok(response)
}

/// Existing structured input/native failures keep the same code and snapshot semantics.
fn query_error(
    session: &AnalysisSession,
    id: &str,
    path: &SourcePath,
    error: &QueryAnalysisRequestError,
) -> Value {
    match error {
        QueryAnalysisRequestError::Request(error) => documents::request_error(error),
        QueryAnalysisRequestError::Input(error) => query_syntax::query_error(
            session,
            id,
            path,
            &QuerySyntaxRequestError::Syntax(error.clone()),
        ),
        QueryAnalysisRequestError::Native(error) => language::query_error(session, id, error),
        QueryAnalysisRequestError::Disabled => protocol::error_value(
            -32000,
            "analysis_profile_disabled",
            json!({"profile":"query_analysis"}),
        ),
    }
}
