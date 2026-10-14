//! Property and BSL previews share one independently pinned base and versioned-edit capability.

use eska::{
    project_core::{
        Project,
        eska_source::EskaSource,
        metadata_model::ProjectScope,
        source_format::{AcceptedBase, AcceptedRef},
    },
    tool_cli,
};
use serde_json::{Value, json};

use super::protocol;
use crate::RequestCancellation;

pub struct Authority {
    project: Project,
    scope: ProjectScope,
    reference: AcceptedRef,
}

impl Authority {
    /// Selection advertises previews without opening accepted storage or trusting working metadata.
    pub fn selected(
        source: &EskaSource,
        explicit: Option<&str>,
        params: &Value,
    ) -> Result<Option<Self>, Value> {
        if explicit.is_none() && std::env::var_os("ESKA_ACCEPTED_BASE_SHA256").is_none() {
            return Ok(None);
        }
        let reference = tool_cli::accepted_ref(explicit)
            .map_err(|failure| protocol::error_value(-32602, failure.code, Value::Null))?;
        if params.pointer("/capabilities/workspace/workspaceEdit/documentChanges")
            != Some(&json!(true))
        {
            return Ok(None);
        }
        Ok(Some(Self {
            project: source.project().clone(),
            scope: source.scope().clone(),
            reference,
        }))
    }

    /// Every preview verifies the full accepted base, with the same cooperative request cancellation.
    pub fn open(&self, control: &RequestCancellation) -> Result<AcceptedBase, Value> {
        let result = (|| {
            EskaSource::open(self.project.clone(), self.scope.clone())?
                .edit_authority_cancellable(&self.reference, &|| control.is_cancelled())
        })();
        result.map_err(|error| {
            protocol::error_value(-32000, error.code, json!({"location":error.location}))
        })
    }
}
