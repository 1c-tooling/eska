//! One serial worker owns revisions shared by editor inputs and lazy metadata requests.

use std::io::{BufRead, Write};

use eska::{
    analysis_core::InputRequest,
    tool_cli::{Failure, localization::Localizer, rpc_framing},
};
use serde_json::{Value, json};

use crate::{AnalysisSession, RequestCancellation, analysis_settings::Settings};

use super::{
    args::Cli, completion, documents, dto, hover, project_changes, property_edits, protocol,
    request_control, signature_help, stream, watched_files, watcher_registration,
};

mod initialize;
mod requests;

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
    edit_plans: Option<property_edits::Plans>,
    project_inputs: Option<project_changes::Inputs>,
    restart: Option<Value>,
    watcher: Option<watcher_registration::Registration>,
}

enum Step {
    Continue(Option<Value>),
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
        project_inputs: None,
        restart: None,
        watcher: None,
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
            self.project_inputs = None;
            self.restart = None;
            self.watcher = None;
            Ok(Value::Null)
        } else if let Some(state) = &self.restart {
            Err(project_changes::error(state))
        } else {
            self.request(&message.method, &message.params, cancellation)
        };
        Step::Continue(Some(protocol::response(message.id.as_ref(), result)))
    }

    /// Full-sync notifications update the host before emitting an acknowledgement event.
    fn document_notification(&mut self, message: &protocol::Message) -> Step {
        if let Some(state) = &self.restart {
            return Step::Continue(Some(
                json!({"jsonrpc":"2.0","method":"eska/inputRejected","params":{
                "snapshot":state["snapshot"],"operation":message.method,"error":project_changes::error(state)}}),
            ));
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
        ) {
            watched_files::Notification::Inputs(event) => Step::Continue(Some(event)),
            watched_files::Notification::Restart(changes) => {
                let state =
                    json!({"snapshot":dto::snapshot(session,&self.session_id),"changes":changes});
                self.session = None;
                self.settings = None;
                self.edit_plans = None;
                self.documents = documents::Documents::default();
                self.watcher = None;
                self.restart = Some(state.clone());
                Step::Continue(Some(
                    json!({"jsonrpc":"2.0","method":project_changes::EVENT,"params":state}),
                ))
            }
        }
    }
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
