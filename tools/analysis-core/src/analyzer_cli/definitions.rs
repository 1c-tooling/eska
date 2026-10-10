//! Standard definition locations bind BSL callers and declared BSL/YAML targets to one snapshot.

use std::time::Duration;

use eska::{analysis_core::InputRequest, tool_cli::Failure};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    AnalysisSession, MetadataLocation, ModuleResult, RequestCancellation,
    analysis_settings::Settings,
};

use super::{
    dto, language,
    positions::{Position, Positions},
    protocol,
    request_control::{self, Method},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Params {
    text_document: language::Document,
    position: Position,
    #[serde(rename = "workDoneToken")]
    _work_done_token: Option<protocol::Id>,
    #[serde(rename = "partialResultToken")]
    _partial_result_token: Option<protocol::Id>,
    pub eska: Option<language::Context>,
}

/// Valid current declarations are returned even when unrelated inputs remain explicitly incomplete.
pub fn pull(
    session: &mut AnalysisSession,
    id: &str,
    params: &Params,
    settings: &Result<Settings, Failure>,
    input: &InputRequest,
    cancellation: &RequestCancellation,
    timeout: Duration,
) -> Result<Value, Value> {
    let _settings = settings.as_ref().map_err(|failure| {
        protocol::error_value(-32000, failure.code, json!({"location":failure.location}))
    })?;
    let path = language::source_path(session, &params.text_document.uri)?;
    let method = Method::Language(language::Method::Definition);
    let deadline = request_control::start(session, id, method, cancellation, timeout)?;
    let result = (|| {
        language::load_module(session, &path, input)?;
        let offset = session
            .with_module_text(&path, session.revision(), &cancellation.query, |text| {
                params.position.offset(text)
            })
            .map_err(|error| language::query_error(session, id, &error))?
            .value
            .map_err(|mut error| {
                error["data"]["details"] = dto::snapshot(session, id);
                error
            })?;
        let metadata = session
            .module_metadata_definition(&path, session.revision(), offset, &cancellation.query)
            .map_err(|error| language::query_error(session, id, &error))?;
        if metadata.value.is_some() {
            return metadata_response(session, id, metadata);
        }
        let result = session
            .module_definition(&path, session.revision(), offset, &cancellation.query)
            .map_err(|error| language::query_error(session, id, &error))?;
        let coverage = json!({"metadata_models_complete":result.metadata_models_complete,
            "module_inputs_complete":result.module_inputs_complete,"semantic_complete":false});
        let Some(target) = result.value else {
            if !result.metadata_models_complete || !result.module_inputs_complete {
                return Err(protocol::error_value(
                    -32000,
                    "navigation_incomplete",
                    json!({
                    "snapshot":dto::snapshot(session,id),"source":result.source,
                    "source_sha256":result.source_sha256,"coverage":coverage,"resources":session.resources()}),
                ));
            }
            return Ok(Value::Null);
        };
        let range = session
            .with_module_text(
                &target.source,
                session.revision(),
                &cancellation.query,
                |text| {
                    Positions::new(text, &[target.start_byte, target.end_byte])?
                        .range(target.start_byte, target.end_byte)
                },
            )
            .map_err(|error| language::query_error(session, id, &error))?
            .value?;
        let uri = url::Url::from_file_path(session.root().join(target.source.as_path())).map_err(
            |()| protocol::error_value(-32603, "path_encoding_unsupported", Value::Null),
        )?;
        Ok(json!({"uri":uri.as_str(),"range":range,"eska":{
            "snapshot":dto::snapshot(session,id),"source":result.source,"source_sha256":result.source_sha256,
            "target":{"source":target.source,"source_sha256":target.source_sha256,
                "start_byte":target.start_byte,"end_byte":target.end_byte},"coverage":coverage,"resources":session.resources()}}))
    })();
    request_control::finish(session, id, method, &deadline, result)
}

/// Canonical YAML ranges carry their own input authority rather than borrowing a module's text.
fn metadata_response(
    session: &AnalysisSession,
    id: &str,
    result: ModuleResult<Option<MetadataLocation>>,
) -> Result<Value, Value> {
    let target = result
        .value
        .ok_or_else(|| protocol::error_value(-32603, "session_invariant", Value::Null))?;
    let uri = url::Url::from_file_path(session.root().join(target.source.as_path()))
        .map_err(|()| protocol::error_value(-32603, "path_encoding_unsupported", Value::Null))?;
    Ok(json!({"uri":uri.as_str(),"range":target.range,"eska":{
        "snapshot":dto::snapshot(session,id),"source":result.source,"source_sha256":result.source_sha256,
        "target":{"kind":"metadata","source":target.source,"source_sha256":target.source_sha256,
            "start_byte":target.byte_range.start,"end_byte":target.byte_range.end,
            "uuid":target.uuid,"native_path":target.native_path,"origin":dto::origin(target.origin)},
        "coverage":{"metadata_models_complete":result.metadata_models_complete,
            "module_inputs_complete":result.module_inputs_complete,"semantic_complete":false},
        "resources":session.resources()}}))
}
