//! Bounded reader/worker/writer scheduling with out-of-band cancellation.

use super::{
    envelope::{self, Id, domain, error, response},
    framing,
    server::Server,
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    io,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

const MAX_PENDING: usize = 128;
const MAX_BYTES: usize = 4_194_304;

struct Job {
    value: Value,
    bytes: usize,
    ids: Vec<Id>,
}
#[derive(Default)]
struct Queue {
    jobs: VecDeque<Job>,
    pending: HashMap<Id, Arc<AtomicBool>>,
    bytes: usize,
    overflow: bool,
}
struct Shared {
    queue: Mutex<Queue>,
    worker: thread::Thread,
    stop: AtomicI32,
    initialized: AtomicBool,
    diagnostics: AtomicUsize,
    exit: AtomicI32,
    published: AtomicUsize,
    written: AtomicUsize,
}
struct Output {
    body: Vec<u8>,
    ids: Vec<Id>,
}
type Writer = mpsc::SyncSender<Output>;

/// Run the worker on the calling thread; process exit also releases blocked native pipe reads.
pub(super) fn run() -> u8 {
    let Ok(mut server) = Server::new() else {
        return 1;
    };
    let shared = Arc::new(Shared {
        queue: Mutex::new(Queue::default()),
        worker: thread::current(),
        stop: AtomicI32::new(-1),
        initialized: AtomicBool::new(false),
        diagnostics: AtomicUsize::new(0),
        exit: AtomicI32::new(-1),
        published: AtomicUsize::new(0),
        written: AtomicUsize::new(0),
    });
    // One queued plus one actively written frame is bounded by 128 MiB.
    let (writer, output) = mpsc::sync_channel(1);
    let writer_state = Arc::clone(&shared);
    let writer_thread = thread::Builder::new()
        .name("eska-ide-writer".into())
        .spawn(move || write_loop(&writer_state, &output));
    if writer_thread.is_err() {
        return 1;
    }
    let reader_state = Arc::clone(&shared);
    let reader_writer = writer.clone();
    let reader_thread = thread::Builder::new()
        .name("eska-ide-reader".into())
        .spawn(move || read_loop(&reader_state, &reader_writer));
    if reader_thread.is_err() {
        return 1;
    }
    while shared.stop.load(Ordering::Acquire) < 0 {
        let (job, overflow) = {
            let mut queue = lock(&shared.queue);
            let job = queue.jobs.pop_front();
            if let Some(job) = &job {
                queue.bytes -= job.bytes;
            }
            (job, std::mem::take(&mut queue.overflow))
        };
        let mut events = Vec::new();
        if overflow {
            server.file_overflow(&mut events);
        }
        if let Some(job) = job {
            let result = process(&mut server, &shared, &writer, &job.value, &mut events);
            shared
                .initialized
                .store(server.initialized, Ordering::Release);
            for event in events {
                send(&shared, &writer, &event, Vec::new());
            }
            if let Some(result) = result {
                send(&shared, &writer, &result, job.ids);
            }
            let exit = shared.exit.load(Ordering::Acquire);
            if exit >= 0 {
                let published = shared.published.load(Ordering::Acquire);
                while shared.written.load(Ordering::Acquire) < published
                    && shared.stop.load(Ordering::Acquire) < 0
                {
                    thread::park();
                }
                stop(&shared, exit);
            }
        } else {
            let worked = server.index_step(&mut events);
            for event in events {
                send(&shared, &writer, &event, Vec::new());
            }
            if !worked {
                thread::park();
            }
        }
    }
    u8::try_from(shared.stop.load(Ordering::Acquire)).unwrap_or(1)
}

/// Preserve the first terminal condition across racing reader, worker and writer failures.
fn stop(shared: &Shared, code: i32) {
    let _ = shared
        .stop
        .compare_exchange(-1, code, Ordering::AcqRel, Ordering::Acquire);
    shared.worker.unpark();
}

/// Bound diagnostics per connection so a malformed client cannot fill stderr indefinitely.
fn diagnostic(shared: &Shared, code: &str) {
    if shared.diagnostics.fetch_add(1, Ordering::Relaxed) < 32 {
        eprintln!("{code}");
    }
}

/// Recover queue ownership after poisoning so shutdown can still release its resources.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// One writer owns stdout and releases pending IDs only after the complete frame is sent.
fn write_loop(shared: &Shared, receiver: &mpsc::Receiver<Output>) {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    while let Ok(message) = receiver.recv() {
        if shared.stop.load(Ordering::Acquire) >= 0 {
            break;
        }
        if framing::write_frame(&mut output, &message.body).is_err() {
            stop(shared, 1);
            break;
        }
        let mut queue = lock(&shared.queue);
        for id in message.ids {
            queue.pending.remove(&id);
        }
        drop(queue);
        shared.written.fetch_add(1, Ordering::Release);
        shared.worker.unpark();
    }
}

/// Reader accepts cancellation independently of XML processing and never grows its input queue.
fn read_loop(shared: &Shared, writer: &Writer) {
    let stdin = io::stdin();
    while shared.stop.load(Ordering::Acquire) < 0 {
        let frame = framing::read_frame(&mut stdin.lock());
        let body = match frame {
            Ok(Some(body)) => body,
            Ok(None) => {
                stop(shared, 0);
                break;
            }
            Err(_) => {
                stop(shared, 2);
                break;
            }
        };
        let value = match envelope::parse(&body) {
            Ok(value) => value,
            Err(failure) => {
                send(shared, writer, &response(None, Err(failure)), Vec::new());
                continue;
            }
        };
        enqueue(shared, writer, value, body.len());
    }
}

/// Reserve every request ID atomically before admitting a batch or observing its cancellations.
fn enqueue(shared: &Shared, writer: &Writer, value: Value, bytes: usize) {
    let items = value
        .as_array()
        .map_or_else(|| std::slice::from_ref(&value), Vec::as_slice);
    let ids: Vec<_> = items
        .iter()
        .filter_map(|item| envelope::request(item).ok()?.id)
        .collect();
    let mut queue = lock(&shared.queue);
    let mut seen = std::collections::HashSet::new();
    if ids
        .iter()
        .any(|id| queue.pending.contains_key(id) || !seen.insert(id))
    {
        stop(shared, 2);
        return;
    }
    let oversized = items.len() > 16;
    let full = queue.jobs.len() >= MAX_PENDING
        || queue.pending.len() + ids.len() > MAX_PENDING
        || queue.bytes + bytes > MAX_BYTES;
    if !oversized {
        mark_cancellations(shared, &queue, items);
    }
    if oversized || full {
        if !oversized
            && items
                .iter()
                .any(|item| item["method"] == "workspace/didChangeFiles")
        {
            queue.overflow = true;
            shared.worker.unpark();
        }
        drop(queue);
        let replies: Vec<_> = items.iter().filter_map(|item|match envelope::request(item) {
            Ok(request) => request.id.as_ref().map(|id|response(Some(id),Err(domain("resource_limit",json!({"resource":if oversized {"batch"} else {"queue"},"limit":if oversized {16} else {MAX_PENDING}}))))),
            Err(failure) => Some(response(None,Err(failure))),
        }).collect();
        if !replies.is_empty() {
            let result = if value.is_array() {
                Value::Array(replies)
            } else {
                replies[0].clone()
            };
            send(shared, writer, &result, Vec::new());
        }
        return;
    }
    for id in &ids {
        queue
            .pending
            .insert(id.clone(), Arc::new(AtomicBool::new(false)));
    }
    mark_cancellations(shared, &queue, items);
    queue.bytes += bytes;
    queue.jobs.push_back(Job { value, bytes, ids });
    drop(queue);
    // The retained park token also covers enqueue/stop racing the worker's idle check.
    shared.worker.unpark();
}

/// Cancellation is out-of-band even when the regular request queue is saturated.
fn mark_cancellations(shared: &Shared, queue: &Queue, items: &[Value]) {
    if !shared.initialized.load(Ordering::Acquire) {
        return;
    }
    for item in items {
        if let Ok(request) = envelope::request(item)
            && request.id.is_none()
            && request.method == "$/cancelRequest"
            && let Some(id) = Id::parse(&request.params["id"])
            && let Some(cancelled) = queue.pending.get(&id)
        {
            cancelled.store(true, Ordering::Release);
        }
    }
}

/// Aggregate batch responses while reserving room for remaining bounded error envelopes.
fn process(
    server: &mut Server,
    shared: &Shared,
    writer: &Writer,
    value: &Value,
    events: &mut Vec<Value>,
) -> Option<Value> {
    if let Some(items) = value.as_array() {
        if items.is_empty() {
            return Some(response(None, Err(error(-32600))));
        }
        let mut replies = Vec::new();
        let mut bytes = 2;
        let mut reset = std::collections::HashSet::new();
        for item in items {
            if shared.stop.load(Ordering::Acquire) >= 0 || shared.exit.load(Ordering::Acquire) >= 0
            {
                break;
            }
            let scope = (
                item["params"]["sessionId"]
                    .as_str()
                    .unwrap_or("")
                    .to_owned(),
                item["params"]["projectId"]
                    .as_str()
                    .unwrap_or("")
                    .to_owned(),
            );
            if item.get("id").is_none()
                && item["method"] == "workspace/didChangeFiles"
                && reset.contains(&scope)
            {
                continue;
            }
            if let Some(mut result) = process_one(server, shared, item, events) {
                if item["method"] == "metadata/refresh"
                    && item["params"]["resetFileSequence"] == true
                    && result.get("result").is_some()
                {
                    reset.insert(scope);
                }
                let size =
                    serde_json::to_vec(&result).map_or(framing::MAX_RESPONSE, |body| body.len());
                if bytes + size + 8192 > framing::MAX_RESPONSE {
                    result = too_large(&result);
                }
                bytes += serde_json::to_vec(&result).map_or(0, |body| body.len()) + 1;
                replies.push(result);
            }
            for event in events.drain(..) {
                send(shared, writer, &event, Vec::new());
            }
        }
        (!replies.is_empty()).then_some(Value::Array(replies))
    } else {
        process_one(server, shared, value, events)
    }
}

/// Cancellation checks surround reads; committed lifecycle/index/refresh operations retain results.
fn process_one(
    server: &mut Server,
    shared: &Shared,
    value: &Value,
    events: &mut Vec<Value>,
) -> Option<Value> {
    let request = match envelope::request(value) {
        Ok(request) => request,
        Err(failure) => return Some(response(None, Err(failure))),
    };
    let Some(id) = request.id.as_ref() else {
        match request.method {
            "exit" if request.params.is_object() => {
                shared
                    .exit
                    .store(i32::from(!server.closing), Ordering::Release);
            }
            "workspace/didChangeFiles"
                if server.initialized
                    && !server.closing
                    && server.files_changed(&request.params, events).is_err() =>
            {
                diagnostic(shared, "IDE_INVALID_FILE_NOTIFICATION");
            }
            "$/cancelRequest"
                if server.initialized
                    && !server.closing
                    && Id::parse(&request.params["id"]).is_none() =>
            {
                diagnostic(shared, "IDE_INVALID_CANCEL_NOTIFICATION");
            }
            method
                if server.initialized
                    && !server.closing
                    && (super::metadata::is_method(method)
                        || matches!(
                            method,
                            "initialize"
                                | "workspace/open"
                                | "workspace/close"
                                | "project/info"
                                | "shutdown"
                        )) =>
            {
                diagnostic(shared, "IDE_REQUEST_ID_REQUIRED");
            }
            "exit" => diagnostic(shared, "IDE_INVALID_EXIT_NOTIFICATION"),
            _ => (),
        }
        return None;
    };
    let cancelled = lock(&shared.queue).pending.get(id).cloned();
    let is_cancelled = || {
        cancelled
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Acquire))
    };
    if is_cancelled() {
        return Some(response(Some(id), Err(domain("cancelled", json!({})))));
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        server.execute(request.method, &request.params, events)
    }))
    .unwrap_or_else(|_| Err(error(-32603)));
    let committed = matches!(
        request.method,
        "initialize"
            | "workspace/open"
            | "workspace/close"
            | "shutdown"
            | "metadata/refresh"
            | "metadata/index"
    );
    if request.method == "metadata/refresh"
        && result.is_ok()
        && request.params["resetFileSequence"] == true
    {
        purge_events(shared, &request.params);
    }
    Some(response(
        Some(id),
        if !committed && is_cancelled() {
            Err(domain("cancelled", json!({})))
        } else {
            result
        },
    ))
}

/// Reset removes only queued file notifications for this session/project, including batch entries.
fn purge_events(shared: &Shared, params: &Value) {
    let matches = |item: &Value| {
        item.get("id").is_none()
            && item["method"] == "workspace/didChangeFiles"
            && item["params"]["sessionId"] == params["sessionId"]
            && item["params"]["projectId"] == params["projectId"]
    };
    for job in &mut lock(&shared.queue).jobs {
        if let Some(items) = job.value.as_array_mut() {
            for item in items.iter_mut().filter(|item| matches(item)) {
                *item = json!({"jsonrpc":"2.0","method":"ignored"});
            }
        } else if matches(&job.value) {
            job.value = json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{}});
        }
    }
}

/// Bound serialization before transferring a complete response to the writer.
fn send(shared: &Shared, writer: &Writer, value: &Value, ids: Vec<Id>) {
    if shared.stop.load(Ordering::Acquire) >= 0 {
        return;
    }
    let Ok(mut body) = serde_json::to_vec(value) else {
        stop(shared, 1);
        return;
    };
    if body.len() > framing::MAX_RESPONSE {
        body = serde_json::to_vec(&too_large(value)).unwrap_or_default();
    }
    let mut output = Output { body, ids };
    while shared.stop.load(Ordering::Acquire) < 0 {
        match writer.try_send(output) {
            Ok(()) => {
                shared.published.fetch_add(1, Ordering::Release);
                return;
            }
            Err(mpsc::TrySendError::Full(message)) => {
                output = message;
                thread::sleep(Duration::from_millis(2));
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                stop(shared, 1);
                return;
            }
        }
    }
}

/// Keep the original request ID when a complete result cannot fit the response budget.
fn too_large(value: &Value) -> Value {
    response(
        Id::parse(&value["id"]).as_ref(),
        Err(domain(
            "response_too_large",
            json!({"limitBytes":framing::MAX_RESPONSE}),
        )),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fill the queue without a worker to deterministically exercise admission and cancellation.
    #[test]
    fn saturated_queue_still_accepts_cancellation_and_reports_file_loss() {
        let shared = Shared {
            queue: Mutex::new(Queue::default()),
            worker: thread::current(),
            stop: AtomicI32::new(-1),
            initialized: AtomicBool::new(true),
            diagnostics: AtomicUsize::new(0),
            exit: AtomicI32::new(-1),
            published: AtomicUsize::new(0),
            written: AtomicUsize::new(0),
        };
        let (writer, output) = mpsc::sync_channel(2);
        for id in 0..MAX_PENDING {
            enqueue(
                &shared,
                &writer,
                json!({"jsonrpc":"2.0","id":id,"method":"unknown"}),
                50,
            );
        }
        enqueue(
            &shared,
            &writer,
            json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":0}}),
            50,
        );
        assert!(lock(&shared.queue).pending[&Id::Number(0)].load(Ordering::Acquire));
        enqueue(
            &shared,
            &writer,
            json!({"jsonrpc":"2.0","id":"excess","method":"unknown"}),
            50,
        );
        let reply: Value = serde_json::from_slice(&output.try_recv().unwrap().body).unwrap();
        assert_eq!(reply["error"]["data"]["kind"], "resource_limit");
        enqueue(
            &shared,
            &writer,
            json!({"jsonrpc":"2.0","method":"workspace/didChangeFiles","params":{}}),
            50,
        );
        assert!(lock(&shared.queue).overflow);
        assert_eq!(lock(&shared.queue).pending.len(), MAX_PENDING);
    }

    /// A complete oversized result is replaced, preserving the original correlation ID.
    #[test]
    fn oversized_response_is_not_partially_published() {
        let shared = Shared {
            queue: Mutex::new(Queue::default()),
            worker: thread::current(),
            stop: AtomicI32::new(-1),
            initialized: AtomicBool::new(true),
            diagnostics: AtomicUsize::new(0),
            exit: AtomicI32::new(-1),
            published: AtomicUsize::new(0),
            written: AtomicUsize::new(0),
        };
        let (writer, output) = mpsc::sync_channel(1);
        let mut result = json!({"jsonrpc":"2.0","id":"large"});
        result["result"] = Value::String("x".repeat(framing::MAX_RESPONSE));
        send(&shared, &writer, &result, vec![]);
        let message: Value = serde_json::from_slice(&output.try_recv().unwrap().body).unwrap();
        assert_eq!(message["id"], "large");
        assert_eq!(message["error"]["data"]["kind"], "response_too_large");
        assert!(message.get("result").is_none());
    }

    /// EOF releases a worker waiting for output capacity instead of blocking shutdown forever.
    #[test]
    fn stopped_connection_does_not_wait_for_output_capacity() {
        let shared = Shared {
            queue: Mutex::new(Queue::default()),
            worker: thread::current(),
            stop: AtomicI32::new(0),
            initialized: AtomicBool::new(true),
            diagnostics: AtomicUsize::new(0),
            exit: AtomicI32::new(-1),
            published: AtomicUsize::new(0),
            written: AtomicUsize::new(0),
        };
        let (writer, _output) = mpsc::sync_channel(0);
        send(&shared, &writer, &json!({}), vec![]);
    }
}
