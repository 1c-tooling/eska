//! Standard signature help projects current kernel candidates and guarded caller authority.

mod presentation;
#[cfg(test)]
mod tests;

use std::time::Duration;

use eska::{analysis_core::InputRequest, tool_cli::Failure};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{AnalysisSession, RequestCancellation, analysis_settings::Settings};

use super::{
    dto, language,
    markup::Format,
    positions::Position,
    protocol,
    request_control::{self, Method},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Params {
    text_document: language::Document,
    position: Position,
    context: Option<RequestContext>,
    #[serde(rename = "workDoneToken")]
    _work_done_token: Option<protocol::Id>,
    pub eska: Option<language::Context>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RequestContext {
    trigger_kind: u8,
    trigger_character: Option<String>,
    #[serde(rename = "isRetrigger")]
    _is_retrigger: bool,
    #[serde(rename = "activeSignatureHelp")]
    _active_signature_help: Option<serde_json::Map<String, Value>>,
}

#[derive(Clone, Copy)]
pub struct Capabilities {
    pub documentation: Format,
    pub parameters: Parameters,
    context: bool,
}

/// Parameter label, selection and null support are independent negotiated presentation choices.
#[derive(Clone, Copy)]
pub struct Parameters {
    pub label_offsets: bool,
    pub active_per_signature: bool,
    pub no_active: bool,
}

impl Capabilities {
    /// Signature documentation negotiates independently of hover and completion markup options.
    pub fn negotiate(params: &Value) -> Self {
        let options = params.pointer("/capabilities/textDocument/signatureHelp");
        let information = options.and_then(|options| options.get("signatureInformation"));
        let flag = |pointer| {
            information
                .and_then(|value| value.pointer(pointer))
                .and_then(Value::as_bool)
                .unwrap_or(false)
        };
        Self {
            documentation: Format::preferred(
                information.and_then(|value| value.get("documentationFormat")),
            ),
            parameters: Parameters {
                label_offsets: flag("/parameterInformation/labelOffsetSupport"),
                active_per_signature: flag("/activeParameterSupport"),
                no_active: flag("/noActiveParameterSupport"),
            },
            context: options
                .and_then(|value| value.get("contextSupport"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }

    /// Closing-parenthesis retriggers are advertised only for clients supporting signature context.
    pub fn provider(self) -> Value {
        let mut provider = json!({"triggerCharacters":["(",","],"workDoneProgress":false});
        if self.context {
            provider["retriggerCharacters"] = json!([")"]);
        }
        provider
    }
}

pub struct Options<'a> {
    pub session_id: &'a str,
    pub request: &'a InputRequest,
    pub cancellation: &'a RequestCancellation,
    pub timeout: Duration,
    pub capabilities: Capabilities,
}

/// Native call binding recomputes selection; client active-card hints never replace current inputs.
pub fn pull(
    session: &mut AnalysisSession,
    params: &Params,
    settings: &Result<Settings, Failure>,
    options: &Options<'_>,
) -> Result<Value, Value> {
    if params.context.as_ref().is_some_and(|context| {
        !(1..=3).contains(&context.trigger_kind)
            || (context.trigger_kind == 2
                && context
                    .trigger_character
                    .as_deref()
                    .is_none_or(|value| value.chars().count() != 1))
    }) {
        return Err(protocol::error_value(
            -32602,
            "rpc_invalid_params",
            Value::Null,
        ));
    }
    let _settings = settings.as_ref().map_err(|failure| {
        protocol::error_value(-32000, failure.code, json!({"location":failure.location}))
    })?;
    let path = language::source_path(session, &params.text_document.uri)?;
    let method = Method::Language(language::Method::SignatureHelp);
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
        session
            .prepare_signature_help_metadata(
                &path,
                session.revision(),
                offset,
                options.request,
                &options.cancellation.query,
            )
            .map_err(|error| language::query_error(session, options.session_id, &error))?;
        let result = session
            .module_signature_help(
                &path,
                session.revision(),
                offset,
                &options.cancellation.query,
            )
            .map_err(|error| language::query_error(session, options.session_id, &error))?;
        let mut context = json!({"snapshot":dto::snapshot(session,options.session_id),"source":result.source,
            "source_sha256":result.source_sha256,"profile":"call-signature","position_byte":offset,
            "coverage":{"metadata_models_complete":result.metadata_models_complete,
                "module_inputs_complete":result.module_inputs_complete,"semantic_complete":false},"resources":session.resources()});
        let Some(help) = result.value else {
            if !result.metadata_models_complete || !result.module_inputs_complete {
                return Err(protocol::error_value(
                    -32000,
                    "signature_help_incomplete",
                    context,
                ));
            }
            return Ok(Value::Null);
        };
        context["active_signature"] = json!(help.active_signature);
        context["active_parameter"] = json!(help.active_parameter);
        let mut rendered = presentation::render(help, options.capabilities)?;
        rendered["eska"] = context;
        Ok(rendered)
    })();
    request_control::finish(session, options.session_id, method, &deadline, result)
}
