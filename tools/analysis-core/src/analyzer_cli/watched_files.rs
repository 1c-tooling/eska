//! Client file events invalidate a bounded declared batch; lazy readers own all subsequent disk I/O.

use std::collections::BTreeSet;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::{AnalysisSession, ModuleState};

use super::{documents, dto, project_changes, protocol, source_saves::RootPublication};

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

/// Manual clients retain the explicit batch limit; negotiated globs are bounded by the transport frame.
pub fn capabilities(recursive: bool) -> Value {
    json!({"method":METHOD,"scope":"declared-saved-inputs","inputNotification":EVENT,
        "maxChanges":if recursive {None} else {Some(MAX_CHANGES)},"maxRequestBytes":documents::MAX_REQUEST,
        "unknownSourceEvents":if recursive {"ignore"} else {"reject"},
        "binaryInputs":if recursive {"acknowledge"} else {"reject"},
        "lazyRead":true,"dynamicRegistration":recursive,"compositionUpdates":false})
}

/// Emit one acknowledgement after the entire batch or reject it before accepting any input change.
pub fn notify(
    session: &mut AnalysisSession,
    id: &str,
    value: &Value,
    controls: &project_changes::Inputs,
    recursive: bool,
    root_publication: Option<&RootPublication>,
) -> Notification {
    match update(session, value, controls, recursive, root_publication) {
        Ok(Notification::Inputs(inputs)) => {
            Notification::Inputs(json!({"jsonrpc":"2.0","method":EVENT,"params":{
            "snapshot":dto::snapshot(session,id),"inputs":inputs["inputs"],
            "ignoredSources":inputs["ignoredSources"],"resources":session.resources()}}))
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
    recursive: bool,
    root_publication: Option<&RootPublication>,
) -> Result<Notification, Value> {
    let params: Params = protocol::params(value)?;
    if !recursive && params.changes.len() > MAX_CHANGES {
        return Err(protocol::error_value(
            -32602,
            "file_event_limit_exceeded",
            json!({"max":MAX_CHANGES}),
        ));
    }
    let mut files = BTreeSet::new();
    let mut changed_controls = std::collections::BTreeMap::new();
    let mut root_checks = std::collections::BTreeMap::new();
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
            if matches!(kind, project_changes::Kind::RootConfiguration)
                && *root_checks.entry(file.clone()).or_insert_with(|| {
                    root_publication.is_some_and(|publication| publication.matches(session, &file))
                })
            {
                files.insert(file);
                continue;
            }
            changed_controls.insert(
                file,
                project_changes::Change {
                    kind,
                    uri: event.uri,
                },
            );
        } else if session.root_ids().any(|root| {
            session
                .source_root(root)
                .is_some_and(|directory| file.starts_with(directory))
        }) {
            files.insert(file);
        } else {
            return Err(protocol::error_value(
                -32602,
                "document_outside_source",
                Value::Null,
            ));
        }
    }
    if let Some(change) = controls.active_import()? {
        // Source moves may deliver root/manifest events first; retain the guard without hiding settings changes.
        changed_controls.insert(documents::file_path(&change.uri)?, change);
    }
    if !changed_controls.is_empty() {
        return Ok(Notification::Restart(
            changed_controls.into_values().collect(),
        ));
    }
    let mut paths = Vec::new();
    let mut ignored = 0_usize;
    for file in &files {
        match documents::declared_key(session, file) {
            Ok(path) => paths.push(path),
            Err(error) if recursive && error["data"]["kind"] == "unknown_source" => ignored += 1,
            Err(error) => return Err(error),
        }
    }
    // Binary declarations have no readable input to invalidate; recursive watchers still acknowledge them.
    let invalidations = paths
        .iter()
        .filter(|path| {
            !recursive
                || !session
                    .module_at(path)
                    .is_some_and(|input| input.state == ModuleState::Binary)
        })
        .cloned()
        .collect::<Vec<_>>();
    let changed = session
        .invalidate_saved_inputs_at(&invalidations, session.revision())
        .map_err(|error| documents::request_error(&error))?;
    let inputs = paths
        .iter()
        .map(|path| {
            json!({"source":path.source,"root":path.root,"invalidated":changed.binary_search(path).is_ok(),
                "input":documents::current_at(session,path)})
        })
        .collect::<Vec<_>>();
    Ok(Notification::Inputs(
        json!({"inputs":inputs,"ignoredSources":ignored}),
    ))
}
