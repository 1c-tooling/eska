//! Standard document pulls execute the same explicit syntax profile as the standalone checker.

#[cfg(test)]
mod tests;

use eska::{
    analysis_core::{InputFailure, InputOrigin, InputRequest},
    tool_cli::Failure,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    AnalysisSession, Diagnostic, ModuleFailure, ModuleState, QueryError, RequestCancellation,
    analysis_settings::Settings, batch::syntax_profile,
};

use super::{documents, dto, positions::Positions, protocol};

pub const IDENTIFIER: &str = "eska-syntax";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Params {
    text_document: Document,
    identifier: Option<String>,
    #[serde(rename = "previousResultId")]
    _previous_result_id: Option<String>,
    #[serde(rename = "workDoneToken")]
    _work_done_token: Option<protocol::Id>,
    #[serde(rename = "partialResultToken")]
    _partial_result_token: Option<protocol::Id>,
    pub eska: Option<Context>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    uri: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Context {
    pub snapshot: protocol::Snapshot,
}

/// Invalid settings or a disabled rule cannot be advertised as a successful empty syntax profile.
pub fn enabled(settings: &Result<Settings, Failure>) -> bool {
    settings.as_ref().is_ok_and(|settings| {
        !settings
            .diagnostics
            .is_disabled(crate::DiagnosticCode::ParseError)
    })
}

/// A configuration failure affects language analysis without disabling the independent metadata tree.
pub fn availability(settings: &Result<Settings, Failure>) -> Value {
    match settings {
        Ok(settings) => json!({"profiles":["syntax"],
            "syntaxEnabled":!settings.diagnostics.is_disabled(crate::DiagnosticCode::ParseError),
            "configSha256":settings.sha256, "configPath":settings.path, "issue":null}),
        Err(error) => {
            json!({"profiles":["syntax"],"syntaxEnabled":false,"configSha256":null,"configPath":null,
            "issue":{"code":error.code,"location":error.location}})
        }
    }
}

/// Standard clients use the current ordered input; ESKA clients may require an exact snapshot too.
pub fn pull(
    session: &mut AnalysisSession,
    id: &str,
    params: &Params,
    settings: &Result<Settings, Failure>,
    request: &InputRequest,
    cancellation: &RequestCancellation,
) -> Result<Value, Value> {
    if params
        .identifier
        .as_deref()
        .is_some_and(|identifier| identifier != IDENTIFIER)
    {
        return Err(protocol::error_value(
            -32602,
            "analysis_profile_unsupported",
            json!({"supported":IDENTIFIER}),
        ));
    }
    let settings = settings.as_ref().map_err(|failure| {
        protocol::error_value(-32000, failure.code, json!({"location":failure.location}))
    })?;
    let (config, syntax_enabled) = syntax_profile(&settings.diagnostics);
    if !syntax_enabled {
        return Err(protocol::error_value(
            -32000,
            "analysis_profile_disabled",
            json!({"profile":"syntax"}),
        ));
    }
    let path = documents::source_path(session, &params.text_document.uri)?;
    let input = session
        .module(&path)
        .ok_or_else(|| protocol::error_value(-32602, "source_not_module", Value::Null))?;
    if cancellation.is_cancelled() {
        return Err(cancelled(session, id));
    }
    // A cancelled lazy read is transient. Other failed inputs still require an explicit update.
    let retry_cancelled = matches!(input.origin, InputOrigin::Saved)
        && matches!(
            input.state,
            ModuleState::Unavailable(ModuleFailure::Input(InputFailure::Cancelled))
        );
    if matches!(input.state, ModuleState::Pending) || retry_cancelled {
        session
            .load_module_saved(&path, session.revision(), request)
            .map_err(|error| documents::request_error(&error))?;
    }
    if cancellation.is_cancelled() {
        return Err(cancelled(session, id));
    }
    let result = session
        .module_diagnostics(&path, session.revision(), &config, &cancellation.query)
        .map_err(|error| query_error(session, id, &error))?;
    let items = session
        .with_module_text(&path, session.revision(), &cancellation.query, |text| {
            items(&result.value, text)
        })
        .map_err(|error| query_error(session, id, &error))?
        .value?;
    let summary = session.summary();
    Ok(json!({"kind":"full",
        "resultId":format!("{id}:{}:{}:{}",result.revision,settings.sha256,result.source_sha256),
        "items":items,"eska":{"snapshot":dto::snapshot(session,id),"profile":"syntax",
        "source":result.source,"source_sha256":result.source_sha256,"config_sha256":settings.sha256,
        "coverage":{"module_syntax_complete":true,"metadata_inputs_complete":summary.inputs.metadata_input_complete,
            "metadata_models_complete":summary.descriptor_models_complete,"module_inputs_complete":result.module_inputs_complete,
            "semantic_complete":false},"resources":session.resources()}}))
}

/// Cancellation is a response to the original request, never an acknowledgement notification.
pub fn cancelled(session: &AnalysisSession, id: &str) -> Value {
    protocol::error_value(-32800, "analysis_cancelled", dto::snapshot(session, id))
}

/// Build UTF-16 ranges once and preserve canonical byte offsets for checker parity and future plans.
fn items(diagnostics: &[Diagnostic], text: &str) -> Result<Vec<Value>, Value> {
    let offsets = diagnostics
        .iter()
        .flat_map(|diagnostic| {
            [
                u32::from(diagnostic.range.start()),
                u32::from(diagnostic.range.end()),
            ]
        })
        .collect::<Vec<_>>();
    let positions = Positions::new(text, &offsets)?;
    diagnostics.iter().map(|diagnostic| {
        let start = u32::from(diagnostic.range.start());
        let end = u32::from(diagnostic.range.end());
        let severity = match diagnostic.severity {
            ide::Severity::Blocker | ide::Severity::Critical | ide::Severity::Major | ide::Severity::Error => 1,
            ide::Severity::Warning => 2,
            ide::Severity::Information => 3,
            ide::Severity::Hint => 4,
        };
        Ok(json!({"range":positions.range(start,end)?,"severity":severity,"code":diagnostic.code.as_str(),
            "source":"eska-analyzer","message":diagnostic.message,"data":{"start_byte":start,"end_byte":end,
                "original_severity":diagnostic.severity.as_str()}}))
    }).collect()
}

/// A missing/binary/refused module produces a coded failure with current state, never empty items.
fn query_error(session: &AnalysisSession, id: &str, error: &QueryError) -> Value {
    match error {
        QueryError::Request(error) => documents::request_error(error),
        QueryError::ModuleUnavailable { path, .. } => protocol::error_value(
            -32000,
            "module_unavailable",
            json!({
            "snapshot":dto::snapshot(session,id),"source":path,"input":session.module(path).map(dto::module),
            "resources":session.resources()}),
        ),
        QueryError::Cancelled => {
            protocol::error_value(-32800, "analysis_cancelled", dto::snapshot(session, id))
        }
        QueryError::CancellationTokenInUse => {
            protocol::error_value(-32603, "query_token_in_use", Value::Null)
        }
        QueryError::InconsistentDiagnostic {
            start_byte,
            end_byte,
        } => protocol::error_value(
            -32603,
            "diagnostic_inconsistent",
            json!({"start_byte":start_byte,"end_byte":end_byte}),
        ),
        QueryError::InvalidOffset { .. } => {
            protocol::error_value(-32602, "position_invalid", Value::Null)
        }
    }
}
