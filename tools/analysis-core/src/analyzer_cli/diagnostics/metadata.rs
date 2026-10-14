//! Current metadata findings become localized LSP items; no text is loaded to convert captured ranges.

use eska::{
    analysis_core::{InputFailure, InputOrigin, InputState},
    project_core::source_format::SourcePath,
};
use serde_json::{Value, json};

use crate::{AnalysisSession, MetadataDiagnosticsError, tool_metadata};

use super::{Options, documents, dto, protocol};

/// Only pending or transiently cancelled saved inputs are retried; open and failed current YAML stay authoritative.
pub(super) fn pull(
    session: &mut AnalysisSession,
    id: &str,
    path: &SourcePath,
    options: &Options<'_>,
) -> Result<Value, Value> {
    let current = session
        .metadata(path, session.revision())
        .map_err(|error| documents::request_error(&error))?;
    let retry = matches!(current.origin, InputOrigin::Saved)
        && matches!(
            current.input,
            InputState::Unavailable(InputFailure::Cancelled)
        );
    if matches!(current.input, InputState::Pending) || retry {
        session
            .load_saved(path, session.revision(), options.request)
            .map_err(|error| documents::request_error(&error))?;
    }
    session
        .load_pending_resource_for_owner(path, options.request)
        .map_err(|error| documents::request_error(&error))?;
    let report = session
        .metadata_diagnostics(path, session.revision(), &options.cancellation.query)
        .map_err(|error| failure(session, id, path, &error))?;
    let items = report
        .findings
        .iter()
        .map(|finding| {
            if options.cancellation.is_cancelled() {
                return Err(protocol::error_value(
                    -32800,
                    "analysis_cancelled",
                    dto::snapshot(session, id),
                ));
            }
            let mut value = tool_metadata::finding(finding);
            value["source"] = json!("eska-analyzer");
            value["message"] = json!(tool_metadata::message(&finding.issue, options.localizer));
            Ok(value)
        })
        .collect::<Result<Vec<_>, Value>>()?;
    let summary = session.summary();
    let mut response = json!({"kind":"full","resultId":format!("{id}:{}:metadata:{}",report.revision,report.source_sha256),
        "items":items,"eska":{"snapshot":dto::snapshot(session,id),"profile":"metadata","analysis_scope":session.scope(),
        "source":report.source,"source_sha256":report.source_sha256,"origin":dto::origin(report.origin),"config_sha256":null,
        "coverage":{"metadata_document_complete":report.input_complete,"metadata_model_complete":report.main_model_complete,
            "metadata_descriptors_complete":report.descriptor_models_complete,"metadata_inputs_complete":summary.inputs.metadata_input_complete,
            "metadata_models_complete":summary.descriptor_models_complete,"semantic_complete":false},"resources":session.resources()}});
    super::super::external_context::coverage(&mut response["eska"], session.scope());
    Ok(response)
}

/// Missing/refused YAML stays an error carrying the current state; unavailable text cannot get a fabricated range.
fn failure(
    session: &AnalysisSession,
    id: &str,
    path: &SourcePath,
    error: &MetadataDiagnosticsError,
) -> Value {
    match error {
        MetadataDiagnosticsError::Request(error) => documents::request_error(error),
        MetadataDiagnosticsError::Cancelled => {
            protocol::error_value(-32800, "analysis_cancelled", dto::snapshot(session, id))
        }
        MetadataDiagnosticsError::InconsistentCoordinates(_) => protocol::error_value(
            -32603,
            "diagnostic_range_invalid",
            dto::snapshot(session, id),
        ),
        MetadataDiagnosticsError::Pending | MetadataDiagnosticsError::Unavailable(_) => {
            protocol::error_value(
                -32000,
                "metadata_input_unavailable",
                json!({"snapshot":dto::snapshot(session,id),"source":path,"input":documents::current(session,path),"resources":session.resources()}),
            )
        }
    }
}
