//! Standard hover renders a typed current symbol card without forwarding kernel UI prose.

mod presentation;

use std::time::Duration;

use eska::{
    analysis_core::InputRequest,
    tool_cli::{Failure, localization::Localizer},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{AnalysisSession, RequestCancellation, analysis_settings::Settings};

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
    pub eska: Option<language::Context>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Format {
    Plaintext,
    Markdown,
}

impl Format {
    /// Use the first supported standard client preference; absent preferences receive plain text.
    pub fn negotiate(params: &Value) -> Self {
        params
            .pointer("/capabilities/textDocument/hover/contentFormat")
            .and_then(Value::as_array)
            .and_then(|formats| {
                formats.iter().find_map(|format| match format.as_str() {
                    Some("plaintext") => Some(Self::Plaintext),
                    Some("markdown") => Some(Self::Markdown),
                    _ => None,
                })
            })
            .unwrap_or(Self::Plaintext)
    }

    /// Markup kinds are wire vocabulary and never depend on the presentation locale.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Plaintext => "plaintext",
            Self::Markdown => "markdown",
        }
    }
}

pub struct Options<'a> {
    pub session_id: &'a str,
    pub request: &'a InputRequest,
    pub cancellation: &'a RequestCancellation,
    pub timeout: Duration,
    pub localizer: &'a Localizer,
    pub format: Format,
}

/// A resolved card can be shown with explicit incomplete coverage; absent cards require full inputs.
pub fn pull(
    session: &mut AnalysisSession,
    params: &Params,
    settings: &Result<Settings, Failure>,
    options: &Options<'_>,
) -> Result<Value, Value> {
    let settings = settings.as_ref().map_err(|failure| {
        protocol::error_value(-32000, failure.code, json!({"location":failure.location}))
    })?;
    let path = language::source_path(session, &params.text_document.uri)?;
    let method = Method::Language(language::Method::Hover);
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
            .module_symbol_hover(
                &path,
                session.revision(),
                offset,
                settings.diagnostics.locale,
                &options.cancellation.query,
            )
            .map_err(|error| language::query_error(session, options.session_id, &error))?;
        let mut context = json!({"snapshot":dto::snapshot(session,options.session_id),
            "source":result.source,"source_sha256":result.source_sha256,"profile":"symbol",
            "position_byte":offset,"coverage":{"metadata_models_complete":result.metadata_models_complete,
                "module_inputs_complete":result.module_inputs_complete,"semantic_complete":false},
            "resources":session.resources()});
        let Some(card) = result.value else {
            if !result.metadata_models_complete || !result.module_inputs_complete {
                return Err(protocol::error_value(-32000, "hover_incomplete", context));
            }
            return Ok(Value::Null);
        };
        let range = session
            .with_module_text(
                &path,
                session.revision(),
                &options.cancellation.query,
                |text| {
                    Positions::new(text, &[card.start_byte, card.end_byte])?
                        .range(card.start_byte, card.end_byte)
                },
            )
            .map_err(|error| language::query_error(session, options.session_id, &error))?
            .value?;
        context["kind"] = json!(card.kind);
        context["start_byte"] = json!(card.start_byte);
        context["end_byte"] = json!(card.end_byte);
        Ok(json!({"contents":{"kind":options.format.as_str(),
            "value":presentation::render(&card, options.format, options.localizer)},
            "range":range,"eska":context}))
    })();
    request_control::finish(session, options.session_id, method, &deadline, result)
}

#[cfg(test)]
mod tests;
