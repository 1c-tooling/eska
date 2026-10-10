//! Standard locations retain per-occurrence current authority, including explicit search limits.

mod presentation;

use super::{
    dto, language,
    positions::Position,
    protocol,
    request_control::{self, Method},
};
use crate::{
    AnalysisSession, RequestCancellation,
    analysis_settings::Settings,
    queries::references::{MAX_REFERENCE_FILES, MAX_REFERENCE_HITS},
};
use eska::{analysis_core::InputRequest, tool_cli::Failure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Duration;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Params {
    text_document: language::Document,
    position: Position,
    context: Context,
    #[serde(rename = "workDoneToken")]
    _work_done_token: Option<protocol::Id>,
    #[serde(rename = "partialResultToken")]
    _partial_result_token: Option<protocol::Id>,
    pub eska: Option<language::Context>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Context {
    include_declaration: bool,
}

pub struct Options<'a> {
    pub session_id: &'a str,
    pub request: &'a InputRequest,
    pub cancellation: &'a RequestCancellation,
    pub timeout: Duration,
}

/// Limits describe candidate-file search and projected locations, not a heap/RSS guarantee.
pub fn capability() -> Value {
    json!({"scope":"current-ready-bsl-inputs","incompleteResults":"location-context-or-error",
        "maxFiles":MAX_REFERENCE_FILES,"maxLocations":MAX_REFERENCE_HITS})
}

/// Only confirmed current occurrences are returned; unresolved incomplete searches stay explicit.
pub fn pull(
    session: &mut AnalysisSession,
    params: &Params,
    settings: &Result<Settings, Failure>,
    options: &Options<'_>,
) -> Result<Value, Value> {
    let _settings = settings.as_ref().map_err(|failure| {
        protocol::error_value(-32000, failure.code, json!({"location":failure.location}))
    })?;
    let path = language::source_path(session, &params.text_document.uri)?;
    let method = Method::Language(language::Method::References);
    let deadline = request_control::start(
        session,
        options.session_id,
        method,
        options.cancellation,
        options.timeout,
    )?;
    let result = (|| {
        language::load_module(session, &path, options.request)?;
        let offset = session
            .with_module_text(
                &path,
                session.revision(),
                &options.cancellation.query,
                |text| params.position.offset(text),
            )
            .map_err(|error| language::query_error(session, options.session_id, &error))?
            .value
            .map_err(|mut error| {
                error["data"]["details"] = dto::snapshot(session, options.session_id);
                error
            })?;
        let result = session
            .module_references(
                &path,
                session.revision(),
                offset,
                params.context.include_declaration,
                &options.cancellation.query,
            )
            .map_err(|error| language::query_error(session, options.session_id, &error))?;
        let mut context = json!({"snapshot":dto::snapshot(session,options.session_id),"source":result.source,"source_sha256":result.source_sha256,
            "profile":"references","position_byte":offset,"include_declaration":params.context.include_declaration,
            "coverage":{"metadata_models_complete":result.metadata_models_complete,"module_inputs_complete":result.module_inputs_complete,"semantic_complete":false},
            "resources":session.resources()});
        let incomplete = !result.metadata_models_complete || !result.module_inputs_complete;
        let Some(search) = result.value else {
            return if incomplete {
                Err(protocol::error_value(
                    -32000,
                    "references_incomplete",
                    context,
                ))
            } else {
                Ok(Value::Null)
            };
        };
        context["search"] = json!({"files_scanned":search.files_scanned,"files_capped":search.files_capped,"hits_capped":search.hits_capped,
            "max_files":MAX_REFERENCE_FILES,"max_locations":MAX_REFERENCE_HITS});
        if search.occurrences.is_empty()
            && (incomplete || search.files_capped || search.hits_capped)
        {
            return Err(protocol::error_value(
                -32000,
                "references_incomplete",
                context,
            ));
        }
        presentation::render(session, &search.occurrences, &context, options)
    })();
    request_control::finish(session, options.session_id, method, &deadline, result)
}
