//! Dispatch queries against the current session after validating the caller's snapshot.

use std::time::Duration;

use eska::{
    analysis_core::{InputRequest, InputState},
    project_core::source_format::SourcePath,
};
use serde_json::{Value, json};

use super::super::{
    completion, definitions, diagnostics, documents, dto, language, metadata_locations,
    property_edits, protocol,
};
use super::Server;
use crate::{AnalysisSession, RequestCancellation};

impl Server<'_> {
    /// Every custom request validates the connection identity and exact current revision first.
    pub(super) fn request(
        &mut self,
        method: &str,
        value: &Value,
        cancellation: Option<&RequestCancellation>,
    ) -> Result<Value, Value> {
        if let Some(method) = language::Method::parse(method) {
            return self.language_request(method, value, cancellation);
        }
        if method == property_edits::METHOD {
            let params: property_edits::Params = protocol::params(value)?;
            let session = self
                .session
                .as_ref()
                .ok_or_else(|| protocol::error_value(-32603, "session_invariant", Value::Null))?;
            validate_snapshot(session, &self.session_id, &params.snapshot)?;
            let cancellation = cancellation
                .ok_or_else(|| protocol::error_value(-32603, "request_invariant", Value::Null))?;
            return self
                .edit_plans
                .as_ref()
                .ok_or_else(|| {
                    protocol::error_value(-32000, "source_edit_plans_disabled", Value::Null)
                })?
                .prepare(
                    session,
                    &self.session_id,
                    &params,
                    cancellation,
                    Duration::from_secs(self.cli.timeout_seconds),
                );
        }
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| protocol::error_value(-32603, "session_invariant", Value::Null))?;
        match method {
            metadata_locations::METHOD => {
                let params: metadata_locations::Params = protocol::params(value)?;
                validate_snapshot(session, &self.session_id, &params.snapshot)?;
                metadata_locations::locate(session, &self.session_id, &params)
            }
            "eska/metadata/owners" => {
                let params: protocol::Owners = protocol::params(value)?;
                validate_snapshot(session, &self.session_id, &params.snapshot)?;
                if !(1..=200).contains(&params.limit) {
                    return Err(protocol::error_value(
                        -32602,
                        "page_limit_invalid",
                        json!({"max":200}),
                    ));
                }
                let owners = session
                    .owner_paths()
                    .skip(params.offset)
                    .take(params.limit)
                    .map(|path| dto::owner(session, path))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(
                    json!({"snapshot":dto::snapshot(session, &self.session_id), "total":session.owner_paths().count(),
                    "offset":params.offset, "owners":owners, "resources":session.resources()}),
                )
            }
            "eska/metadata/read" | "eska/metadata/refresh" => {
                let params: protocol::ReadOwner = protocol::params(value)?;
                validate_snapshot(session, &self.session_id, &params.snapshot)?;
                let path = SourcePath::parse(&params.source)
                    .map_err(|error| protocol::error_value(-32602, error.code, Value::Null))?;
                let pending = matches!(
                    session
                        .metadata(&path, session.revision())
                        .map_err(|_| protocol::error_value(-32602, "unknown_source", Value::Null))?
                        .input,
                    InputState::Pending
                );
                if method == "eska/metadata/refresh" || pending {
                    let request = InputRequest {
                        max_document_bytes: self.cli.max_document_bytes,
                        ..InputRequest::default()
                    };
                    session
                        .load_saved(&path, session.revision(), &request)
                        .map_err(|error| documents::request_error(&error))?;
                }
                let current = session
                    .metadata(&path, session.revision())
                    .map_err(|_| protocol::error_value(-32603, "session_invariant", Value::Null))?;
                Ok(
                    json!({"snapshot":dto::snapshot(session, &self.session_id), "metadata":dto::metadata(&current),
                    "resources":session.resources()}),
                )
            }
            _ => Err(protocol::error_value(
                -32601,
                "rpc_method_not_found",
                Value::Null,
            )),
        }
    }

    /// Language methods share process settings and input controls while keeping their own payloads.
    fn language_request(
        &mut self,
        method: language::Method,
        value: &Value,
        cancellation: Option<&RequestCancellation>,
    ) -> Result<Value, Value> {
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| protocol::error_value(-32603, "session_invariant", Value::Null))?;
        let cancellation = cancellation
            .ok_or_else(|| protocol::error_value(-32603, "request_invariant", Value::Null))?;
        let settings = self
            .settings
            .as_ref()
            .ok_or_else(|| protocol::error_value(-32603, "session_invariant", Value::Null))?;
        let request = InputRequest {
            max_document_bytes: self.cli.max_document_bytes,
            cancellation: cancellation.input.clone(),
            ..InputRequest::default()
        };
        let timeout = Duration::from_secs(self.cli.timeout_seconds);
        match method {
            language::Method::Diagnostic => {
                let params: diagnostics::Params = protocol::params(value)?;
                if let Some(expected) = &params.eska {
                    validate_snapshot(session, &self.session_id, &expected.snapshot)?;
                }
                diagnostics::pull(
                    session,
                    &self.session_id,
                    &params,
                    settings,
                    &diagnostics::Options {
                        request: &request,
                        cancellation,
                        timeout,
                        localizer: self.localizer,
                    },
                )
            }
            language::Method::Completion => {
                let params: completion::Params = protocol::params(value)?;
                if let Some(expected) = &params.eska {
                    validate_snapshot(session, &self.session_id, &expected.snapshot)?;
                }
                completion::pull(
                    session,
                    &params,
                    settings,
                    &completion::Options {
                        session_id: &self.session_id,
                        request,
                        cancellation,
                        timeout,
                        snippets: self.snippets,
                        extended_kinds: self.extended_kinds,
                    },
                )
            }
            language::Method::Definition => {
                let params: definitions::Params = protocol::params(value)?;
                if let Some(expected) = &params.eska {
                    validate_snapshot(session, &self.session_id, &expected.snapshot)?;
                }
                definitions::pull(
                    session,
                    &self.session_id,
                    &params,
                    settings,
                    &request,
                    cancellation,
                    timeout,
                )
            }
        }
    }
}

/// Clients copy the opaque token; incrementing it or keeping it after reconnect is invalid.
fn validate_snapshot(
    session: &AnalysisSession,
    session_id: &str,
    expected: &protocol::Snapshot,
) -> Result<(), Value> {
    if expected.session_id != session_id {
        return Err(protocol::error_value(
            -32000,
            "unknown_session",
            dto::snapshot(session, session_id),
        ));
    }
    if expected.revision != session.revision().to_string() {
        return Err(protocol::error_value(
            -32801,
            "stale_revision",
            dto::snapshot(session, session_id),
        ));
    }
    Ok(())
}
