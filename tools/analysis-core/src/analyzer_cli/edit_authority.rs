//! Property and BSL previews share one independently pinned base and versioned-edit capability.

use std::path::PathBuf;

use eska::{
    project_core::{
        SourceFormat,
        eska_source::EskaSource,
        source_format::{AcceptedBase, AcceptedRef},
    },
    tool_cli,
};
use serde_json::{Value, json};

use super::protocol;
use crate::RequestCancellation;

pub struct Authority {
    root: PathBuf,
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
        let SourceFormat::Eska { accepted_base } = source.project().configuration().source_format()
        else {
            return Err(protocol::error_value(
                -32603,
                "session_invariant",
                Value::Null,
            ));
        };
        Ok(Some(Self {
            root: source.project().root().join(accepted_base.as_path()),
            reference,
        }))
    }

    /// Every preview verifies the full accepted base, with the same cooperative request cancellation.
    pub fn open(&self, control: &RequestCancellation) -> Result<AcceptedBase, Value> {
        AcceptedBase::open_cancellable(&self.root, &self.reference, &|| control.is_cancelled())
            .map_err(|error| {
                protocol::error_value(-32000, error.code, json!({"location":error.location}))
            })
    }
}
