//! Optional consumer reports extend the current diagnostic pull without changing its legacy contract.

use eska::{project_core::source_format::SourcePath, tool_cli::Failure};
use serde_json::{Value, json};

use super::{diagnostics, documents, language, protocol, query_analysis};
use crate::{
    AnalysisScope, AnalysisSession, QueryConsumerReportStatus, QueryConsumerRequestError,
    QueryContextRequestError, analysis_settings::Settings, query_analysis_enabled,
};

/// Only an explicitly contextual client can opt into the additional report payload.
pub fn negotiate(params: &Value) -> Result<bool, Value> {
    match params.pointer("/initializationOptions/eska/queryConsumers") {
        None | Some(Value::Bool(false)) => Ok(false),
        Some(Value::Bool(true)) if query_analysis::negotiate(params)? => Ok(true),
        Some(_) => Err(protocol::error_value(
            -32602,
            "rpc_invalid_params",
            Value::Null,
        )),
    }
}

/// An independently versioned extension leaves existing metadata/parameter coverage unchanged.
pub fn capability(
    negotiated: bool,
    settings: &Result<Settings, Failure>,
    scope: AnalysisScope,
) -> Value {
    json!({"version":1,"negotiated":negotiated,
        "enabled":negotiated && scope.has_configuration_context()
            && settings.as_ref().is_ok_and(|settings|query_analysis_enabled(&settings.diagnostics)),
        "identifier":query_analysis::IDENTIFIER,"resultKey":"consumers",
        "scope":"current-query-resource-bindings","positionEncoding":"utf-16",
        "rules":["query_consumer_output_field_missing"],
        "authoringPrecision":["exact","enclosing_node","unmapped"],
        "parametersComplete":false,"consumerComplete":false,"semanticComplete":false})
}

/// Storage failures stay explicit; active cancellation and native invariants use the existing errors.
pub fn check(
    session: &mut AnalysisSession,
    id: &str,
    path: &SourcePath,
    options: &diagnostics::Options<'_>,
) -> Result<Value, Value> {
    let report = session
        .check_query_consumer_report(path, options.request, &options.cancellation.query)
        .map_err(|error| match error {
            QueryConsumerRequestError::Request(error)
            | QueryConsumerRequestError::Context(QueryContextRequestError::Request(error)) => {
                documents::request_error(&error)
            }
            QueryConsumerRequestError::Native(error) => language::query_error(session, id, &error),
            QueryConsumerRequestError::Context(QueryContextRequestError::Cancelled) => {
                language::query_error(session, id, &crate::QueryError::Cancelled)
            }
            QueryConsumerRequestError::Context(_) => {
                protocol::error_value(-32603, "session_invariant", Value::Null)
            }
        })?;
    let mut value = serde_json::to_value(&report)
        .map_err(|_| protocol::error_value(-32603, "session_invariant", Value::Null))?;
    if let QueryConsumerReportStatus::Checked { report } = &report {
        let findings = value
            .pointer_mut("/report/findings")
            .and_then(Value::as_array_mut)
            .filter(|findings| findings.len() == report.findings.len())
            .ok_or_else(|| protocol::error_value(-32603, "session_invariant", Value::Null))?;
        for (value, finding) in findings.iter_mut().zip(&report.findings) {
            let fields = value
                .as_object_mut()
                .ok_or_else(|| protocol::error_value(-32603, "session_invariant", Value::Null))?;
            fields.insert(
                "message".into(),
                json!(crate::tool_query_consumers::message(
                    finding,
                    options.localizer
                )),
            );
        }
    }
    Ok(value)
}
