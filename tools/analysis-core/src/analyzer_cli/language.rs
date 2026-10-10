//! Language methods share bounded input, current query errors and a first-cause cooperative timer.

use std::time::Duration;

use eska::{
    analysis_core::{InputFailure, InputOrigin, InputRequest},
    project_core::source_format::SourcePath,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    AnalysisSession, ModuleFailure, ModuleState, QueryError, RequestCancellation,
    cancellation::RequestStop,
    deadline::{Deadline, DeadlineError},
};

use super::{documents, dto, protocol};

pub const METHODS: [&str; 2] = ["textDocument/diagnostic", "textDocument/completion"];

#[derive(Clone, Copy)]
pub enum Method {
    Diagnostic,
    Completion,
}

impl Method {
    /// Only supported language requests receive transport cancellation controls.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "textDocument/diagnostic" => Some(Self::Diagnostic),
            "textDocument/completion" => Some(Self::Completion),
            _ => None,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub uri: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Context {
    pub snapshot: protocol::Snapshot,
}

/// Resolve only declared local modules before a language query can allocate a timer or read input.
pub fn source_path(session: &AnalysisSession, uri: &str) -> Result<SourcePath, Value> {
    let path = documents::source_path(session, uri)?;
    session
        .module(&path)
        .ok_or_else(|| protocol::error_value(-32602, "source_not_module", Value::Null))?;
    Ok(path)
}

/// Retry only a transient cancelled saved read; editor authority and other failed inputs remain.
pub fn load_module(
    session: &mut AnalysisSession,
    path: &SourcePath,
    request: &InputRequest,
) -> Result<(), Value> {
    let input = session
        .module(path)
        .ok_or_else(|| protocol::error_value(-32603, "session_invariant", Value::Null))?;
    let retry_cancelled = matches!(input.origin, InputOrigin::Saved)
        && matches!(
            input.state,
            ModuleState::Unavailable(ModuleFailure::Input(InputFailure::Cancelled))
        );
    if matches!(input.state, ModuleState::Pending) || retry_cancelled {
        session
            .load_module_saved(path, session.revision(), request)
            .map_err(|error| documents::request_error(&error))?;
    }
    Ok(())
}

/// Zero expires before input reading, while an already accepted client cancellation retains priority.
pub fn start(
    session: &AnalysisSession,
    id: &str,
    method: Method,
    cancellation: &RequestCancellation,
    timeout: Duration,
) -> Result<Deadline, Value> {
    if let Some(reason) = cancellation.stop_reason() {
        return Err(stopped(session, id, method, reason));
    }
    let deadline = Deadline::start(timeout, cancellation.clone()).map_err(|error| {
        let kind = match error {
            DeadlineError::TimerUnavailable => "analysis_timer_unavailable",
            DeadlineError::TimeoutOutOfRange => "analysis_timeout_out_of_range",
        };
        protocol::error_value(-32000, kind, dto::snapshot(session, id))
    })?;
    if let Some(reason) = deadline.stop_reason() {
        return Err(stopped(session, id, method, reason));
    }
    Ok(deadline)
}

/// Neither late findings nor a successful empty list may hide an elapsed deadline.
pub fn finish(
    session: &AnalysisSession,
    id: &str,
    method: Method,
    deadline: &Deadline,
    result: Result<Value, Value>,
) -> Result<Value, Value> {
    deadline
        .stop_reason()
        .map_or(result, |reason| Err(stopped(session, id, method, reason)))
}

/// `ServerCancelled` is specified for diagnostic pulls; other methods use the JSON-RPC server range.
pub fn stopped(session: &AnalysisSession, id: &str, method: Method, reason: RequestStop) -> Value {
    match reason {
        RequestStop::Cancelled => {
            protocol::error_value(-32800, "analysis_cancelled", dto::snapshot(session, id))
        }
        RequestStop::DeadlineExceeded => {
            let diagnostic = matches!(method, Method::Diagnostic);
            let mut error = protocol::error_value(
                if diagnostic { -32802 } else { -32000 },
                "analysis_deadline_exceeded",
                dto::snapshot(session, id),
            );
            if diagnostic {
                error["data"]["retriggerRequest"] = json!(false);
            }
            error
        }
    }
}

/// Missing/binary/refused modules remain coded failures with their current state, never empty items.
pub fn query_error(session: &AnalysisSession, id: &str, error: &QueryError) -> Value {
    match error {
        QueryError::Request(error) => documents::request_error(error),
        QueryError::ModuleUnavailable { path, .. } => protocol::error_value(
            -32000,
            "module_unavailable",
            json!({"snapshot":dto::snapshot(session,id),"source":path,
                "input":session.module(path).map(dto::module),"resources":session.resources()}),
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
            protocol::error_value(-32602, "position_invalid", dto::snapshot(session, id))
        }
    }
}
