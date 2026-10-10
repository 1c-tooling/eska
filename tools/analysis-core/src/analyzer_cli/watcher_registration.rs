//! One optional client registration watches only the fixed control inventory, never all metadata files.

use serde_json::{Value, json};

use super::{project_changes, protocol, watched_files};

pub const EVENT: &str = "eska/watchRegistration";
const METHOD: &str = "client/registerCapability";
const REQUEST_ID: &str = "eska/project-watch/1";
const REGISTRATION_ID: &str = "eska/project-control-files";
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
        "requiresRelativePatterns":true,"inputNotification":EVENT})
}

impl Registration {
    /// Build at most one small request from the already selected control paths, without reading files.
    pub fn selected(
        inputs: &project_changes::Inputs,
        params: &Value,
    ) -> Result<Option<Self>, Value> {
        if !eligible(params) {
            return Ok(None);
        }
        let request = json!({"jsonrpc":"2.0","id":REQUEST_ID,"method":METHOD,"params":{
            "registrations":[{"id":REGISTRATION_ID,"method":watched_files::METHOD,
                "registerOptions":{"watchers":inputs.watchers()?}}]}});
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
        let mut params =
            json!({"scope":SCOPE,"snapshot":snapshot,"registrationId":REGISTRATION_ID});
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
