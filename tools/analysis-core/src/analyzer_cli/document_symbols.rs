//! Standard document symbols project one current module tree, independently of the metadata tree.

use std::time::Duration;

use eska::{analysis_core::InputRequest, tool_cli::Failure};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{AnalysisSession, DocumentSymbol, RequestCancellation, analysis_settings::Settings};

use super::{
    dto, language,
    positions::Positions,
    protocol,
    request_control::{self, Method},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Params {
    text_document: language::Document,
    #[serde(rename = "workDoneToken")]
    _work_done_token: Option<protocol::Id>,
    #[serde(rename = "partialResultToken")]
    _partial_result_token: Option<protocol::Id>,
    pub eska: Option<language::Context>,
}

pub struct Options<'a> {
    pub session_id: &'a str,
    pub request: &'a InputRequest,
    pub cancellation: &'a RequestCancellation,
    pub timeout: Duration,
    pub hierarchical: bool,
}

struct Node {
    parent: Option<usize>,
    value: Value,
    children: Vec<Value>,
}

/// Hierarchy is used only when the client explicitly supports `DocumentSymbol` results.
pub fn hierarchical(params: &Value) -> bool {
    params
        .pointer("/capabilities/textDocument/documentSymbol/hierarchicalDocumentSymbolSupport")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// A syntax-only module tree never triggers eager reads of pending metadata or sibling modules.
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
    let method = Method::Language(language::Method::DocumentSymbol);
    let deadline = request_control::start(
        session,
        options.session_id,
        method,
        options.cancellation,
        options.timeout,
    )?;
    let result = (|| {
        language::load_module(session, &path, options.request)?;
        let result = session
            .module_document_symbols(&path, session.revision(), &options.cancellation.query)
            .map_err(|error| language::query_error(session, options.session_id, &error))?;
        let offsets = endpoints(&result.value);
        let positions = session
            .with_module_text(
                &path,
                session.revision(),
                &options.cancellation.query,
                |text| Positions::new(text, &offsets),
            )
            .map_err(|error| language::query_error(session, options.session_id, &error))?
            .value?;
        let uri = url::Url::from_file_path(session.root().join(path.as_path())).map_err(|()| {
            protocol::error_value(-32603, "path_encoding_unsupported", Value::Null)
        })?;
        let context = json!({"snapshot":dto::snapshot(session,options.session_id),
            "source":result.source,"source_sha256":result.source_sha256,"profile":"structure",
            "coverage":{"metadata_models_complete":result.metadata_models_complete,
                "module_inputs_complete":result.module_inputs_complete,"semantic_complete":false},
            "resources":session.resources()});
        render(result.value, &positions, uri.as_str(), &context, options)
    })();
    request_control::finish(session, options.session_id, method, &deadline, result)
}

/// Collect all current endpoints for one linear UTF-16 conversion, including nested regions.
fn endpoints(symbols: &[DocumentSymbol]) -> Vec<u32> {
    let mut pending = vec![symbols];
    let mut offsets = Vec::new();
    while let Some(symbols) = pending.pop() {
        for symbol in symbols {
            offsets.extend([
                u32::from(symbol.range.start()),
                u32::from(symbol.range.end()),
                u32::from(symbol.selection_range.start()),
                u32::from(symbol.selection_range.end()),
            ]);
            pending.push(&symbol.children);
        }
    }
    offsets
}

/// Build nodes in preorder, then attach children in reverse order without recursive conversion.
fn render(
    symbols: Vec<DocumentSymbol>,
    positions: &Positions,
    uri: &str,
    context: &Value,
    options: &Options<'_>,
) -> Result<Value, Value> {
    let mut pending: Vec<(Option<usize>, DocumentSymbol)> = symbols
        .into_iter()
        .rev()
        .map(|symbol| (None, symbol))
        .collect::<Vec<_>>();
    let mut nodes: Vec<Node> = Vec::new();
    while let Some((parent, symbol)) = pending.pop() {
        if options.cancellation.query.is_cancelled() {
            return Err(protocol::error_value(
                -32800,
                "analysis_cancelled",
                context["snapshot"].clone(),
            ));
        }
        let mut value = symbol_value(&symbol, positions, uri, context, options.hierarchical)?;
        if !options.hierarchical
            && let Some(parent) = parent
        {
            value["containerName"] = nodes.get(parent).ok_or_else(invariant)?.value["name"].clone();
        }
        let index = nodes.len();
        nodes.push(Node {
            parent,
            value,
            children: Vec::new(),
        });
        pending.extend(
            symbol
                .children
                .into_iter()
                .rev()
                .map(|symbol| (Some(index), symbol)),
        );
    }
    if !options.hierarchical {
        return Ok(Value::Array(
            nodes.into_iter().map(|node| node.value).collect(),
        ));
    }
    let mut result = Vec::new();
    while let Some(mut node) = nodes.pop() {
        if !node.children.is_empty() {
            node.children.reverse();
            node.value["children"] = Value::Array(node.children);
        }
        if let Some(parent) = node.parent {
            nodes
                .get_mut(parent)
                .ok_or_else(invariant)?
                .children
                .push(node.value);
        } else {
            result.push(node.value);
        }
    }
    result.reverse();
    Ok(Value::Array(result))
}

/// Kinds are all from the initial LSP set; only the negotiated result shape changes.
fn symbol_value(
    symbol: &DocumentSymbol,
    positions: &Positions,
    uri: &str,
    context: &Value,
    hierarchical: bool,
) -> Result<Value, Value> {
    let start = u32::from(symbol.range.start());
    let end = u32::from(symbol.range.end());
    let selection_start = u32::from(symbol.selection_range.start());
    let selection_end = u32::from(symbol.selection_range.end());
    let kind = match symbol.kind() {
        ide::SymbolKind::Procedure | ide::SymbolKind::Function => 12,
        ide::SymbolKind::Variable => 13,
        ide::SymbolKind::Region => 3,
    };
    let mut value = json!({"name":symbol.name,"kind":kind,"eska":context});
    value["eska"]["kind"] = json!(symbol.kind().as_str());
    value["eska"]["start_byte"] = json!(start);
    value["eska"]["end_byte"] = json!(end);
    value["eska"]["selection_start_byte"] = json!(selection_start);
    value["eska"]["selection_end_byte"] = json!(selection_end);
    if hierarchical {
        value["range"] = positions.range(start, end)?;
        value["selectionRange"] = positions.range(selection_start, selection_end)?;
    } else {
        value["location"] =
            json!({"uri":uri,"range":positions.range(selection_start,selection_end)?});
    }
    Ok(value)
}

/// Parent indices are internal traversal identities, never caller-controlled offsets.
fn invariant() -> Value {
    protocol::error_value(-32603, "document_symbol_invariant", Value::Null)
}
