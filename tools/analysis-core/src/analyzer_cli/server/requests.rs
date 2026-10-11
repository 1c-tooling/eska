//! Dispatch queries against the current session after validating the caller's snapshot.

use std::time::Duration;

use eska::{
    analysis_core::{InputRequest, InputState},
    project_core::source_format::SourcePath,
};
use serde_json::{Value, json};

use super::super::{
    args::Cli,
    completion, definitions,
    diagnostics::{self, Options as DiagnosticOptions},
    document_symbols, documents, dto, hover, kernel_memory, language, metadata_locations,
    property_edits, protocol, references, rename, signature_help,
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
            let authority = self.edit_plans.as_ref().ok_or_else(|| {
                protocol::error_value(-32000, "source_edit_plans_disabled", Value::Null)
            })?;
            return property_edits::prepare(
                authority,
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
            kernel_memory::METHOD => {
                let params: kernel_memory::Params = protocol::params(value)?;
                validate_snapshot(session, &self.session_id, &params.snapshot)?;
                Ok(kernel_memory::report(session, &self.session_id))
            }
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
                let request = InputRequest {
                    max_document_bytes: self.cli.max_document_bytes,
                    ..InputRequest::default()
                };
                if method == "eska/metadata/refresh" || pending {
                    session
                        .load_saved(&path, session.revision(), &request)
                        .map_err(|error| documents::request_error(&error))?;
                }
                session
                    .load_pending_resource_for_owner(&path, &request)
                    .map_err(|error| documents::request_error(&error))?;
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
        let invariant = || protocol::error_value(-32603, "session_invariant", Value::Null);
        let session = self.session.as_mut().ok_or_else(invariant)?;
        let settings = self.settings.as_ref().ok_or_else(invariant)?;
        let (params, cancellation) =
            decode(session, &self.session_id, method, value, cancellation)?;
        let (request, timeout) = query_limits(self.cli, cancellation);
        let profile = self.cli.profile.into();
        match params {
            language::Request::Diagnostic(params) => diagnostics::pull(
                session,
                &self.session_id,
                &params,
                settings,
                &DiagnosticOptions::new(&request, cancellation, timeout, self.localizer, profile),
            ),
            language::Request::Completion(params) => completion::pull(
                session,
                &params,
                settings,
                &completion::Options {
                    session_id: &self.session_id,
                    request,
                    cancellation,
                    timeout,
                    capabilities: self.completion,
                },
            ),
            language::Request::Definition(params) => definitions::pull(
                session,
                &self.session_id,
                &params,
                settings,
                &request,
                cancellation,
                timeout,
            ),
            language::Request::DocumentSymbol(params) => document_symbols::pull(
                session,
                &params,
                settings,
                &document_symbols::Options {
                    session_id: &self.session_id,
                    request: &request,
                    cancellation,
                    timeout,
                    hierarchical: self.hierarchical_symbols,
                },
            ),
            language::Request::Hover(params) => hover::pull(
                session,
                &params,
                settings,
                &hover::Options {
                    session_id: &self.session_id,
                    request: &request,
                    cancellation,
                    timeout,
                    localizer: self.localizer,
                    format: self.hover_format,
                },
            ),
            language::Request::SignatureHelp(params) => signature_help::pull(
                session,
                &params,
                settings,
                &signature_help::Options {
                    session_id: &self.session_id,
                    request: &request,
                    cancellation,
                    timeout,
                    capabilities: self.signature,
                },
            ),
            language::Request::References(params) => references::pull(
                session,
                &params,
                settings,
                &references::Options {
                    session_id: &self.session_id,
                    request: &request,
                    cancellation,
                    timeout,
                },
            ),
            language::Request::LocalRename(params) => rename::pull(
                session,
                &params,
                settings,
                &rename::Options::new(
                    &self.session_id,
                    self.edit_plans.as_ref(),
                    cancellation,
                    timeout,
                ),
            ),
        }
    }
}

/// Lazy language reads inherit the same document bound and the request's input cancellation handle.
fn query_limits(cli: &Cli, cancellation: &RequestCancellation) -> (InputRequest, Duration) {
    (
        InputRequest {
            max_document_bytes: cli.max_document_bytes,
            cancellation: cancellation.input.clone(),
            ..InputRequest::default()
        },
        Duration::from_secs(cli.timeout_seconds),
    )
}

/// Typed decoding and snapshot checks precede dispatch; managed controls cannot be optional afterward.
fn decode<'a>(
    session: &AnalysisSession,
    id: &str,
    method: language::Method,
    value: &Value,
    cancellation: Option<&'a RequestCancellation>,
) -> Result<(language::Request, &'a RequestCancellation), Value> {
    let cancellation = cancellation
        .ok_or_else(|| protocol::error_value(-32603, "request_invariant", Value::Null))?;
    let params = language::Request::parse(method, value)?;
    if let Some(expected) = params.context() {
        validate_snapshot(session, id, &expected.snapshot)?;
    }
    Ok((params, cancellation))
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
