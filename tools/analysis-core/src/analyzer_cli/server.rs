//! One serial worker owns revisions shared by editor inputs and lazy metadata requests.

use std::io::{BufRead, Write};

use eska::{
    analysis_core::InputRequest,
    tool_cli::{Failure, localization::Localizer, rpc_framing},
};
use serde_json::{Value, json};

use crate::{AnalysisSession, RequestCancellation, analysis_settings::Settings};

use super::{
    args::Cli, completion, documents, dto, edit_authority, hover, navigation::Navigation,
    project_changes, protocol, read_permissions::ReadPermissions, request_control, signature_help,
    source_saves, stream, watched_files, watcher_registration,
};

mod initialize;
mod project_guard;
mod requests;
mod response;
#[cfg(test)]
mod source_save_tests;

#[derive(Clone, Copy, Eq, PartialEq)]
enum Phase {
    New,
    Initializing,
    Ready,
    Shutdown,
}

struct Server<'a> {
    cli: &'a Cli,
    localizer: &'a Localizer,
    phase: Phase,
    session: Option<AnalysisSession>,
    session_id: String,
    documents: documents::Documents,
    settings: Option<Result<Settings, Failure>>,
    completion: completion::Capabilities,
    hierarchical_symbols: bool,
    hover_format: hover::Format,
    signature: signature_help::Capabilities,
    edit_plans: Option<edit_authority::Authority>,
    save_authority: Option<edit_authority::Authority>,
    root_publication: Option<source_saves::RootPublication>,
    navigation: Option<Navigation>,
    read_permissions: Option<ReadPermissions>,
    project_inputs: Option<project_changes::Inputs>,
    restart: Option<Value>,
    watcher: Option<watcher_registration::Registration>,
    published_snapshot: Option<Value>,
    pending_operation: Option<&'static str>,
}

enum Step {
    Continue(Option<Value>),
    Committed(Value),
    Exit(u8),
}

/// Keep stdout framed; malformed boundaries and lost pipes terminate without guessed resync.
pub fn run(
    cli: &Cli,
    localizer: &Localizer,
    input: impl BufRead + Send + 'static,
    output: &mut impl Write,
) -> u8 {
    let Ok(incoming) = stream::Incoming::start(input) else {
        return 1;
    };
    let mut server = Server {
        cli,
        localizer,
        phase: Phase::New,
        session: None,
        session_id: String::new(),
        documents: documents::Documents::default(),
        settings: None,
        completion: completion::Capabilities::default(),
        hierarchical_symbols: false,
        hover_format: hover::Format::Plaintext,
        signature: signature_help::Capabilities::negotiate(&Value::Null),
        edit_plans: None,
        save_authority: None,
        root_publication: None,
        navigation: None,
        read_permissions: None,
        project_inputs: None,
        restart: None,
        watcher: None,
        published_snapshot: None,
        pending_operation: None,
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
            stream::Envelope::Response(response) => (server.client_response(response), None),
        };
        if let Step::Exit(code) = step {
            return if incoming.join_after_exit() { code } else { 1 };
        }
        let Ok(body) = server.encode_step_response(step, tracked.as_ref()) else {
            return 1;
        };
        if let Some(body) = body {
            drop(tracked);
            if rpc_framing::write_frame(output, &body).is_err() {
                return 1;
            }
            server.acknowledge_response();
        }
    }
}

impl Server<'_> {
    /// A confirmed publication bypasses read-result cancellation and import fences through wire encoding.
    fn encode_step_response(
        &mut self,
        step: Step,
        tracked: Option<&stream::TrackedRequest>,
    ) -> Result<Option<Vec<u8>>, ()> {
        let mut response = match step {
            Step::Committed(response) => {
                return response::encode(&response, None, rpc_framing::MAX_RESPONSE)
                    .map(Some)
                    .map_err(|_| ());
            }
            Step::Continue(Some(response)) => response,
            Step::Continue(None) => return Ok(None),
            Step::Exit(_) => return Err(()),
        };
        self.cancel_response(&mut response, tracked)?;
        self.fence_response(&mut response)?;
        let current = self
            .session
            .as_ref()
            .map(|session| dto::snapshot(session, &self.session_id));
        let mut body = response::encode(&response, current.as_ref(), rpc_framing::MAX_RESPONSE)
            .map_err(|_| ())?;
        let cancelled = self.cancel_response(&mut response, tracked)?;
        let fenced = self.fence_response(&mut response)?;
        if cancelled || fenced {
            response::serialize(&response, &mut body, rpc_framing::MAX_RESPONSE).map_err(|_| ())?;
        }
        Ok(Some(body))
    }

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
        self.pending_operation = match message.method.as_str() {
            "textDocument/didOpen" => Some("textDocument/didOpen"),
            "textDocument/didChange" => Some("textDocument/didChange"),
            "textDocument/didClose" => Some("textDocument/didClose"),
            "textDocument/didSave" => Some("textDocument/didSave"),
            watched_files::METHOD => Some(watched_files::METHOD),
            _ => None,
        };
        if message.id.is_none() {
            return match message.method.as_str() {
                "exit" => Step::Exit(u8::from(self.phase != Phase::Shutdown)),
                "initialized" if self.phase == Phase::Initializing => {
                    self.phase = Phase::Ready;
                    Step::Continue(
                        self.watcher
                            .as_mut()
                            .and_then(watcher_registration::Registration::request),
                    )
                }
                watched_files::METHOD if self.phase == Phase::Ready => {
                    self.watched_notification(message)
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
            self.save_authority = None;
            self.root_publication = None;
            self.navigation = None;
            self.read_permissions = None;
            self.project_inputs = None;
            self.restart = None;
            self.watcher = None;
            self.published_snapshot = None;
            Ok(Value::Null)
        } else if let Err(error) = self.ensure_current() {
            Err(error)
        } else if message.method == source_saves::METHOD {
            return match self.source_save_request(&message.params, cancellation) {
                Ok(source_saves::Committed {
                    value,
                    root_publication,
                }) => {
                    if let (Some(publication), Some(permissions)) =
                        (&root_publication, &mut self.read_permissions)
                    {
                        let (source, sha256) = publication.fingerprint();
                        permissions.acknowledge_root_publication(source, sha256);
                    }
                    if root_publication.is_some() {
                        self.root_publication = root_publication;
                    }
                    Step::Committed(protocol::response(message.id.as_ref(), Ok(value)))
                }
                Err(error) => {
                    Step::Continue(Some(protocol::response(message.id.as_ref(), Err(error))))
                }
            };
        } else {
            self.request(&message.method, &message.params, cancellation)
        };
        Step::Continue(Some(protocol::response(message.id.as_ref(), result)))
    }

    /// Full-sync notifications update the host before emitting an acknowledgement event.
    fn document_notification(&mut self, message: &protocol::Message) -> Step {
        if let Err(error) = self.ensure_current() {
            return self.input_rejected(&message.method, &error);
        }
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

    /// Client replies share the bounded input queue but never receive JSON-RPC responses of their own.
    fn client_response(&mut self, response: protocol::ClientResponse) -> Step {
        self.pending_operation = None;
        if self.phase != Phase::Ready {
            return Step::Continue(None);
        }
        let (Some(watcher), Some(session)) = (self.watcher.as_mut(), self.session.as_ref()) else {
            return Step::Continue(None);
        };
        Step::Continue(watcher.response(response, &dto::snapshot(session, &self.session_id)))
    }

    /// Retire the old host after a control event; only an explicit process restart can reopen it.
    fn watched_notification(&mut self, message: &protocol::Message) -> Step {
        if let Some(state) = &self.restart {
            return Step::Continue(Some(
                json!({"jsonrpc":"2.0","method":"eska/inputRejected","params":{
                "snapshot":state["snapshot"],"operation":message.method,"error":project_changes::error(state)}}),
            ));
        }
        let (Some(session), Some(controls)) = (self.session.as_mut(), self.project_inputs.as_ref())
        else {
            return Step::Exit(1);
        };
        match watched_files::notify(
            session,
            &self.session_id,
            &message.params,
            controls,
            self.watcher.is_some(),
            self.root_publication.as_ref(),
        ) {
            watched_files::Notification::Inputs(event) => Step::Continue(Some(event)),
            watched_files::Notification::Restart(changes) => {
                let Ok(state) = self.retire(&changes) else {
                    return Step::Exit(1);
                };
                Step::Continue(Some(
                    json!({"jsonrpc":"2.0","method":project_changes::EVENT,"params":state}),
                ))
            }
        }
    }
}
