//! One serial worker owns revisions shared by editor inputs and lazy metadata requests.

use std::{
    io::{BufRead, Write},
    time::Duration,
};

use eska::{
    analysis_core::{InputRequest, InputState},
    project_core::{eska_source::EskaSource, source_format::SourcePath},
    tool_cli::{self, Failure, localization::resolve_locale_from_environment, rpc_framing},
};
use serde_json::{Value, json};

use crate::{AnalysisSession, RequestCancellation, SessionOptions, analysis_settings::Settings};

use super::{
    args::Cli, completion, definitions, diagnostics, documents, dto, language, metadata_locations,
    property_edits, protocol, request_control, stream, watched_files,
};

#[derive(Clone, Copy, Eq, PartialEq)]
enum Phase {
    New,
    Initializing,
    Ready,
    Shutdown,
}

struct Server<'a> {
    cli: &'a Cli,
    phase: Phase,
    session: Option<AnalysisSession>,
    session_id: String,
    documents: documents::Documents,
    settings: Option<Result<Settings, Failure>>,
    snippets: bool,
    extended_kinds: bool,
    edit_plans: Option<property_edits::Plans>,
}

enum Step {
    Continue(Option<Value>),
    Exit(u8),
}

/// Keep stdout framed; malformed boundaries and lost pipes terminate without guessed resync.
pub fn run(cli: &Cli, input: impl BufRead + Send + 'static, output: &mut impl Write) -> u8 {
    let Ok(incoming) = stream::Incoming::start(input) else {
        return 1;
    };
    let mut server = Server {
        cli,
        phase: Phase::New,
        session: None,
        session_id: String::new(),
        documents: documents::Documents::default(),
        settings: None,
        snippets: false,
        extended_kinds: false,
        edit_plans: None,
    };
    loop {
        let Ok(envelope) = incoming.receive() else {
            return 1;
        };
        let (step, tracked) = match envelope {
            stream::Envelope::Message(message, tracked) => (
                server.process(&message, tracked.as_ref().map(|request| &request.control)),
                tracked,
            ),
            stream::Envelope::Error(error) => (Step::Continue(Some(error)), None),
        };
        match step {
            Step::Exit(code) => return if incoming.join_after_exit() { code } else { 1 },
            Step::Continue(None) => (),
            Step::Continue(Some(mut response)) => {
                if server
                    .cancel_response(&mut response, tracked.as_ref())
                    .is_err()
                {
                    return 1;
                }
                let mut body = LimitedBody(Vec::new());
                if serde_json::to_writer(&mut body, &response).is_err() {
                    let current = server
                        .session
                        .as_ref()
                        .map(|session| dto::snapshot(session, &server.session_id));
                    let error = json!({"jsonrpc":"2.0", "id":response["id"],
                        "error":protocol::error_value(-32000,"response_too_large",
                            json!({"limit":rpc_framing::MAX_RESPONSE,"snapshot":current}))});
                    body.0.clear();
                    if serde_json::to_writer(&mut body, &error).is_err() {
                        return 1;
                    }
                }
                match server.cancel_response(&mut response, tracked.as_ref()) {
                    Err(()) => return 1,
                    Ok(true) => {
                        body.0.clear();
                        if serde_json::to_writer(&mut body, &response).is_err() {
                            return 1;
                        }
                    }
                    Ok(false) => (),
                }
                drop(tracked);
                if rpc_framing::write_frame(output, &body.0).is_err() {
                    return 1;
                }
            }
        }
    }
}

impl Server<'_> {
    /// Discard a late successful result before its wire write; retain the original request ID.
    fn cancel_response(
        &self,
        response: &mut Value,
        tracked: Option<&stream::TrackedRequest>,
    ) -> Result<bool, ()> {
        let Some((request, reason)) = tracked.and_then(|request| {
            request
                .control
                .stop_reason()
                .map(|reason| (request, reason))
        }) else {
            return Ok(false);
        };
        if response.get("result").is_none() {
            return Ok(false);
        }
        let session = self.session.as_ref().ok_or(())?;
        let fields = response.as_object_mut().ok_or(())?;
        fields.remove("result");
        fields.insert(
            "error".into(),
            request_control::stopped(session, &self.session_id, request.method, reason),
        );
        Ok(true)
    }

    /// Enforce standard lifecycle before a custom request can see or change a session.
    fn process(
        &mut self,
        message: &protocol::Message,
        cancellation: Option<&RequestCancellation>,
    ) -> Step {
        if message.id.is_none() {
            return match message.method.as_str() {
                "exit" => Step::Exit(u8::from(self.phase != Phase::Shutdown)),
                "initialized" if self.phase == Phase::Initializing => {
                    self.phase = Phase::Ready;
                    Step::Continue(None)
                }
                watched_files::METHOD if self.phase == Phase::Ready => {
                    let Some(session) = self.session.as_mut() else {
                        return Step::Exit(1);
                    };
                    Step::Continue(Some(watched_files::notify(
                        session,
                        &self.session_id,
                        &message.params,
                    )))
                }
                "textDocument/didOpen"
                | "textDocument/didChange"
                | "textDocument/didClose"
                | "textDocument/didSave"
                    if self.phase == Phase::Ready =>
                {
                    self.document_notification(message)
                }
                _ => Step::Continue(None),
            };
        }
        let result = if message.method == "initialize" {
            self.initialize(&message.params)
        } else if self.phase == Phase::Shutdown {
            Err(protocol::error_value(
                -32600,
                "server_shutdown",
                Value::Null,
            ))
        } else if self.phase != Phase::Ready {
            Err(protocol::error_value(
                -32002,
                "server_not_initialized",
                Value::Null,
            ))
        } else if message.method == "shutdown" {
            self.phase = Phase::Shutdown;
            self.session = None;
            self.documents = documents::Documents::default();
            self.settings = None;
            self.edit_plans = None;
            Ok(Value::Null)
        } else {
            self.request(&message.method, &message.params, cancellation)
        };
        Step::Continue(Some(protocol::response(message.id.as_ref(), result)))
    }

    /// Reject an incompatible ESKA protocol before project discovery or source reads.
    fn initialize(&mut self, params: &Value) -> Result<Value, Value> {
        if self.phase != Phase::New {
            return Err(protocol::error_value(
                -32600,
                "already_initialized",
                Value::Null,
            ));
        }
        if !params.get("capabilities").is_some_and(Value::is_object) {
            return Err(protocol::error_value(
                -32602,
                "rpc_invalid_params",
                Value::Null,
            ));
        }
        if params.pointer("/initializationOptions/eska/protocolVersion")
            != Some(&json!(protocol::VERSION))
        {
            return Err(protocol::error_value(
                -32602,
                "protocol_version_unsupported",
                json!({"supported":[protocol::VERSION]}),
            ));
        }
        let selected = tool_cli::selected(&self.cli.project_dir, self.cli.project.as_deref())
            .map_err(|failure| {
                protocol::error_value(-32000, failure.code, json!({"location":failure.location}))
            })?;
        let settings = crate::analysis_settings::load(
            selected.project.root(),
            None,
            resolve_locale_from_environment(self.cli.lang.as_deref()),
        );
        let source = EskaSource::open(selected.project, selected.scope).map_err(|error| {
            protocol::error_value(-32000, error.code, json!({"location":error.location}))
        })?;
        let edit_plans = property_edits::Plans::selected(
            &source,
            self.cli.accepted_base_sha256.as_deref(),
            params,
        )?;
        let options = SessionOptions {
            max_retained_source_bytes: self.cli.max_retained_source_bytes,
            ..SessionOptions::default()
        };
        let session =
            AnalysisSession::from_reader(source.into_reader(), options).map_err(|_| {
                protocol::error_value(-32000, "analysis_session_open_failed", Value::Null)
            })?;
        let session_id = bsl_metadata::Uuid::new_v4().to_string();
        let mut result = json!({
            "capabilities":{"textDocumentSync":{"openClose":true,"change":1,"save":{"includeText":false}},
                "positionEncoding":"utf-16",
                "experimental":{"eska":{
                "protocolVersion":protocol::VERSION,
                "sourceFormat":"eska", "scope":"selected-project",
                "metadata":{"owners":true, "read":true, "refresh":true,
                    "locate":{"method":metadata_locations::METHOD,"scope":"current-ready-owner","positionEncoding":"utf-16","lazyRead":false}},
                "documentSync":true, "languageQueries":true, "requestCancellation":true,
                "cancellableMethods":request_control::METHODS,"maxQueuedMessages":stream::MAX_QUEUED_MESSAGES,
                "requestTimeoutSeconds":self.cli.timeout_seconds,
                "languageMethods":language::METHODS,
                "analysis":diagnostics::availability(&settings),
                "inputNotifications":["eska/inputChanged", "eska/inputRejected", watched_files::EVENT],
                "watchedFiles":watched_files::capabilities(),
                "editPlans":edit_plans.is_some(), "locationEncoding":"utf-8-bytes",
                "maxRequestBytes":documents::MAX_REQUEST,
                "maxResponseBytes":rpc_framing::MAX_RESPONSE,
                "snapshot":dto::snapshot(&session, &session_id),
            }}},
            "serverInfo":{"name":"eska-analyzer", "version":env!("CARGO_PKG_VERSION")},
        });
        if edit_plans.is_some() {
            result["capabilities"]["experimental"]["eska"]["propertyEditPlans"] =
                json!({"method":property_edits::METHOD,"scope":"open-document","versioned":true});
        }
        if diagnostics::enabled(&settings) {
            result["capabilities"]["diagnosticProvider"] = json!({
                "identifier":diagnostics::IDENTIFIER, "interFileDependencies":false,
                "workspaceDiagnostics":false, "workDoneProgress":false});
        }
        if settings.is_ok() {
            result["capabilities"]["completionProvider"] =
                json!({"resolveProvider":false,"triggerCharacters":["."],"workDoneProgress":false});
            result["capabilities"]["definitionProvider"] = json!(true);
            result["capabilities"]["experimental"]["eska"]["definition"] = json!({"scope":"ready-bsl-and-canonical-metadata","metadataTargets":"typed-objects-and-members","incompleteResults":"error"});
        }
        self.snippets = params
            .pointer("/capabilities/textDocument/completion/completionItem/snippetSupport")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        self.extended_kinds = params
            .pointer("/capabilities/textDocument/completion/completionItemKind")
            .is_some_and(Value::is_object);
        self.edit_plans = edit_plans;
        self.session = Some(session);
        self.settings = Some(settings);
        self.session_id = session_id;
        self.phase = Phase::Initializing;
        Ok(result)
    }

    /// Full-sync notifications update the host before emitting an acknowledgement event.
    fn document_notification(&mut self, message: &protocol::Message) -> Step {
        let Some(session) = self.session.as_mut() else {
            return Step::Exit(1);
        };
        let request = InputRequest {
            max_document_bytes: self.cli.max_document_bytes,
            ..InputRequest::default()
        };
        Step::Continue(Some(self.documents.notify(
            session,
            &self.session_id,
            &message.method,
            &message.params,
            &request,
        )))
    }

    /// Every custom request validates the connection identity and exact current revision first.
    fn request(
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
                    &request,
                    cancellation,
                    timeout,
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

struct LimitedBody(Vec<u8>);

impl Write for LimitedBody {
    /// Serialization never grows the wire buffer beyond the advertised response cap.
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > rpc_framing::MAX_RESPONSE.saturating_sub(self.0.len()) {
            return Err(std::io::Error::other("response_too_large"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    /// The in-memory buffer has no deferred output.
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
