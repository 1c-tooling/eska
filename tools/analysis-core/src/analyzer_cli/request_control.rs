//! Language queries and property plans share first-cause cancellation and cooperative deadlines.

use std::time::Duration;

use serde_json::{Value, json};

use crate::{
    AnalysisSession, RequestCancellation,
    cancellation::RequestStop,
    deadline::{Deadline, DeadlineError},
};

use super::{dto, language, property_edits, protocol};

pub const METHODS: [&str; 7] = [
    "textDocument/diagnostic",
    "textDocument/completion",
    "textDocument/definition",
    "textDocument/documentSymbol",
    "textDocument/hover",
    "textDocument/signatureHelp",
    property_edits::METHOD,
];

#[derive(Clone, Copy)]
pub enum Method {
    Language(language::Method),
    PropertyEdit,
}

impl Method {
    /// Only supported requests own transport controls; language capability stays a separate list.
    pub fn parse(name: &str) -> Option<Self> {
        if name == property_edits::METHOD {
            Some(Self::PropertyEdit)
        } else {
            language::Method::parse(name).map(Self::Language)
        }
    }
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
            let diagnostic = matches!(method, Method::Language(language::Method::Diagnostic));
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
