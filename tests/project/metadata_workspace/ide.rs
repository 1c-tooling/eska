//! Independent pipe client: no production DTO or framing code is reused.

use super::{bytes, fixture};
use crate::support::TestDir;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

struct Client {
    child: Child,
    input: Option<ChildStdin>,
    messages: mpsc::Receiver<Value>,
    serial: u64,
    events: Vec<Value>,
}
impl Client {
    /// Read protocol stdout on another thread so each assertion has a finite timeout.
    fn new(locale: &str) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_eska"))
            .args(["--lang", locale, "ide", "--stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = child.stdin.take();
        let output = child.stdout.take().unwrap();
        let (send, messages) = mpsc::channel();
        thread::spawn(move || {
            let mut output = BufReader::new(output);
            loop {
                let mut header = String::new();
                if output.read_line(&mut header).unwrap_or(0) == 0 {
                    break;
                }
                assert!(
                    header.starts_with("Content-Length: ") && header.ends_with("\r\n"),
                    "unexpected stdout: {header:?}"
                );
                let length: usize = header
                    .trim()
                    .strip_prefix("Content-Length: ")
                    .unwrap()
                    .parse()
                    .unwrap();
                let mut separator = [0; 2];
                output.read_exact(&mut separator).unwrap();
                assert_eq!(&separator, b"\r\n");
                let mut body = vec![0; length];
                output.read_exact(&mut body).unwrap();
                if send.send(serde_json::from_slice(&body).unwrap()).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            input,
            messages,
            serial: 0,
            events: Vec::new(),
        }
    }
    /// Send an independent byte-counted UTF-8 frame.
    fn send(&mut self, value: &Value) {
        let body = serde_json::to_vec(value).unwrap();
        self.raw(&body);
    }
    /// Send deliberately malformed JSON inside a valid frame.
    fn raw(&mut self, body: &[u8]) {
        let input = self.input.as_mut().unwrap();
        write!(input, "Content-Length: {}\r\n\r\n", body.len()).unwrap();
        input.write_all(body).unwrap();
        input.flush().unwrap();
    }
    /// Skip server notifications while retaining their exact ordering for assertions.
    fn receive(&mut self) -> Value {
        loop {
            let value = self
                .messages
                .recv_timeout(Duration::from_secs(10))
                .expect("protocol response timeout");
            if value.get("method").is_some() {
                self.events.push(value);
            } else {
                return value;
            }
        }
    }
    /// Allocate an ID unique to this connection and verify correlation.
    fn request(&mut self, method: &str, params: Value) -> Value {
        self.serial += 1;
        let mut request = json!({"jsonrpc":"2.0","id":self.serial,"method":method});
        request["params"] = params;
        self.send(&request);
        let result = self.receive();
        assert_eq!(result["id"], self.serial, "{result}");
        result
    }
    /// Require success without weakening error assertions elsewhere.
    fn ok(&mut self, method: &str, params: Value) -> Value {
        let result = self.request(method, params);
        assert!(result.get("error").is_none(), "{result}");
        result["result"].clone()
    }
    /// Negotiate the public API, using explicit locale to check machine parity.
    fn initialize(&mut self, locale: &str) -> Value {
        self.ok("initialize",json!({"apiVersion":{"major":1,"minor":0},"client":{"name":"integration","version":"1"},"locale":locale}))
    }
    /// Open only a test-owned source without writing a derivative cache.
    fn open(&mut self, path: &std::path::Path) -> Value {
        self.ok("workspace/open",json!({"start":{"value":path,"encoding":"utf-8"},"selection":{"kind":"current"},"diskCache":false}))
    }
    /// Shutdown acknowledgement precedes exit; the reader need not receive EOF to stop.
    fn finish(&mut self) {
        self.ok("shutdown", json!({}));
        self.send(&json!({"jsonrpc":"2.0","method":"exit"}));
        assert_eq!(self.wait_exit(), 0);
    }
    /// Poll only the child exit status with a deadline, killing on assertion unwinding.
    fn wait_exit(&mut self) -> i32 {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status.code().unwrap();
            }
            assert!(Instant::now() < deadline, "process failed to exit");
            thread::sleep(Duration::from_millis(5));
        }
    }
}
impl Drop for Client {
    /// Always reap test processes, including tests failing before shutdown.
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Build generation-bound parameters from returned tokens rather than assuming identities.
fn context(open: &Value, extra: Value) -> Value {
    let mut result = json!({"sessionId":open["sessionId"],"projectId":open["projects"][0]["projectId"],"generation":open["projects"][0]["generation"]});
    result.as_object_mut().unwrap().extend(match extra {
        Value::Object(fields) => fields,
        _ => panic!("extra fields"),
    });
    result
}

/// Every supported project type can be navigated and closed without any source or cache writes.
#[test]
fn process_navigates_four_types_and_preserves_sources() {
    for kind in ["configuration", "extension", "processing", "report"] {
        let directory = TestDir::new();
        fixture(&directory.0, kind);
        assert!(
            Command::new("git")
                .args(["init", "-q"])
                .current_dir(&directory.0)
                .status()
                .unwrap()
                .success()
        );
        let before = bytes(&directory.0);
        let mut client = Client::new("ru");
        client.initialize("ru-RU");
        let open = client.open(&directory.0);
        assert_eq!(open["projects"][0]["type"], kind);
        let root = open["projects"][0]["root"].clone();
        let node = client.ok("metadata/root", context(&open, json!({})));
        assert_eq!(node["node"]["id"], root);
        let children = client.ok(
            "metadata/children",
            context(&open, json!({"node":root,"hideEmptyRootSections":false})),
        );
        assert!(!children["nodes"].as_array().unwrap().is_empty());
        let object = client.ok(
            "metadata/get",
            context(&open, json!({"objectId":root["objectId"]})),
        );
        assert_eq!(object["object"]["objectId"], root["objectId"]);
        let properties = client.ok(
            "metadata/properties",
            context(&open, json!({"objectId":root["objectId"]})),
        );
        assert!(!properties["properties"].as_array().unwrap().is_empty());
        let sources = client.ok("metadata/source", context(&open, json!({"node":root})));
        assert!(
            sources["sources"]
                .as_array()
                .unwrap()
                .iter()
                .all(|source| source["path"]["encoding"] == "utf-8")
        );
        let search = client.ok("metadata/search", context(&open, json!({"text":"ИНН"})));
        assert_eq!(search["progress"]["state"], "not_started");
        client.ok("workspace/close", json!({"sessionId":open["sessionId"]}));
        let reopened = client.open(&directory.0);
        assert_ne!(reopened["sessionId"], open["sessionId"]);
        client.finish();
        assert_eq!(bytes(&directory.0), before);
    }
}

/// Unopened inline records are searchable, revealable and recoverable after missing file events.
#[test]
fn process_search_reveal_refresh_and_file_sequence_recovery() {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    let mut client = Client::new("en");
    client.initialize("en-US");
    let mut open = client.open(&directory.0);
    client.ok("metadata/index", context(&open, json!({"action":"start"})));
    for _ in 0..1000 {
        let progress = client.ok("metadata/index", context(&open, json!({"action":"status"})));
        if progress["progress"]["state"] != "building" {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    let search = client.ok("metadata/search", context(&open, json!({"text":"ИНН"})));
    assert_eq!(search["progress"]["state"], "incomplete");
    let hit = &search["hits"][0];
    assert_eq!(hit["name"], "ИНН");
    let reveal = client.ok(
        "metadata/reveal",
        context(&open, json!({"objectId":hit["objectId"]})),
    );
    assert_eq!(reveal["ancestry"], hit["ancestry"]);
    client.ok(
        "metadata/get",
        context(&open, json!({"objectId":hit["objectId"]})),
    );
    let errors = client.ok("metadata/indexErrors", context(&open, json!({})));
    assert!(!errors["errors"].as_array().unwrap().is_empty());
    client.send(&json!({"jsonrpc":"2.0","method":"workspace/didChangeFiles","params":{"sessionId":open["sessionId"],"projectId":open["projects"][0]["projectId"],"sequence":"2","paths":[{"value":"Catalogs/Контрагенты.xml","encoding":"utf-8"}]}}));
    let rejected = client.request("metadata/root", context(&open, json!({})));
    assert_eq!(rejected["error"]["data"]["kind"], "resync_required");
    assert!(
        client
            .events
            .iter()
            .any(|event| event["method"] == "metadata/changed"
                && event["params"]["requiresRefresh"] == true)
    );
    let root = open["projects"][0]["root"].clone();
    let refresh = client.ok(
        "metadata/refresh",
        context(&open, json!({"node":root,"resetFileSequence":true})),
    );
    open["projects"][0]["generation"] = refresh["generation"].clone();
    client.ok("metadata/root", context(&open, json!({})));
    client.send(&json!({"jsonrpc":"2.0","method":"workspace/didChangeFiles","params":{"sessionId":open["sessionId"],"projectId":open["projects"][0]["projectId"],"sequence":"1","paths":[],"manifestChanged":true}}));
    assert_eq!(
        client.request("metadata/root", context(&open, json!({})))["error"]["data"]["kind"],
        "reopen_required"
    );
    client.finish();
}

/// Invalid framed JSON is recoverable; batch notifications and writes have no side effects.
#[test]
fn process_envelopes_batches_and_lifecycle() {
    let mut client = Client::new("en");
    assert_eq!(
        client.request("metadata/root", json!({}))["error"]["data"]["kind"],
        "invalid_state"
    );
    client.raw(b"{");
    assert_eq!(client.receive()["error"]["code"], -32700);
    client.raw(br#"{"a":1,"a":2}"#);
    assert_eq!(client.receive()["error"]["code"], -32600);
    client.initialize("en-US");
    client.send(&json!([
        {"jsonrpc":"2.0","id":"unknown","method":"metadata/updateProperty","params":{}},
        {"jsonrpc":"2.0","method":"unknown"},
        {"jsonrpc":"2.0","id":"badparams","method":"project/info","params":[]},
        {"jsonrpc":"2.0","id":null,"method":"project/info"}
    ]));
    let replies = client.receive();
    assert_eq!(replies.as_array().unwrap().len(), 3);
    assert_eq!(replies[0]["error"]["code"], -32601);
    assert_eq!(replies[1]["error"]["code"], -32602);
    assert_eq!(replies[2]["error"]["code"], -32600);
    client.send(&json!([{ "jsonrpc":"2.0","method":"unknown" }]));
    assert_eq!(client.request("exit", json!({}))["error"]["code"], -32601);
    client.finish();
}

/// Locales affect help only, including both translations in every schema label.
#[test]
fn process_locales_preserve_identical_dtos() {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    let mut snapshots = Vec::new();
    for (lang, locale) in [("ru", "ru-RU"), ("en", "en-US")] {
        let mut client = Client::new(lang);
        let init = client.initialize(locale);
        let open = client.open(&directory.0);
        let nodes = client.ok(
            "metadata/children",
            context(
                &open,
                json!({"node":open["projects"][0]["root"],"hideEmptyRootSections":false}),
            ),
        );
        snapshots.push(json!([init, open, nodes]));
        client.finish();
    }
    assert_eq!(snapshots[0], snapshots[1]);
}

/// Clean EOF, truncated frames and exit without shutdown have distinct exit codes.
#[test]
fn process_eof_and_bad_framing_exit_codes() {
    for (body, code) in [
        (b"".as_slice(), 0),
        (b"Content-Length: 2\r\n\r\n{", 2),
        (b"Content-Length: 2\n\n{}", 2),
    ] {
        let mut client = Client::new("en");
        client.input.as_mut().unwrap().write_all(body).unwrap();
        client.input.take();
        assert_eq!(client.wait_exit(), code);
    }
    let mut client = Client::new("en");
    client.send(&json!({"jsonrpc":"2.0","method":"exit"}));
    assert_eq!(client.wait_exit(), 1);
}

/// Real external edits advance generation, reject stale requests and recover malformed roots.
#[test]
fn process_external_edits_and_malformed_xml() {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    let mut client = Client::new("en");
    client.initialize("en-US");
    let mut open = client.open(&directory.0);
    let root = open["projects"][0]["root"].clone();
    let path = directory.0.join("src/Configuration.xml");
    let original = std::fs::read(&path).unwrap();
    std::fs::write(&path, b"invalid XML").unwrap();
    client.send(&json!({"jsonrpc":"2.0","method":"workspace/didChangeFiles","params":{"sessionId":open["sessionId"],"projectId":open["projects"][0]["projectId"],"sequence":"1","paths":[{"value":"Configuration.xml","encoding":"utf-8"}]}}));
    assert_eq!(
        client.request("metadata/root", context(&open, json!({})))["error"]["data"]["kind"],
        "resync_required"
    );
    let bad = client.request("metadata/refresh", context(&open, json!({"node":root})));
    assert_eq!(bad["error"]["data"]["kind"], "xml_invalid");
    std::fs::write(&path, original).unwrap();
    let fresh = client.ok("metadata/refresh", context(&open, json!({"node":root})));
    assert_eq!(
        client.request("metadata/root", context(&open, json!({})))["error"]["data"]["kind"],
        "stale_generation"
    );
    open["projects"][0]["generation"] = fresh["generation"].clone();
    client.ok("metadata/root", context(&open, json!({})));
    client.finish();
}

/// Multiple manifest members have independent project tokens and source generations.
#[test]
fn process_workspace_members_are_isolated() {
    let directory = TestDir::new();
    for name in ["first", "second"] {
        fixture(&directory.0.join(name), "processing");
        std::fs::write(
            directory.0.join(name).join("eska.toml"),
            format!("[project]\nname='{name}'\ntype='processing'\n"),
        )
        .unwrap();
    }
    std::fs::write(
        directory.0.join("eska.toml"),
        "[workspace]\nmembers=['first','second']\n",
    )
    .unwrap();
    let mut client = Client::new("en");
    client.initialize("en-US");
    let open = client.open(&directory.0);
    assert_eq!(open["projects"].as_array().unwrap().len(), 2);
    assert_ne!(
        open["projects"][0]["projectId"],
        open["projects"][1]["projectId"]
    );
    assert_eq!(open["projects"][0]["root"], open["projects"][1]["root"]);
    let root = open["projects"][0]["root"].clone();
    client.ok("metadata/refresh", context(&open, json!({"node":root})));
    let info = client.ok("project/info", json!({"sessionId":open["sessionId"]}));
    assert_eq!(info["projects"][0]["generation"], "1");
    assert_eq!(info["projects"][1]["generation"], "0");
    client.finish();
}

/// A cancellation inside an admitted batch reaches a queued request before execution.
#[test]
fn process_cancellation_and_duplicate_pending_ids() {
    let mut client = Client::new("en");
    client.initialize("en-US");
    client.send(&json!([
        {"jsonrpc":"2.0","id":"cancel-me","method":"project/info","params":{"sessionId":"unknown"}},
        {"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":"cancel-me"}}
    ]));
    let result = client.receive();
    assert_eq!(result[0]["error"]["data"]["kind"], "cancelled");
    client.send(&json!([
        {"jsonrpc":"2.0","id":"duplicate","method":"unknown"},
        {"jsonrpc":"2.0","id":"duplicate","method":"unknown"}
    ]));
    assert_eq!(client.wait_exit(), 2);
}

/// The whole oversized batch is rejected before even its valid shutdown can execute.
#[test]
fn process_batch_limit_is_atomic() {
    let mut client = Client::new("en");
    client.initialize("en-US");
    let batch: Vec<_> = (0..17)
        .map(|id| json!({"jsonrpc":"2.0","id":format!("batch-{id}"),"method":"shutdown"}))
        .collect();
    client.send(&json!(batch));
    let replies = client.receive();
    assert_eq!(replies.as_array().unwrap().len(), 17);
    for reply in replies.as_array().unwrap() {
        assert_eq!(reply["error"]["data"]["kind"], "resource_limit");
    }
    client.finish();
}

/// Incorrect optional types and absent manifests are distinct recoverable request failures.
#[test]
fn process_parameter_and_manifest_errors() {
    let directory = TestDir::new();
    let mut client = Client::new("en");
    client.initialize("en-US");
    let result = client.request("workspace/open",json!({"start":{"value":directory.0,"encoding":"utf-8"},"selection":{"kind":"current"},"diskCache":false}));
    assert_eq!(
        result["error"]["data"]["details"]["reason"],
        "manifest_missing"
    );
    assert_eq!(
        client.request("workspace/open", json!({"diskCache":null}))["error"]["code"],
        -32602
    );
    fixture(&directory.0, "configuration");
    let open = client.open(&directory.0);
    for extra in [
        json!({"text":"a","limit":0}),
        json!({"text":"a","limit":501}),
        json!({"text":"a","limit":null}),
        json!({"text":false}),
    ] {
        assert_eq!(
            client.request("metadata/search", context(&open, extra))["error"]["code"],
            -32602
        );
    }
    client.finish();
}

/// Closing the output pipe must terminate without waiting for stdin EOF.
#[test]
fn process_broken_stdout_pipe_exits() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_eska"))
        .args(["ide", "--stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let body = br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"apiVersion":{"major":1,"minor":0},"client":{"name":"test","version":"1"}}}"#;
    let input = child.stdin.as_mut().unwrap();
    write!(input, "Content-Length: {}\r\n\r\n", body.len()).unwrap();
    input.write_all(body).unwrap();
    input.flush().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert_eq!(status.code(), Some(1));
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("broken pipe did not stop worker");
        }
        thread::sleep(Duration::from_millis(5));
    }
}

/// Index cancellation is a separate job control operation and retains partial progress.
#[test]
fn process_index_cancel_resume_and_root_filter() {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    let mut client = Client::new("en");
    client.initialize("en-US");
    let open = client.open(&directory.0);
    client.send(&json!([
        {"jsonrpc":"2.0","id":"start-index","method":"metadata/index","params":context(&open,json!({"action":"start"}))},
        {"jsonrpc":"2.0","id":"cancel-index","method":"metadata/index","params":context(&open,json!({"action":"cancel"}))}
    ]));
    let replies = client.receive();
    assert_eq!(replies[1]["result"]["progress"]["state"], "cancelled");
    client.ok("metadata/index", context(&open, json!({"action":"resume"})));
    let root = open["projects"][0]["root"].clone();
    let all = client.ok(
        "metadata/children",
        context(&open, json!({"node":root,"hideEmptyRootSections":false})),
    );
    let filtered = client.ok("metadata/children", context(&open, json!({"node":root})));
    assert!(all["nodes"].as_array().unwrap().len() > filtered["nodes"].as_array().unwrap().len());
    assert_eq!(all["generation"], filtered["generation"]);
    client.finish();
}

/// Pipelined shutdown/exit flushes accepted responses even when the input remains open.
#[test]
fn process_pipelined_shutdown_flushes_response() {
    let mut client = Client::new("en");
    client.initialize("en-US");
    client.send(&json!([
        {"jsonrpc":"2.0","id":"shutdown","method":"shutdown"},
        {"jsonrpc":"2.0","method":"exit"}
    ]));
    assert_eq!(client.receive()[0]["id"], "shutdown");
    assert_eq!(client.wait_exit(), 0);
}

/// Cached sessions may create only disposable cache files and preserve original bytes exactly.
#[test]
fn process_disk_cache_is_the_only_allowed_write() {
    let directory = TestDir::new();
    fixture(&directory.0, "processing");
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(&directory.0)
            .status()
            .unwrap()
            .success()
    );
    let before = bytes(&directory.0);
    let mut client = Client::new("en");
    client.initialize("en-US");
    let open = client.ok(
        "workspace/open",
        json!({"start":{"value":directory.0,"encoding":"utf-8"},"selection":{"kind":"current"}}),
    );
    client.ok(
        "metadata/properties",
        context(
            &open,
            json!({"objectId":open["projects"][0]["root"]["objectId"]}),
        ),
    );
    client.finish();
    let after = bytes(&directory.0);
    for (path, content) in &before {
        assert_eq!(after.get(path), Some(content));
    }
    let added: Vec<_> = after
        .keys()
        .filter(|path| !before.contains_key(*path))
        .collect();
    assert!(!added.is_empty());
    assert!(
        added
            .iter()
            .all(|path| path.starts_with(".eska/cache/metadata")),
        "{added:?}"
    );
}
