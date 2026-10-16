//! Retirement follows the published frame; it never retracts input ACK or a confirmed Save receipt.

use serde_json::{Value, json};

use super::{Phase, Server};

impl Server<'_> {
    /// Inspect only a ready serial worker after all response fences and successful frame publication.
    pub(super) fn check_resource_budget(&mut self) -> Result<Option<Value>, ()> {
        if self.phase != Phase::Ready || self.restart.is_some() || self.published_snapshot.is_none()
        {
            return Ok(None);
        }
        let Some(session) = self.session.as_ref() else {
            return Ok(None);
        };
        let Some(policy) = self.resource_budget.as_ref() else {
            return Ok(None);
        };
        let Some(report) = policy.exceeded(session) else {
            return Ok(None);
        };
        let negotiated = policy.negotiated;
        let mut state = self.retire(&[]).map_err(|_| ())?;
        state["cause"] = json!("resource_limit");
        state["resource_limit"] = report;
        self.restart = Some(state.clone());
        Ok(negotiated
            .then(|| json!({"jsonrpc":"2.0","method":"eska/restartRequired","params":state})))
    }
}
