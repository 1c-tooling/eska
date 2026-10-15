//! Property and BSL previews share one independently pinned base and versioned-edit capability.

use std::path::PathBuf;

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
    source_root: PathBuf,
    manifest_sha256: Option<String>,
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
            source_root: source.reader().root().to_owned(),
            manifest_sha256: None,
        }))
    }

    /// Storage policy enables guarded save independently of language scope and preview capabilities.
    /// The configured path locates storage; only the explicit CLI/environment pin approves it.
    pub fn for_save(source: &EskaSource, explicit: Option<&str>) -> Result<Option<Self>, Value> {
        if explicit.is_none() && std::env::var_os("ESKA_ACCEPTED_BASE_SHA256").is_none() {
            return Ok(None);
        }
        let reference = tool_cli::accepted_ref(explicit)
            .map_err(|failure| protocol::error_value(-32602, failure.code, Value::Null))?;
        Ok(Some(Self {
            project: source.project().clone(),
            scope: source.scope().clone(),
            reference,
            source_root: source.reader().root().to_owned(),
            manifest_sha256: Some(source.reader().configuration().manifest_sha256.clone()),
        }))
    }

    /// Every preview verifies the full accepted base, with the same cooperative request cancellation.
    pub fn open(&self, control: &RequestCancellation) -> Result<AcceptedBase, Value> {
        let result = (|| {
            let source = EskaSource::open(self.project.clone(), self.scope.clone())?;
            if source.reader().root() != self.source_root
                || self.manifest_sha256.as_ref().is_some_and(|expected| {
                    source.reader().configuration().manifest_sha256 != *expected
                })
            {
                return Err(eska::project_core::source_format::Error {
                    code: "source_edit_structure_changed",
                    location: Vec::new(),
                });
            }
            source.edit_authority_cancellable(&self.reference, &|| control.is_cancelled())
        })();
        result.map_err(|error| {
            protocol::error_value(-32000, error.code, json!({"location":error.location}))
        })
    }
}
