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
    document_symbols, documents, dto, hover, input_declarations, kernel_memory, language,
    metadata_locations,
    navigation::Navigation,
    permissions, property_edits, protocol, references, rename, signature_help, source_saves,
};
use super::Server;
use crate::{AnalysisScope, AnalysisSession, RequestCancellation, SessionRequestError};

impl Server<'_> {
    /// Guarded publication returns a typed commit outcome before generic read response fencing.
    pub(super) fn source_save_request(
        &self,
        value: &Value,
        cancellation: Option<&RequestCancellation>,
    ) -> Result<source_saves::Committed, Value> {
        let params: source_saves::Params = protocol::params(value)?;
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| protocol::error_value(-32603, "session_invariant", Value::Null))?;
        validate_snapshot(session, &self.session_id, &params.snapshot)?;
        let cancellation = cancellation
            .ok_or_else(|| protocol::error_value(-32603, "request_invariant", Value::Null))?;
        let authority = self
            .save_authority
            .as_ref()
            .ok_or_else(|| protocol::error_value(-32000, "source_save_disabled", Value::Null))?;
        source_saves::save(
            authority,
            session,
            &self.documents,
            &params,
            &source_saves::Options {
                id: &self.session_id,
                cancellation,
                timeout: Duration::from_secs(self.cli.timeout_seconds),
            },
        )
    }

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
        if method == permissions::METHOD {
            return self.permissions_request(value, cancellation);
        }
        if method == input_declarations::METHOD {
            return self.input_declarations_request(value, cancellation);
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
                let navigation = self.navigation.as_ref().ok_or_else(|| {
                    protocol::error_value(-32603, "session_invariant", Value::Null)
                })?;
                owner_page(session, navigation, &self.session_id, &params)
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

    /// Foreign synchronization pages reuse the session's declarations and transport request controls.
    fn input_declarations_request(
        &self,
        value: &Value,
        cancellation: Option<&RequestCancellation>,
    ) -> Result<Value, Value> {
        let params: input_declarations::Params = protocol::params(value)?;
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| protocol::error_value(-32603, "session_invariant", Value::Null))?;
        validate_snapshot(session, &self.session_id, &params.snapshot)?;
        let cancellation = cancellation
            .ok_or_else(|| protocol::error_value(-32603, "request_invariant", Value::Null))?;
        input_declarations::page(
            session,
            &self.session_id,
            &params,
            cancellation,
            Duration::from_secs(self.cli.timeout_seconds),
        )
    }

    /// Policy scope is independent of semantic readiness and versioned-edit capability.
    fn permissions_request(
        &self,
        value: &Value,
        cancellation: Option<&RequestCancellation>,
    ) -> Result<Value, Value> {
        let params: permissions::Params = protocol::params(value)?;
        let invariant = || protocol::error_value(-32603, "session_invariant", Value::Null);
        let session = self.session.as_ref().ok_or_else(invariant)?;
        validate_snapshot(session, &self.session_id, &params.snapshot)?;
        let cancellation = cancellation
            .ok_or_else(|| protocol::error_value(-32603, "request_invariant", Value::Null))?;
        let authority = self.read_permissions.as_ref().ok_or_else(invariant)?;
        permissions::read(
            authority,
            session,
            &self.session_id,
            &params,
            cancellation,
            Duration::from_secs(self.cli.timeout_seconds),
        )
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
                &DiagnosticOptions::new(&request, cancellation, timeout, self.localizer, profile)
                    .with_query_syntax(self.documents.query_syntax())
                    .with_query_analysis(self.documents.query_analysis())
                    .with_query_consumers(self.documents.query_consumers()),
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
                    &request,
                ),
            ),
        }
    }
}

/// A bounded owner page combines immutable declarations with current input/model state without IO.
fn owner_page(
    session: &AnalysisSession,
    navigation: &Navigation,
    id: &str,
    params: &protocol::Owners,
) -> Result<Value, Value> {
    validate_snapshot(session, id, &params.snapshot)?;
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
        .map(|path| -> Result<Value, Value> {
            let invariant = || protocol::error_value(-32603, "session_invariant", Value::Null);
            let mut owner = dto::owner(session, path)?;
            let Value::Object(declaration) = navigation.owner(path).ok_or_else(invariant)? else {
                return Err(invariant());
            };
            owner
                .as_object_mut()
                .ok_or_else(invariant)?
                .extend(declaration);
            Ok(owner)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(
        json!({"snapshot":dto::snapshot(session,id),"total":session.owner_paths().count(),
        "offset":params.offset,"owners":owners,"resources":session.resources()}),
    )
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
    if session.scope() == AnalysisScope::LocalSyntax && !method.is_local_syntax() {
        return Err(documents::request_error(
            &SessionRequestError::ContextRequired,
        ));
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
