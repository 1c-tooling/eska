//! Input-thread cancellation, ID lifetime and ordering are verified without timing-based sleeps.

use std::{io::Cursor, time::Duration};

use serde_json::json;

use super::*;

/// Build actual protocol frames; notifications have no ID field.
fn message(id: Option<Value>, method: &str, params: &Value) -> Message {
    let mut value = json!({"jsonrpc":"2.0","method":method,"params":params});
    if let Some(id) = id {
        value["id"] = id;
    }
    protocol::parse(&serde_json::to_vec(&value).unwrap()).unwrap()
}

/// Current string/numeric IDs remain distinct; completion removes every cancellation record.
#[test]
fn unknown_completed_and_reused_ids_do_not_poison_other_requests() {
    let requests = Arc::new(Requests::default());
    requests.cancel(&Id::Number(7));
    let Envelope::Message(_, Some(first)) = requests.track(message(
        Some(json!(7)),
        "textDocument/diagnostic",
        &Value::Null,
    )) else {
        panic!("missing request");
    };
    assert!(!first.control.is_cancelled());
    let Envelope::Error(duplicate) = requests.track(message(
        Some(json!(7)),
        "textDocument/diagnostic",
        &Value::Null,
    )) else {
        panic!("duplicate accepted");
    };
    assert_eq!(duplicate["id"], 7);
    assert_eq!(duplicate["error"]["data"]["kind"], "request_id_in_use");
    let Envelope::Message(_, Some(other)) = requests.track(message(
        Some(json!("7")),
        "textDocument/diagnostic",
        &Value::Null,
    )) else {
        panic!("missing request");
    };
    requests.cancel(&Id::Number(7));
    assert!(first.control.input.is_cancelled());
    assert!(first.control.query.is_cancelled());
    assert!(!other.control.is_cancelled());
    drop(first);
    requests.cancel(&Id::Number(7));
    let Envelope::Message(_, Some(reused)) = requests.track(message(
        Some(json!(7)),
        "textDocument/diagnostic",
        &Value::Null,
    )) else {
        panic!("reuse rejected");
    };
    assert!(!reused.control.is_cancelled());
    drop(reused);
    drop(other);
    assert!(requests.0.lock().unwrap().is_empty());
}

/// The reader signals a held worker control before the worker can consume subsequent messages.
#[test]
fn input_thread_delivers_cancel_without_worker_dispatch_and_stops_on_exit() {
    let mut wire = Vec::new();
    for value in [
        json!({"jsonrpc":"2.0","id":"проверка","method":"textDocument/diagnostic","params":{}}),
        json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":"проверка"}}),
        json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":false}}),
        json!({"jsonrpc":"2.0","id":3,"method":"eska/metadata/owners","params":{}}),
        json!({"jsonrpc":"2.0","method":"exit"}),
    ] {
        rpc_framing::write_frame(&mut wire, &serde_json::to_vec(&value).unwrap()).unwrap();
    }
    // A suffix is deliberately not read after the exit notification.
    wire.extend_from_slice(b"invalid frame");
    let incoming = Incoming::start(Cursor::new(wire)).unwrap();
    let Envelope::Message(_, Some(held)) = incoming
        .receiver
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
    else {
        panic!("missing control");
    };
    let Envelope::Message(checkpoint, None) = incoming
        .receiver
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
    else {
        panic!("missing checkpoint");
    };
    assert_eq!(checkpoint.method, "eska/metadata/owners");
    assert!(held.control.is_cancelled());
    assert!(held.control.query.is_cancelled());
    let Envelope::Message(exit, None) = incoming
        .receiver
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
    else {
        panic!("missing exit");
    };
    assert_eq!(exit.method, "exit");
    drop(held);
    assert!(incoming.join_after_exit());
}
