//! A selected independently pinned base enables read-only plans for versioned open YAML documents.

use std::time::Duration;

use eska::project_core::source_format::{NativeUuid, PropertyName, PropertyValue, SourcePath};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{AnalysisSession, PropertyEditError, RequestCancellation};

use super::{documents, dto, edit_authority::Authority, protocol, request_control, text_edits};

pub const METHOD: &str = "eska/metadata/preparePropertyEdit";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Params {
    pub snapshot: protocol::Snapshot,
    source: String,
    document_version: i32,
    text: String,
    uuid: NativeUuid,
    property: PropertyName,
    value: PropertyValue,
}

/// Stale input and malformed current text are refused before a full accepted-base verification.
pub fn prepare(
    authority: &Authority,
    session: &AnalysisSession,
    id: &str,
    params: &Params,
    cancellation: &RequestCancellation,
    timeout: Duration,
) -> Result<Value, Value> {
    let path = SourcePath::parse(&params.source)
        .map_err(|error| protocol::error_value(-32602, error.code, Value::Null))?;
    let input = session
        .property_edit_input(
            &path,
            session.revision(),
            i64::from(params.document_version),
            &params.text,
        )
        .map_err(error_value)?;
    let method = request_control::Method::PropertyEdit;
    let deadline = request_control::start(session, id, method, cancellation, timeout)?;
    let result = (|| {
        let base = authority.open(cancellation)?;
        let plan = input
            .prepare(&base, &params.uuid, params.property, params.value.clone())
            .map_err(error_value)?;
        let edits = text_edits::between(&params.text, plan.preview.updated())?;
        let uri = url::Url::from_file_path(session.root().join(plan.preview.path().as_path()))
            .map_err(|()| {
                protocol::error_value(-32603, "path_encoding_unsupported", Value::Null)
            })?;
        Ok(
            json!({"snapshot":dto::snapshot(session,id),"source":plan.preview.path(),
            "source_sha256":plan.source_sha256,"document_version":plan.document_version,
            "accepted_base":plan.preview.baseline(),"unchanged":plan.preview.is_empty(),
            "workspace_edit":{"documentChanges":[{"textDocument":{"uri":uri.as_str(),"version":params.document_version},"edits":edits}]}}),
        )
    })();
    request_control::finish(session, id, method, &deadline, result)
}

/// Errors describe input/policy failures without formatting human strings inside the core.
fn error_value(error: PropertyEditError) -> Value {
    match error {
        PropertyEditError::Request(error) => documents::request_error(&error),
        PropertyEditError::InputUnavailable => {
            protocol::error_value(-32000, "source_edit_input_unavailable", Value::Null)
        }
        PropertyEditError::DocumentNotOpen => {
            protocol::error_value(-32000, "source_edit_document_not_open", Value::Null)
        }
        PropertyEditError::DocumentVersion { current, received } => protocol::error_value(
            -32801,
            "source_edit_document_version",
            json!({"current":current,"received":received}),
        ),
        PropertyEditError::SourceConflict => {
            protocol::error_value(-32801, "source_edit_input_conflict", Value::Null)
        }
        PropertyEditError::Source(error) => {
            protocol::error_value(-32000, error.code, json!({"location":error.location}))
        }
    }
}
