//! Negotiated standalone query pulls use current host text and the existing diagnostic deadline.

use eska::{
    analysis_core::{InputFailure, InputOrigin},
    project_core::source_format::SourcePath,
    tool_cli::Failure,
};
use serde_json::{Value, json};

use crate::{
    AnalysisSession, DiagnosticCode, QuerySyntaxError, QuerySyntaxRequestError, QueryTextState,
    analysis_settings::Settings, batch::BatchProfile,
};

use super::{
    diagnostics, documents, dto, language, protocol,
    request_control::{self, Method},
};

pub const IDENTIFIER: &str = "eska-query-syntax";

/// Explicit negotiation avoids accepting query buffers from clients that only know BSL/YAML sync.
pub fn negotiate(params: &Value) -> Result<bool, Value> {
    match params.pointer("/initializationOptions/eska/querySyntax") {
        None => Ok(false),
        Some(Value::Bool(enabled)) => Ok(*enabled),
        Some(_) => Err(protocol::error_value(
            -32602,
            "rpc_invalid_params",
            Value::Null,
        )),
    }
}

/// Syntax availability never advertises query metadata or consumer semantic completeness.
pub fn capability(negotiated: bool, settings: &Result<Settings, Failure>) -> Value {
    json!({"negotiated":negotiated,"enabled":negotiated && settings.as_ref().is_ok_and(|settings| !settings.diagnostics.is_disabled(DiagnosticCode::QueryParseError)),
        "languageId":"sdbl","identifier":IDENTIFIER,"scope":"syntax_only","rules":["QueryParseError"],
        "metadataComplete":false,"consumerComplete":false,"positionEncoding":"utf-16",
        "nativeCancellation":"before-and-after-parser"})
}

/// Rule preflight and the shared deadline precede lazy query reads and native parse diagnostics.
pub fn pull(
    session: &mut AnalysisSession,
    id: &str,
    path: &SourcePath,
    identifier: Option<&str>,
    settings: &Result<Settings, Failure>,
    options: &diagnostics::Options<'_>,
) -> Result<Value, Value> {
    if !options.query_syntax {
        return Err(not_negotiated());
    }
    if identifier.is_some_and(|selected| {
        selected != IDENTIFIER && selected != diagnostics::identifier(options.profile)
    }) {
        return Err(protocol::error_value(
            -32602,
            "analysis_profile_unsupported",
            json!({"supported":IDENTIFIER}),
        ));
    }
    let settings = settings.as_ref().map_err(|failure| {
        protocol::error_value(-32000, failure.code, json!({"location":failure.location}))
    })?;
    if settings
        .diagnostics
        .is_disabled(DiagnosticCode::QueryParseError)
    {
        return Err(protocol::error_value(
            -32000,
            "analysis_profile_disabled",
            json!({"profile":"query_syntax"}),
        ));
    }
    let method = Method::Language(language::Method::Diagnostic);
    let deadline =
        request_control::start(session, id, method, options.cancellation, options.timeout)?;
    let result = analyze(session, id, path, settings, options);
    request_control::finish(session, id, method, &deadline, result)
}

/// Read only a pending or cancelled saved query; failed editor authority never falls back to disk.
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
    let retry_cancelled = input.origin == InputOrigin::Saved
        && matches!(
            input.state,
            QueryTextState::Unavailable(InputFailure::Cancelled)
        );
    if matches!(input.state, QueryTextState::Pending) || retry_cancelled {
        session
            .load_query_saved(path, session.revision(), options.request)
            .map_err(|error| documents::request_error(&error))?;
    }
    let result = session
        .query_syntax_diagnostics(
            path,
            session.revision(),
            &settings.diagnostics,
            options.request,
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
    let items = diagnostics::items(&result.result.diagnostics, text, BatchProfile::Syntax)?;
    Ok(
        json!({"kind":"full","resultId":format!("{id}:{}:query-syntax:{}:{}",result.revision,settings.sha256,result.result.source_sha256),
        "items":items,"eska":{"snapshot":dto::snapshot(session,id),"profile":"query_syntax","scope":result.result.scope,
        "source":result.result.source,"source_sha256":result.result.source_sha256,"input_revision":result.result.input_revision.to_string(),
        "origin":dto::origin(result.result.origin),"config_sha256":settings.sha256,"analysis_scope":session.scope(),
        "coverage":{"query_syntax_complete":result.result.parse_enabled,"metadata_complete":false,"consumer_complete":false,"semantic_complete":false},
        "resources":session.resources()}}),
    )
}

/// A negotiation refusal precedes accepting any new query document or advancing its host revision.
pub fn not_negotiated() -> Value {
    protocol::error_value(-32602, "query_syntax_not_negotiated", Value::Null)
}

/// Current input failures remain coded and fingerprint-free; cancellation cannot publish clean findings.
fn query_error(
    session: &AnalysisSession,
    id: &str,
    path: &SourcePath,
    error: &QuerySyntaxRequestError,
) -> Value {
    match error {
        QuerySyntaxRequestError::Request(error) => documents::request_error(error),
        QuerySyntaxRequestError::Syntax(QuerySyntaxError::Cancelled) => {
            protocol::error_value(-32800, "analysis_cancelled", dto::snapshot(session, id))
        }
        QuerySyntaxRequestError::Syntax(
            QuerySyntaxError::Pending | QuerySyntaxError::Unavailable(_),
        ) => protocol::error_value(
            -32000,
            "query_unavailable",
            json!({"snapshot":dto::snapshot(session,id),"source":path,"input":documents::current(session,path),"resources":session.resources()}),
        ),
        QuerySyntaxRequestError::Syntax(QuerySyntaxError::DiagnosticRangeInvalid {
            start_byte,
            end_byte,
            ..
        }) => protocol::error_value(
            -32603,
            "diagnostic_inconsistent",
            json!({"start_byte":start_byte,"end_byte":end_byte}),
        ),
        QuerySyntaxRequestError::Syntax(QuerySyntaxError::FingerprintMissing) => {
            protocol::error_value(-32603, "session_invariant", Value::Null)
        }
    }
}
