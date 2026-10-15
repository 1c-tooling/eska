//! Standard rename requests produce versioned previews only for eligible open method-local symbols.

mod errors;
mod presentation;

use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};

use super::{edit_authority::Authority, language, positions::Position, protocol, request_control};
use crate::{AnalysisSession, RequestCancellation, analysis_settings::Settings};
use eska::tool_cli::Failure;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrepareParams {
    text_document: language::Document,
    position: Position,
    #[serde(rename = "workDoneToken")]
    _work_done_token: Option<protocol::Id>,
    eska: Option<language::Context>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenameParams {
    text_document: language::Document,
    position: Position,
    new_name: String,
    #[serde(rename = "workDoneToken")]
    _work_done_token: Option<protocol::Id>,
    eska: Option<language::Context>,
}

pub enum Request {
    Prepare(PrepareParams),
    Rename(RenameParams),
}

impl Request {
    /// Both standard requests share the optional connection snapshot guard without flatten decoding.
    pub const fn context(&self) -> Option<&language::Context> {
        match self {
            Self::Prepare(params) => params.eska.as_ref(),
            Self::Rename(params) => params.eska.as_ref(),
        }
    }

    /// The declaration anchor is always decoded against this request's current open document.
    const fn position(&self) -> (&language::Document, Position) {
        match self {
            Self::Prepare(params) => (&params.text_document, params.position),
            Self::Rename(params) => (&params.text_document, params.position),
        }
    }

    /// Prepare requests never infer or reuse a previous replacement name.
    const fn method(&self) -> language::Method {
        match self {
            Self::Prepare(_) => language::Method::PrepareRename,
            Self::Rename(_) => language::Method::Rename,
        }
    }
}

pub struct Options<'a> {
    pub session_id: &'a str,
    pub authority: Option<&'a Authority>,
    pub cancellation: &'a RequestCancellation,
    pub timeout: Duration,
    pub request: &'a eska::analysis_core::InputRequest,
}

impl<'a> Options<'a> {
    /// Rename requests borrow the selected authority and the host's bounded input/stop control.
    pub const fn new(
        session_id: &'a str,
        authority: Option<&'a Authority>,
        cancellation: &'a RequestCancellation,
        timeout: Duration,
        request: &'a eska::analysis_core::InputRequest,
    ) -> Self {
        Self {
            session_id,
            authority,
            cancellation,
            timeout,
            request,
        }
    }
}

/// Object-form `RenameOptions` is permitted only when the client advertises prepare support.
pub fn provider(params: &Value) -> Value {
    if params.pointer("/capabilities/textDocument/rename/prepareSupport") == Some(&json!(true)) {
        json!({"prepareProvider":true,"workDoneProgress":false})
    } else {
        json!(true)
    }
}

/// The standard provider and its local proof contract are advertised together after authorization.
pub fn advertise(result: &mut Value, params: &Value) {
    result["capabilities"]["renameProvider"] = provider(params);
    result["capabilities"]["experimental"]["eska"]["localRename"] = capability();
}

/// Rename scope is intrinsic to the method binding; metadata coverage remains separate and partial.
pub fn capability() -> Value {
    json!({"scope":"method-local-bsl","openDocumentsOnly":true,"versioned":true,
        "acceptedBaseRequired":true,"conflictCheck":"current-module-names","incompleteResults":"error",
        "maxOccurrences":crate::queries::references::MAX_REFERENCE_HITS,
        "maxNameBytes":crate::queries::local_rename::MAX_RENAME_NAME_BYTES})
}

/// Closed or unsupported input is refused before accepted-base IO; no edit is applied by the server.
pub fn pull(
    session: &mut AnalysisSession,
    params: &Request,
    settings: &Result<Settings, Failure>,
    options: &Options<'_>,
) -> Result<Value, Value> {
    let _settings = settings.as_ref().map_err(|failure| {
        protocol::error_value(-32000, failure.code, json!({"location":failure.location}))
    })?;
    let authority = options
        .authority
        .ok_or_else(|| protocol::error_value(-32000, "rename_plans_disabled", Value::Null))?;
    let (document, position) = params.position();
    let path = language::source_path(session, &document.uri)?;
    let method = request_control::Method::Language(params.method());
    let deadline = request_control::start(
        session,
        options.session_id,
        method,
        options.cancellation,
        options.timeout,
    )?;
    let result = (|| {
        let offset = session
            .with_module_text(
                &path,
                session.revision(),
                &options.cancellation.query,
                |text| position.offset(text),
            )
            .map_err(|error| language::query_error(session, options.session_id, &error))?
            .value?;
        if session.composition_sha256().is_some() {
            session
                .prepare_completion_metadata(
                    &path,
                    session.revision(),
                    offset,
                    options.request,
                    &options.cancellation.query,
                )
                .map_err(|error| language::query_error(session, options.session_id, &error))?;
        }
        let result = session
            .module_local_rename_input(
                &path,
                session.revision(),
                offset,
                &options.cancellation.query,
            )
            .map_err(|error| errors::value(session, options.session_id, error))?;
        let Some(input) = result.value else {
            return if matches!(params, Request::Prepare(_)) {
                Ok(Value::Null)
            } else {
                Err(protocol::error_value(
                    -32000,
                    "rename_symbol_unsupported",
                    super::dto::snapshot(session, options.session_id),
                ))
            };
        };
        if let Request::Rename(params) = params {
            input
                .validate_new_name(&params.new_name)
                .map_err(|error| errors::value(session, options.session_id, error))?;
        }
        let base = authority.open(options.cancellation)?;
        match params {
            Request::Prepare(_) => {
                let target = input
                    .prepare(session, &base, &options.cancellation.query)
                    .map_err(|error| errors::value(session, options.session_id, error))?;
                presentation::prepare(session, &target, &result.source_sha256, options)
            }
            Request::Rename(params) => {
                let plan = input
                    .rename(
                        session,
                        &base,
                        &params.new_name,
                        &options.cancellation.query,
                    )
                    .map_err(|error| errors::value(session, options.session_id, error))?;
                presentation::rename(session, &plan, options)
            }
        }
    })();
    request_control::finish(session, options.session_id, method, &deadline, result)
}
