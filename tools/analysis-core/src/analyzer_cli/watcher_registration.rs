//! One optional request registers fixed control files and compact globs for declared saved inputs.

use std::collections::BTreeSet;

use eska::project_core::source_format::SourceReader;
use serde_json::{Value, json};

use super::{project_changes, protocol, watched_files};

pub const EVENT: &str = "eska/watchRegistration";
const METHOD: &str = "client/registerCapability";
const REQUEST_ID: &str = "eska/project-watch/1";
const REGISTRATION_ID: &str = "eska/project-control-files";
const SAVED_REGISTRATION_ID: &str = "eska/project-saved-inputs";
const SCOPE: &str = "project-control-inputs";

enum State {
    Planned(Value),
    Pending,
    Finished,
}

pub struct Registration(State);

/// Both flags are required; clients without relative patterns keep the explicit manual inventory.
pub fn eligible(params: &Value) -> bool {
    ["dynamicRegistration", "relativePatternSupport"]
        .iter()
        .all(|name| {
            params.pointer(&format!(
                "/capabilities/workspace/didChangeWatchedFiles/{name}"
            )) == Some(&json!(true))
        })
}

/// This describes an optional post-initialized request, not a static LSP file-watching capability.
pub fn capability(params: &Value) -> Value {
    json!({"method":METHOD,"scope":SCOPE,"eligible":eligible(params),
        "requiresRelativePatterns":true,"inputNotification":EVENT,
        "savedInputs":{"scope":"declared-saved-inputs","registrationId":SAVED_REGISTRATION_ID,
            "patternPolicy":"declared-basenames","inputNotification":watched_files::EVENT}})
}

impl Registration {
    /// Build one small request from the selected inventory, without opening payloads or scanning directories.
    pub fn selected(
        inputs: &project_changes::Inputs,
        source: &SourceReader,
        params: &Value,
    ) -> Result<Option<Self>, Value> {
        if !eligible(params) {
            return Ok(None);
        }
        let request = json!({"jsonrpc":"2.0","id":REQUEST_ID,"method":METHOD,"params":{
            "registrations":[{"id":REGISTRATION_ID,"method":watched_files::METHOD,
                "registerOptions":{"watchers":inputs.watchers()?}},
                {"id":SAVED_REGISTRATION_ID,"method":watched_files::METHOD,
                "registerOptions":{"watchers":saved_watchers(source)?}}]}});
        Ok(Some(Self(State::Planned(request))))
    }

    /// The initialized boundary sends the request once and never waits for a reply on the worker.
    pub fn request(&mut self) -> Option<Value> {
        match std::mem::replace(&mut self.0, State::Pending) {
            State::Planned(request) => Some(request),
            state => {
                self.0 = state;
                None
            }
        }
    }

    /// Only the pending server ID is a reply; duplicate, unknown and late responses cannot activate it.
    pub fn response(
        &mut self,
        response: protocol::ClientResponse,
        snapshot: &Value,
    ) -> Option<Value> {
        if !matches!(self.0, State::Pending)
            || response.id != Some(protocol::Id::String(REQUEST_ID.into()))
        {
            return None;
        }
        self.0 = State::Finished;
        let mut params = json!({"scope":SCOPE,"snapshot":snapshot,"registrationId":REGISTRATION_ID,
                "registrationIds":[REGISTRATION_ID,SAVED_REGISTRATION_ID]});
        match response.result {
            Ok(Value::Null) => params["status"] = json!("registered"),
            result => {
                params["status"] = json!("failed");
                params["error"] = match result {
                    Err(error) => json!({"kind":"client_rejected_registration","code":error.code}),
                    Ok(_) => json!({"kind":"registration_result_invalid","code":null}),
                };
            }
        }
        Some(json!({"jsonrpc":"2.0","method":EVENT,"params":params}))
    }
}

/// Generated owner/module and declared rights basenames are literal; parents remain in the base URI.
fn saved_watchers(source: &SourceReader) -> Result<Vec<Value>, Value> {
    let invalid = || protocol::error_value(-32603, "path_encoding_unsupported", Value::Null);
    let base = url::Url::from_directory_path(source.root()).map_err(|()| invalid())?;
    let names = source
        .owner_paths()
        .chain(source.module_paths())
        .chain(source.role_rights_paths())
        .filter(|path| path.as_str() != "configuration.yaml")
        .map(|path| {
            path.as_path()
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(invalid)
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    // An empty saved inventory needs no matching file yet, but the source's standard owner pattern is harmless.
    let names = if names.is_empty() {
        BTreeSet::from(["object.yaml"])
    } else {
        names
    };
    Ok(names.into_iter().map(|name|json!({"globPattern":{"baseUri":base.as_str(),"pattern":format!("**/{name}")},"kind":7})).collect())
}
