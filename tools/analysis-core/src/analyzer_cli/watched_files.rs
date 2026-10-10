//! Client file events invalidate a bounded declared batch; lazy readers own all subsequent disk I/O.

use std::collections::BTreeSet;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::AnalysisSession;

use super::{documents, dto, protocol};

pub const METHOD: &str = "workspace/didChangeWatchedFiles";
pub const EVENT: &str = "eska/inputsChanged";
pub const MAX_CHANGES: usize = 200;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    changes: Vec<FileEvent>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileEvent {
    uri: String,
    #[serde(rename = "type")]
    kind: u8,
}

/// Experimental consumers supply their own watcher; this capability does not request registration.
pub fn capabilities() -> Value {
    json!({"method":METHOD,"scope":"declared-saved-inputs","inputNotification":EVENT,
        "maxChanges":MAX_CHANGES,"lazyRead":true,"dynamicRegistration":false,"compositionUpdates":false})
}

/// Emit one acknowledgement after the entire batch or reject it before accepting any input change.
pub fn notify(session: &mut AnalysisSession, id: &str, value: &Value) -> Value {
    match update(session, value) {
        Ok(inputs) => json!({"jsonrpc":"2.0","method":EVENT,"params":{
            "snapshot":dto::snapshot(session,id),"inputs":inputs,"resources":session.resources()}}),
        Err(error) => json!({"jsonrpc":"2.0","method":"eska/inputRejected","params":{
            "snapshot":dto::snapshot(session,id),"operation":METHOD,"error":error}}),
    }
}

/// Type and URI validation precede host mutation; duplicate events invalidate one path at most once.
fn update(session: &mut AnalysisSession, value: &Value) -> Result<Vec<Value>, Value> {
    let params: Params = protocol::params(value)?;
    if params.changes.len() > MAX_CHANGES {
        return Err(protocol::error_value(
            -32602,
            "file_event_limit_exceeded",
            json!({"max":MAX_CHANGES}),
        ));
    }
    let mut paths = BTreeSet::new();
    for event in params.changes {
        if !(1..=3).contains(&event.kind) {
            return Err(protocol::error_value(
                -32602,
                "file_event_type_invalid",
                Value::Null,
            ));
        }
        paths.insert(documents::source_path(session, &event.uri)?);
    }
    let paths = paths.into_iter().collect::<Vec<_>>();
    let changed = session
        .invalidate_saved_inputs(&paths, session.revision())
        .map_err(|error| documents::request_error(&error))?;
    Ok(paths
        .iter()
        .map(|path| {
            json!({"source":path,"invalidated":changed.binary_search(path).is_ok(),
                "input":documents::current(session,path)})
        })
        .collect())
}
