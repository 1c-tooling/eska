//! Observed import guards retire the old host without touching caller-owned editor buffers.

use serde_json::{Value, json};

use super::{Server, Step, documents, dto, project_changes, protocol};

#[cfg(test)]
mod tests;

impl Server<'_> {
    /// Requests and editor inputs cannot use stale ownership while an outer import replaces source.
    pub(super) fn ensure_current(&mut self) -> Result<(), Value> {
        if let Some(state) = &self.restart {
            return Err(project_changes::error(state));
        }
        let inputs = self
            .project_inputs
            .as_ref()
            .ok_or_else(|| protocol::error_value(-32603, "session_invariant", Value::Null))?;
        if let Some(change) = inputs.active_import()? {
            let state = self.retire(&[change])?;
            return Err(project_changes::error(&state));
        }
        Ok(())
    }

    /// Rejections retain the last accepted snapshot and never acknowledge an unaccepted document version.
    pub(super) fn input_rejected(&self, method: &str, error: &Value) -> Step {
        let snapshot = self.session.as_ref().map_or_else(
            || self.restart.as_ref().map(|state| state["snapshot"].clone()),
            |session| Some(dto::snapshot(session, &self.session_id)),
        );
        let Some(snapshot) = snapshot else {
            return Step::Exit(1);
        };
        Step::Continue(Some(
            json!({"jsonrpc":"2.0","method":"eska/inputRejected","params":{
                "snapshot":snapshot,"operation":method,"error":error}}),
        ))
    }

    /// Discard results computed or serialized across an import before any response frame is written.
    pub(super) fn fence_response(&mut self, response: &mut Value) -> Result<bool, ()> {
        if self.session.is_none()
            || matches!(response["error"]["code"].as_i64(), Some(-32700 | -32600))
        {
            return Ok(false);
        }
        let Err(mut error) = self.ensure_current() else {
            return Ok(false);
        };
        if self.restart.is_none() {
            let mut state = self.retire(&[]).map_err(|_| ())?;
            state["cause"] = error["data"]["kind"].clone();
            error = project_changes::error(&state);
            self.restart = Some(state);
        }
        let method = response["method"].as_str();
        if let Some(method @ ("eska/inputChanged" | "eska/inputRejected" | "eska/inputsChanged")) =
            method
        {
            let operation = response["params"]["operation"]
                .as_str()
                .or(self.pending_operation)
                .unwrap_or(method);
            let Step::Continue(Some(rejected)) = self.input_rejected(operation, &error) else {
                return Err(());
            };
            *response = rejected;
        } else if method.is_some() {
            *response = json!({"jsonrpc":"2.0","method":project_changes::EVENT,
                "params":self.restart});
        } else {
            *response = json!({"jsonrpc":"2.0","id":response["id"],"error":error});
        }
        Ok(true)
    }

    /// Only a complete successful frame write advances the client's published input boundary.
    pub(super) fn acknowledge_response(&mut self) {
        if let Some(session) = &self.session {
            self.published_snapshot = Some(dto::snapshot(session, &self.session_id));
        }
    }

    /// File notifications and direct guard observations share one irreversible retirement boundary.
    pub(super) fn retire(&mut self, changes: &[project_changes::Change]) -> Result<Value, Value> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| protocol::error_value(-32603, "session_invariant", Value::Null))?;
        let snapshot = self
            .published_snapshot
            .clone()
            .unwrap_or_else(|| dto::snapshot(session, &self.session_id));
        let state = json!({"snapshot":snapshot,"changes":changes});
        self.session = None;
        self.settings = None;
        self.edit_plans = None;
        self.documents = documents::Documents::default();
        self.watcher = None;
        self.restart = Some(state.clone());
        Ok(state)
    }
}
