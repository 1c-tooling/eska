//! Language methods share declared module preflight, lazy reads and current kernel failures.

use eska::{
    analysis_core::{InputFailure, InputOrigin, InputRequest},
    project_core::source_format::SourcePath,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{AnalysisSession, ModuleFailure, ModuleState, QueryError};

use super::{documents, dto, protocol};

pub const METHODS: [&str; 3] = [
    "textDocument/diagnostic",
    "textDocument/completion",
    "textDocument/definition",
];

#[derive(Clone, Copy)]
pub enum Method {
    Diagnostic,
    Completion,
    Definition,
}

impl Method {
    /// Parse supported language queries independently of property-plan requests.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "textDocument/diagnostic" => Some(Self::Diagnostic),
            "textDocument/completion" => Some(Self::Completion),
            "textDocument/definition" => Some(Self::Definition),
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
        QueryError::UnsupportedNavigationTarget => protocol::error_value(
            -32000,
            "navigation_target_unsupported",
            dto::snapshot(session, id),
        ),
        QueryError::InconsistentNavigation {
            start_byte,
            end_byte,
        } => protocol::error_value(
            -32603,
            "navigation_inconsistent",
            json!({"start_byte":start_byte,"end_byte":end_byte}),
        ),
        QueryError::InvalidOffset { .. } => {
            protocol::error_value(-32602, "position_invalid", dto::snapshot(session, id))
        }
    }
}
