//! Client file events invalidate a bounded declared batch; lazy readers own all subsequent disk I/O.

use std::collections::BTreeSet;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::AnalysisSession;

use super::{documents, dto, project_changes, protocol};

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

pub enum Notification {
    Inputs(Value),
    Restart(Vec<project_changes::Change>),
}

/// Experimental consumers supply their own watcher; this capability does not request registration.
pub fn capabilities() -> Value {
    json!({"method":METHOD,"scope":"declared-saved-inputs","inputNotification":EVENT,
        "maxChanges":MAX_CHANGES,"lazyRead":true,"dynamicRegistration":false,"compositionUpdates":false})
}

/// Emit one acknowledgement after the entire batch or reject it before accepting any input change.
pub fn notify(
    session: &mut AnalysisSession,
    id: &str,
    value: &Value,
    controls: &project_changes::Inputs,
) -> Notification {
    match update(session, value, controls) {
        Ok(Notification::Inputs(inputs)) => {
            Notification::Inputs(json!({"jsonrpc":"2.0","method":EVENT,"params":{
            "snapshot":dto::snapshot(session,id),"inputs":inputs,"resources":session.resources()}}))
        }
        Ok(restart @ Notification::Restart(_)) => restart,
        Err(error) => Notification::Inputs(
            json!({"jsonrpc":"2.0","method":"eska/inputRejected","params":{
            "snapshot":dto::snapshot(session,id),"operation":METHOD,"error":error}}),
        ),
    }
}

/// Type and URI validation precede host mutation; duplicate events invalidate one path at most once.
fn update(
    session: &mut AnalysisSession,
    value: &Value,
    controls: &project_changes::Inputs,
) -> Result<Notification, Value> {
    let params: Params = protocol::params(value)?;
    if params.changes.len() > MAX_CHANGES {
        return Err(protocol::error_value(
            -32602,
            "file_event_limit_exceeded",
            json!({"max":MAX_CHANGES}),
        ));
    }
    let mut files = BTreeSet::new();
    let mut changed_controls = std::collections::BTreeMap::new();
    for event in params.changes {
        if !(1..=3).contains(&event.kind) {
            return Err(protocol::error_value(
                -32602,
                "file_event_type_invalid",
                Value::Null,
            ));
        }
        let file = documents::file_path(&event.uri)?;
        if let Some(kind) = controls.kind(&file) {
            changed_controls.insert(
                file,
                project_changes::Change {
                    kind,
                    uri: event.uri,
                },
            );
        } else if file.starts_with(session.root()) {
            files.insert(file);
        } else {
            return Err(protocol::error_value(
                -32602,
                "document_outside_source",
                Value::Null,
            ));
        }
    }
    if !changed_controls.is_empty() {
        return Ok(Notification::Restart(
            changed_controls.into_values().collect(),
        ));
    }
    let paths = files
        .iter()
        .map(|path| documents::declared_path(session, path))
        .collect::<Result<Vec<_>, _>>()?;
    let changed = session
        .invalidate_saved_inputs(&paths, session.revision())
        .map_err(|error| documents::request_error(&error))?;
    Ok(Notification::Inputs(Value::Array(
        paths
            .iter()
            .map(|path| {
                json!({"source":path,"invalidated":changed.binary_search(path).is_ok(),
                "input":documents::current(session,path)})
            })
            .collect(),
    )))
}
