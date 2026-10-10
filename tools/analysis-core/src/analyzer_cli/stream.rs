//! A bounded input thread routes cancellation while the sole worker owns all project mutations.

#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, btree_map::Entry},
    io::{self, BufRead},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, SyncSender},
    },
    thread::JoinHandle,
};

use eska::tool_cli::rpc_framing;
use serde::Deserialize;
use serde_json::Value;

use crate::RequestCancellation;

use super::{
    documents,
    language::{self, Method},
    protocol::{self, Id, Message},
};

pub const MAX_QUEUED_MESSAGES: usize = 4;

pub enum Envelope {
    Message(Message, Option<TrackedRequest>),
    Error(Value),
}

pub struct Incoming {
    receiver: Receiver<Envelope>,
    reader: JoinHandle<()>,
}

impl Incoming {
    /// Stdin belongs to a process input thread; source/session/database handles never enter it.
    pub fn start(mut input: impl BufRead + Send + 'static) -> io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(MAX_QUEUED_MESSAGES);
        let requests = Arc::new(Requests::default());
        let reader = std::thread::Builder::new()
            .name("eska-analyzer-input".into())
            .spawn(move || forward(&mut input, &sender, &requests))?;
        Ok(Self { receiver, reader })
    }

    /// Ordered source notifications cannot overtake earlier requests or create another host.
    pub fn receive(&self) -> Result<Envelope, mpsc::RecvError> {
        self.receiver.recv()
    }

    /// The input thread stops after enqueueing exit, so normal shutdown can join without stdin EOF.
    pub fn join_after_exit(self) -> bool {
        self.reader.join().is_ok()
    }
}

/// Cancellation bypasses the worker queue; backpressure bounds all ordinary parsed messages.
fn forward(input: &mut impl BufRead, sender: &SyncSender<Envelope>, requests: &Arc<Requests>) {
    while let Ok(Some(frame)) = rpc_framing::read_frame_with_limit(input, documents::MAX_REQUEST) {
        let envelope = match protocol::parse(&frame) {
            Err(error) => Envelope::Error(protocol::response(None, Err(error))),
            Ok(message) => {
                if message.id.is_none() && message.method == "$/cancelRequest" {
                    if let Ok(params) = protocol::params::<Cancel>(&message.params) {
                        requests.cancel(&params.id);
                    }
                    continue;
                }
                requests.track(message)
            }
        };
        let exit = matches!(&envelope,Envelope::Message(message,_) if message.id.is_none() && message.method=="exit");
        if sender.send(envelope).is_err() || exit {
            break;
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Cancel {
    id: Id,
}

#[derive(Default)]
struct Requests(Mutex<BTreeMap<Id, RequestCancellation>>);

impl Requests {
    /// Only current language requests own controls; unknown/completed IDs cannot poison reuse.
    fn track(self: &Arc<Self>, message: Message) -> Envelope {
        let Some(id) = &message.id else {
            return Envelope::Message(message, None);
        };
        let Some(method) = Method::parse(&message.method) else {
            return Envelope::Message(message, None);
        };
        let mut active = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Entry::Vacant(slot) = active.entry(id.clone()) else {
            return Envelope::Error(protocol::response(
                Some(id),
                Err(protocol::error_value(
                    -32600,
                    "request_id_in_use",
                    Value::Null,
                )),
            ));
        };
        let control = RequestCancellation::default();
        slot.insert(control.clone());
        let request = TrackedRequest {
            id: id.clone(),
            requests: Arc::clone(self),
            control,
            method,
        };
        drop(active);
        Envelope::Message(message, Some(request))
    }

    /// Do not hold the registry mutex while the kernel's cancellation registration is locked.
    fn cancel(&self, id: &Id) {
        let control = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(id)
            .cloned();
        if let Some(control) = control {
            control.cancel();
        }
    }
}

pub struct TrackedRequest {
    id: Id,
    requests: Arc<Requests>,
    pub control: RequestCancellation,
    pub method: language::Method,
}

impl Drop for TrackedRequest {
    /// Success, error and cancellation all remove the in-flight ID before it can be reused.
    fn drop(&mut self) {
        self.requests
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.id);
    }
}
