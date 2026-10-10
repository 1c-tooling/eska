//! Standard completion projects kernel candidates from the current shared YAML/BSL snapshot.

use std::time::Duration;

use eska::{analysis_core::InputRequest, tool_cli::Failure};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{AnalysisSession, CompletionItem, RequestCancellation, analysis_settings::Settings};

use super::{
    dto, language,
    positions::Position,
    protocol,
    request_control::{self, Method},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Params {
    text_document: language::Document,
    position: Position,
    context: Option<Trigger>,
    #[serde(rename = "workDoneToken")]
    _work_done_token: Option<protocol::Id>,
    #[serde(rename = "partialResultToken")]
    _partial_result_token: Option<protocol::Id>,
    pub eska: Option<language::Context>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Trigger {
    trigger_kind: u8,
    trigger_character: Option<String>,
}

pub struct Options<'a> {
    pub session_id: &'a str,
    pub request: InputRequest,
    pub cancellation: &'a RequestCancellation,
    pub timeout: Duration,
    pub snippets: bool,
    pub extended_kinds: bool,
}

/// Unsupported settings are failures; disabling the syntax rule does not disable completion.
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
    let settings = settings.as_ref().map_err(|failure| {
        protocol::error_value(-32000, failure.code, json!({"location":failure.location}))
    })?;
    let path = language::source_path(session, &params.text_document.uri)?;
    let deadline = request_control::start(
        session,
        options.session_id,
        Method::Language(language::Method::Completion),
        options.cancellation,
        options.timeout,
    )?;
    let result = analyze(session, &path, params.position, settings, options);
    request_control::finish(
        session,
        options.session_id,
        Method::Language(language::Method::Completion),
        &deadline,
        result,
    )
}

/// Current bytes are borrowed for position conversion; completion does not scan or load all YAML.
fn analyze(
    session: &mut AnalysisSession,
    path: &eska::project_core::source_format::SourcePath,
    position: Position,
    settings: &Settings,
    options: &Options<'_>,
) -> Result<Value, Value> {
    language::load_module(session, path, &options.request)?;
    let offset = session
        .with_module_text(
            path,
            session.revision(),
            &options.cancellation.query,
            |text| position.offset(text),
        )
        .map_err(|error| language::query_error(session, options.session_id, &error))?
        .value
        .map_err(|mut error| {
            error["data"]["details"] = dto::snapshot(session, options.session_id);
            error
        })?;
    let result = session
        .module_completions(
            path,
            session.revision(),
            offset,
            settings.diagnostics.locale,
            &options.cancellation.query,
        )
        .map_err(|error| language::query_error(session, options.session_id, &error))?;
    let items = result
        .value
        .into_iter()
        .filter_map(|item| item_value(item, options.snippets, options.extended_kinds))
        .collect::<Vec<_>>();
    let summary = session.summary();
    Ok(
        json!({"isIncomplete":!result.metadata_models_complete || !result.module_inputs_complete,
        "items":items,"eska":{"snapshot":dto::snapshot(session,options.session_id),
        "source":result.source,"source_sha256":result.source_sha256,"config_sha256":settings.sha256,
        "position_byte":offset,"coverage":{"metadata_inputs_complete":summary.inputs.metadata_input_complete,
            "metadata_models_complete":result.metadata_models_complete,"module_inputs_complete":result.module_inputs_complete,
            "semantic_complete":false},"resources":session.resources()}}),
    )
}

/// Only typed kernel snippets/calls use snippet syntax; unsupported clients receive plain names.
fn item_value(mut item: CompletionItem, snippets: bool, extended_kinds: bool) -> Option<Value> {
    use ide::CompletionItemKind as Kind;
    let snippet = matches!(item.kind, Kind::Snippet)
        || (matches!(item.kind, Kind::Function | Kind::Method) && item.insert_text.contains('$'));
    let format = if snippet && snippets { 2 } else { 1 };
    if snippet && !snippets {
        if item.kind == Kind::Snippet {
            return None;
        }
        item.insert_text.clone_from(&item.label);
    }
    let kind = match item.kind {
        Kind::MdoType => 7,
        Kind::MdoObject => 9,
        Kind::Field => 5,
        Kind::Property => 10,
        Kind::Function => 3,
        Kind::Method => 2,
        Kind::Keyword => 14,
        Kind::Constant if extended_kinds => 21,
        Kind::EnumMember if extended_kinds => 20,
        Kind::Constant | Kind::EnumMember => 12,
        Kind::Constructor => 4,
        Kind::Snippet => 15,
    };
    let mut value = json!({"label":item.label,"kind":kind,"insertText":item.insert_text,"insertTextFormat":format});
    if let Some(text) = item.sort_text {
        value["sortText"] = json!(text);
    }
    if let Some(text) = item.filter_text {
        value["filterText"] = json!(text);
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A client never receives raw tab stops as plain text, and literal field names stay literal.
    #[test]
    fn insertion_format_honors_capabilities_and_semantic_item_kind() {
        use ide::CompletionItemKind as Kind;
        let call = CompletionItem::simple("Call".into(), Kind::Method, "Call($0)".into());
        assert_eq!(
            item_value(call.clone(), true, false).unwrap()["insertTextFormat"],
            2
        );
        assert_eq!(
            item_value(call, false, false).unwrap()["insertText"],
            "Call"
        );
        let snippet = CompletionItem::simple("If".into(), Kind::Snippet, "If $1\n$0".into());
        assert!(item_value(snippet, false, false).is_none());
        let field = CompletionItem::simple("Literal".into(), Kind::Field, "Literal$0".into());
        assert_eq!(
            item_value(field, true, false).unwrap()["insertTextFormat"],
            1
        );
    }
}
