//! Language methods share declared module preflight, lazy reads and current kernel failures.

use eska::{
    analysis_core::{InputFailure, InputOrigin, InputRequest},
    project_core::source_format::SourcePath,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{AnalysisSession, ModuleFailure, ModuleState, QueryError};

use super::{
    completion, definitions, diagnostics, document_symbols, hover, references, rename,
    signature_help,
};
use super::{documents, dto, protocol};

pub const METHODS: [&str; 9] = [
    "textDocument/diagnostic",
    "textDocument/completion",
    "textDocument/definition",
    "textDocument/documentSymbol",
    "textDocument/hover",
    "textDocument/signatureHelp",
    "textDocument/references",
    "textDocument/prepareRename",
    "textDocument/rename",
];

#[derive(Clone, Copy)]
pub enum Method {
    Diagnostic,
    Completion,
    Definition,
    DocumentSymbol,
    Hover,
    SignatureHelp,
    References,
    PrepareRename,
    Rename,
}

impl Method {
    /// Parse supported language queries independently of property-plan requests.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "textDocument/diagnostic" => Some(Self::Diagnostic),
            "textDocument/completion" => Some(Self::Completion),
            "textDocument/definition" => Some(Self::Definition),
            "textDocument/documentSymbol" => Some(Self::DocumentSymbol),
            "textDocument/hover" => Some(Self::Hover),
            "textDocument/signatureHelp" => Some(Self::SignatureHelp),
            "textDocument/references" => Some(Self::References),
            "textDocument/prepareRename" => Some(Self::PrepareRename),
            "textDocument/rename" => Some(Self::Rename),
            _ => None,
        }
    }
}

/// Decode each standard payload before applying the common optional snapshot guard.
pub enum Request {
    Diagnostic(diagnostics::Params),
    Completion(completion::Params),
    Definition(definitions::Params),
    DocumentSymbol(document_symbols::Params),
    Hover(hover::Params),
    SignatureHelp(signature_help::Params),
    References(references::Params),
    LocalRename(rename::Request),
}

impl Request {
    /// Typed decoding retains each method's unknown-field and required-parameter checks.
    pub fn parse(method: Method, value: &Value) -> Result<Self, Value> {
        match method {
            Method::Diagnostic => protocol::params(value).map(Self::Diagnostic),
            Method::Completion => protocol::params(value).map(Self::Completion),
            Method::Definition => protocol::params(value).map(Self::Definition),
            Method::DocumentSymbol => protocol::params(value).map(Self::DocumentSymbol),
            Method::Hover => protocol::params(value).map(Self::Hover),
            Method::SignatureHelp => protocol::params(value).map(Self::SignatureHelp),
            Method::References => protocol::params(value).map(Self::References),
            Method::PrepareRename => protocol::params(value)
                .map(|params| Self::LocalRename(rename::Request::Prepare(params))),
            Method::Rename => protocol::params(value)
                .map(|params| Self::LocalRename(rename::Request::Rename(params))),
        }
    }

    /// All language request variants pin the same opaque session/revision context.
    pub const fn context(&self) -> Option<&Context> {
        match self {
            Self::Diagnostic(params) => params.eska.as_ref(),
            Self::Completion(params) => params.eska.as_ref(),
            Self::Definition(params) => params.eska.as_ref(),
            Self::DocumentSymbol(params) => params.eska.as_ref(),
            Self::Hover(params) => params.eska.as_ref(),
            Self::SignatureHelp(params) => params.eska.as_ref(),
            Self::References(params) => params.eska.as_ref(),
            Self::LocalRename(params) => params.context(),
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
        QueryError::InconsistentDocumentSymbol {
            start_byte,
            end_byte,
            selection_start_byte,
            selection_end_byte,
        } => protocol::error_value(
            -32603,
            "document_symbol_inconsistent",
            json!({"start_byte":start_byte,"end_byte":end_byte,
                "selection_start_byte":selection_start_byte,"selection_end_byte":selection_end_byte}),
        ),
        QueryError::InconsistentHover {
            start_byte,
            end_byte,
        } => protocol::error_value(
            -32603,
            "hover_inconsistent",
            json!({"start_byte":start_byte,"end_byte":end_byte}),
        ),
        QueryError::UnsupportedHoverSymbol { kind } => {
            protocol::error_value(-32000, "hover_symbol_unsupported", json!({"kind":kind}))
        }
        QueryError::InconsistentSignatureHelp {
            signature,
            parameter,
        } => protocol::error_value(
            -32603,
            "signature_help_inconsistent",
            json!({"signature":signature,"parameter":parameter}),
        ),
        QueryError::UnsupportedReferenceSymbol { category } => protocol::error_value(
            -32000,
            "references_symbol_unsupported",
            json!({"category":category}),
        ),
        QueryError::InconsistentReferences => {
            protocol::error_value(-32603, "references_inconsistent", Value::Null)
        }
    }
}
