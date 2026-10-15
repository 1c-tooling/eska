//! Visible foreign inputs grant synchronization membership, independently of navigation and edits.

use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};

use super::{dto, protocol, request_control};
use crate::{AnalysisSession, ModuleState, RequestCancellation};

pub const METHOD: &str = "eska/analysis/inputs";
const PAGE_LIMIT: usize = 200;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Params {
    pub snapshot: protocol::Snapshot,
    pub root: u32,
    #[serde(default)]
    pub offset: usize,
    pub limit: usize,
}

/// Root-qualified pages are declarations only; they grant no property-read or edit authority.
pub fn capability() -> Value {
    json!({"version":1,"method":METHOD,"scope":"visible-foreign-inputs",
        "pageLimit":PAGE_LIMIT,"documentSync":"existing-full-input-writer"})
}

/// Existing maps define membership without loading bytes, allocating a second registry or advancing revision.
pub fn page(
    session: &AnalysisSession,
    id: &str,
    params: &Params,
    cancellation: &RequestCancellation,
    timeout: Duration,
) -> Result<Value, Value> {
    let method = request_control::Method::InputDeclarations;
    let deadline = request_control::start(session, id, method, cancellation, timeout)?;
    let result = declarations(session, id, params);
    request_control::finish(session, id, method, &deadline, result)
}

/// A topology-visible root must be explicitly requested; selected and hidden roots cannot be substituted.
fn declarations(session: &AnalysisSession, id: &str, params: &Params) -> Result<Value, Value> {
    if !(1..=PAGE_LIMIT).contains(&params.limit) {
        return Err(protocol::error_value(
            -32602,
            "page_limit_invalid",
            json!({"max":PAGE_LIMIT}),
        ));
    }
    let root = session
        .visible_root_ids(session.selected_root_id())
        .into_iter()
        .find(|root| root.index() == params.root && *root != session.selected_root_id())
        .ok_or_else(|| {
            protocol::error_value(
                -32602,
                "analysis_root_not_visible",
                json!({"root":params.root}),
            )
        })?;
    let input = session
        .root_inputs(root)
        .map_err(|_| protocol::error_value(-32603, "session_invariant", Value::Null))?;
    let reader = input.inputs.source_reader();
    let sources = || {
        input
            .inputs
            .owner_paths()
            .map(|path| (path, "eska-source"))
            .chain(
                input
                    .modules
                    .iter()
                    .filter(|(_, entry)| !matches!(entry.model.state, ModuleState::Binary))
                    .map(|(path, _)| (path, "bsl")),
            )
            .chain(input.query_inputs.keys().map(|path| (path, "sdbl")))
            .chain(input.resource_paths().map(|path| (path, "xml")))
    };
    let total = sources().count();
    let inputs = sources()
        .skip(params.offset)
        .take(params.limit)
        .map(|(path, language)| {
            let uri =
                url::Url::from_file_path(reader.root().join(path.as_path())).map_err(|()| {
                    protocol::error_value(-32603, "path_encoding_unsupported", Value::Null)
                })?;
            Ok(json!({"root":root,"source":path,"uri":uri.as_str(),"languageId":language}))
        })
        .collect::<Result<Vec<_>, Value>>()?;
    Ok(
        json!({"snapshot":dto::snapshot(session,id),"root":root,"offset":params.offset,"total":total,"inputs":inputs}),
    )
}
