//! Actual stdio sessions verify negotiation, lazy state and read-only metadata in both locales.

mod adapter_issues;
#[cfg(feature = "check-cli")]
mod checker_metadata_queries;
mod completion_queries;
mod definition_chart_queries;
mod definition_member_queries;
mod definition_metadata_queries;
mod definition_namespace_queries;
mod definition_process_queries;
mod definition_queries;
mod definition_register_queries;
mod definition_union_queries;
mod diagnostic_queries;
mod document_symbol_queries;
mod document_sync;
mod event_subscription_queries;
mod hover_queries;
mod kernel_memory_queries;
mod metadata_diagnostic_queries;
mod metadata_locations;
mod project_changes;
mod property_edit_plans;
mod reference_queries;
mod rename_queries;
mod request_cancellation;
mod request_deadline;
mod saved_watcher_registration;
mod scheduled_job_queries;
mod semantic_diagnostic_queries;
mod signature_queries;
mod subsystem_queries;
mod support;
mod watched_files;
mod watcher_registration;

use std::{
    fs,
    io::{BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Output, Stdio},
};

use eska::tool_cli::rpc_framing;
use serde_json::{Value, json};

/// Valid MIME header variants establish a real session instead of terminating its input reader.
#[test]
fn content_type_variants_keep_stdio_sessions_open_in_both_locales() {
    let (_directory, root) = support::project("// Current module.\n", false);
    for lang in ["en-US", "ru-RU"] {
        for content_type in [
            "application/vscode-jsonrpc",
            "application/vscode-jsonrpc;charset=utf8",
            "APPLICATION/VSCODE-JSONRPC ; CHARSET = \"UTF-8\"",
        ] {
            let mut client = Client::with_options(&root, lang, &[]);
            let body = serde_json::to_vec(&initialize()).unwrap();
            write!(
                client.input,
                "Content-Length: {}\r\nContent-Type: {content_type}\r\n\r\n",
                body.len()
            )
            .unwrap();
            client.input.write_all(&body).unwrap();
            client.input.flush().unwrap();
            let response = client.receive();
            assert_eq!(response["id"], 1);
            assert!(response.get("result").is_some(), "{response}");
            client.send(&notification("initialized"));
            client.finish();
        }
    }
}

/// Requests use standard framed JSON-RPC, including arbitrary Unicode string correlation IDs.
fn request(id: Value, method: &str, params: Value) -> Value {
    let mut request = json!({"jsonrpc":"2.0", "method":method});
    request["id"] = id;
    request["params"] = params;
    request
}

/// The explicit ESKA version is negotiated within standard initialization options.
fn initialize() -> Value {
    request(
        json!(1),
        "initialize",
        json!({"capabilities":{},
        "initializationOptions":{"eska":{"protocolVersion":"0.1"}}}),
    )
}

/// The client replaces this fixture placeholder with the negotiated connection identity.
fn snapshot(revision: &str) -> Value {
    json!({"session_id":"session-1", "revision":revision})
}

/// Only connection identities are normalized; locales must preserve every other protocol field.
fn replace_session_id(value: &mut Value, from: &str, to: &str) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                if key == "session_id" && value.as_str() == Some(from) {
                    *value = json!(to);
                } else {
                    replace_session_id(value, from, to);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                replace_session_id(value, from, to);
            }
        }
        _ => (),
    }
}

/// No PATH entry is needed; each acknowledged request copies the negotiated opaque identity.
fn session(root: &Path, lang: &str, extra: &[&str], requests: &[Value]) -> (Output, Vec<Value>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_eska-analyzer"))
        .current_dir(root)
        .env("PATH", "")
        .args(["--lang", lang])
        .args(extra)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut responses = Vec::new();
    let mut wire = Vec::new();
    let mut session_id: Option<String> = None;
    for value in requests {
        let mut value = value.clone();
        if let Some(id) = &session_id {
            replace_session_id(&mut value, "session-1", id);
        }
        rpc_framing::write_frame(&mut input, &serde_json::to_vec(&value).unwrap()).unwrap();
        if value.get("id").is_some() {
            let body = rpc_framing::read_frame(&mut reader).unwrap().unwrap();
            rpc_framing::write_frame(&mut wire, &body).unwrap();
            let mut response: Value = serde_json::from_slice(&body).unwrap();
            if let Some(id) = response
                .pointer("/result/capabilities/experimental/eska/snapshot/session_id")
                .and_then(Value::as_str)
            {
                session_id = Some(id.to_owned());
            }
            if let Some(id) = &session_id {
                replace_session_id(&mut response, id, "session-1");
            }
            responses.push(response);
        }
    }
    input.flush().unwrap();
    drop(input);
    let mut output = child.wait_with_output().unwrap();
    output.stdout = wire;
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    (output, responses)
}

/// Standard notifications never have an ID or receive a response.
fn notification(method: &str) -> Value {
    json!({"jsonrpc":"2.0", "method":method, "params":{}})
}

/// Only shutdown followed by exit is a successful lifecycle termination.
fn ending() -> [Value; 2] {
    [
        request(json!(999), "shutdown", Value::Null),
        notification("exit"),
    ]
}

struct Client {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    session_id: Option<String>,
}

impl Client {
    /// An interactive test may change only its own fixture between acknowledged requests.
    fn new(root: &Path) -> Self {
        Self::with_options(root, "en-US", &[])
    }

    /// Each locale and quota are exercised through the same actual server process.
    fn with_options(root: &Path, lang: &str, extra: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_eska-analyzer"))
            .current_dir(root)
            .env("PATH", "")
            .args(["--lang", lang])
            .args(extra)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        Self {
            input: child.stdin.take().unwrap(),
            output: BufReader::new(child.stdout.take().unwrap()),
            child,
            session_id: None,
        }
    }

    /// Send one complete frame under the caller's exclusive pipe ownership.
    fn send(&mut self, value: &Value) {
        let mut value = value.clone();
        if let Some(id) = &self.session_id {
            replace_session_id(&mut value, "session-1", id);
        }
        rpc_framing::write_frame(&mut self.input, &serde_json::to_vec(&value).unwrap()).unwrap();
    }

    /// Read exactly the response to an acknowledged mutation before the next disk edit.
    fn ask(&mut self, value: &Value) -> Value {
        self.send(value);
        self.receive()
    }

    /// Read a response after pipelined requests/cancellation without sending another message.
    fn receive(&mut self) -> Value {
        let body = rpc_framing::read_frame(&mut self.output).unwrap().unwrap();
        let mut response: Value = serde_json::from_slice(&body).unwrap();
        if let Some(id) = response
            .pointer("/result/capabilities/experimental/eska/snapshot/session_id")
            .and_then(Value::as_str)
        {
            self.session_id = Some(id.to_owned());
        }
        if let Some(id) = &self.session_id {
            replace_session_id(&mut response, id, "session-1");
        }
        response
    }

    /// Release the server through its normal lifecycle before waiting for the child.
    fn finish(&mut self) {
        assert_eq!(self.ask(&ending()[0])["result"], Value::Null);
        self.send(&ending()[1]);
        assert!(self.child.wait().unwrap().success());
    }
}

impl Drop for Client {
    /// A failing assertion must not leave a test-owned analyzer process behind.
    fn drop(&mut self) {
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// Localized setup output is separate from a framed protocol session.
#[test]
fn help_and_version_are_localized_and_do_not_open_a_project() {
    let directory = support::TestDirectory::new();
    for lang in ["en-US", "ru-RU"] {
        let output = Command::new(env!("CARGO_BIN_EXE_eska-analyzer"))
            .current_dir(&directory.0)
            .args(["--lang", lang, "--help"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.contains(if lang == "ru-RU" {
            "Метаданные ESKA"
        } else {
            "Serve ESKA metadata"
        }));
        assert!(help.contains("--max-retained-source-bytes"));
        assert!(help.contains("--timeout-seconds"));
        assert!(help.contains("--profile"));
        assert!(help.contains("semantic"));
        assert!(help.contains(if lang == "ru-RU" {
            "ноль сразу останавливает операцию"
        } else {
            "zero stops the operation immediately"
        }));
        let version = Command::new(env!("CARGO_BIN_EXE_eska-analyzer"))
            .current_dir(&directory.0)
            .args(["--lang", lang, "--version"])
            .output()
            .unwrap();
        assert!(version.status.success());
        assert!(
            String::from_utf8(version.stdout)
                .unwrap()
                .contains(env!("CARGO_PKG_VERSION"))
        );
    }
}

/// An incompatible old protocol is rejected before source discovery; no project exists here.
#[test]
fn negotiation_and_standard_lifecycle_gate_every_request() {
    let directory = support::TestDirectory::new();
    let wrong = request(
        json!(1),
        "initialize",
        json!({"capabilities":{},
        "initializationOptions":{"eska":{"protocolVersion":"1.0"}}}),
    );
    let (output, responses) = session(
        &directory.0,
        "en-US",
        &[],
        &[
            request(json!(0), "eska/metadata/owners", json!({})),
            wrong,
            initialize(),
            notification("exit"),
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(responses[0]["error"]["code"], -32002);
    assert_eq!(
        responses[1]["error"]["data"]["kind"],
        "protocol_version_unsupported"
    );
    assert_eq!(
        responses[2]["error"]["data"]["kind"],
        "project_discovery_failed"
    );
}

/// Opening composition and owner pages does not parse or require another owner's YAML or BSL.
#[test]
fn lazy_owner_pages_and_missing_input_remain_explicit() {
    let (_directory, root) = support::project("Процедура Тест()\nКонецПроцедуры", false);
    fs::remove_file(root.join("src/CommonModules/Tools/object.yaml")).unwrap();
    fs::remove_file(root.join("src/CommonModules/Tools/Ext/Module.bsl")).unwrap();
    let mut requests = vec![
        initialize(),
        notification("initialized"),
        request(
            json!(2),
            "eska/metadata/owners",
            json!({"snapshot":snapshot("0"),"limit":200}),
        ),
        request(
            json!(3),
            "eska/metadata/read",
            json!({"snapshot":snapshot("0"),"source":"CommonModules/Tools/object.yaml"}),
        ),
        request(
            json!(4),
            "eska/metadata/owners",
            json!({"snapshot":snapshot("1"),"limit":200}),
        ),
    ];
    requests.extend(ending());
    let (output, responses) = session(&root, "en-US", &[], &requests);
    assert!(output.status.success());
    assert!(
        responses[1]["result"]["owners"]
            .as_array()
            .unwrap()
            .iter()
            .all(|owner| owner["state"]["input"]["kind"] == "pending")
    );
    assert_eq!(responses[2]["result"]["snapshot"], snapshot("1"));
    assert_eq!(
        responses[2]["result"]["metadata"]["state"]["input"]["kind"],
        "unavailable"
    );
    assert_eq!(responses[2]["result"]["metadata"]["documents"], json!([]));
    assert!(!root.join(".eska").exists());
}

/// Current typed properties and YAML coordinates are identical across process locales.
#[test]
fn saved_properties_stale_revisions_and_unknown_sources_are_read_only() {
    let (_directory, root) = support::project(
        "\u{feff}// Тест\r\nПроцедура Тест()\r\nКонецПроцедуры\r\n",
        false,
    );
    let path = root.join("src/CommonModules/Tools/object.yaml");
    let bytes = fs::read(&path).unwrap();
    let mut requests = vec![
        initialize(),
        notification("initialized"),
        request(
            json!("чтение"),
            "eska/metadata/read",
            json!({"snapshot":snapshot("0"),"source":"CommonModules/Tools/object.yaml"}),
        ),
        request(
            json!(3),
            "eska/metadata/owners",
            json!({"snapshot":snapshot("0"),"limit":200}),
        ),
        request(
            json!(4),
            "eska/metadata/read",
            json!({"snapshot":snapshot("1"),"source":"../secret.yaml"}),
        ),
        request(
            json!(5),
            "eska/metadata/owners",
            json!({"snapshot":snapshot("1"),"limit":0}),
        ),
        request(
            json!(6),
            "eska/metadata/owners",
            json!({"snapshot":snapshot("1"),"limit":200}),
        ),
    ];
    requests.extend(ending());
    let mut reports = Vec::new();
    for lang in ["en-US", "ru-RU"] {
        let (output, responses) = session(&root, lang, &[], &requests);
        assert!(output.status.success());
        assert_eq!(responses[1]["id"], "чтение");
        let metadata = &responses[1]["result"]["metadata"];
        assert_eq!(metadata["state"]["input"]["kind"], "ready");
        assert_eq!(metadata["location_encoding"], "utf-8-bytes");
        let object = &metadata["documents"][0]["objects"][0];
        assert_eq!(object["name"], "Tools");
        let properties = object["properties"].as_array().unwrap();
        assert_eq!(properties.len(), 2);
        assert!(
            properties
                .iter()
                .any(|property| property["property"]["key"]["name"] == "Server"
                    && property["property"]["value"]["Text"] == "true")
        );
        assert_eq!(responses[2]["error"]["code"], -32801);
        assert_eq!(responses[2]["error"]["data"]["details"], snapshot("1"));
        assert_eq!(responses[3]["error"]["code"], -32602);
        assert_eq!(responses[4]["error"]["data"]["kind"], "page_limit_invalid");
        assert_eq!(responses[5]["result"]["snapshot"], snapshot("1"));
        reports.push(responses);
    }
    assert_eq!(reports[0], reports[1]);
    assert_eq!(fs::read(path).unwrap(), bytes);
}

/// Quota refusal replaces current input with an unavailable result and a new revision.
#[test]
fn source_quota_is_shared_by_the_server_and_core_session() {
    let (_directory, root) = support::project("", false);
    let mut requests = vec![
        initialize(),
        notification("initialized"),
        request(
            json!(2),
            "eska/metadata/read",
            json!({"snapshot":snapshot("0"),"source":"CommonModules/Tools/object.yaml"}),
        ),
    ];
    requests.extend(ending());
    let (output, responses) = session(
        &root,
        "en-US",
        &["--max-retained-source-bytes", "0"],
        &requests,
    );
    assert!(output.status.success());
    assert_eq!(
        responses[1]["result"]["metadata"]["state"]["input"]["failure"]["code"],
        "analysis_source_budget_exceeded"
    );
    assert_eq!(responses[1]["result"]["resources"]["total_source_bytes"], 0);
}

/// Inline metadata belongs to its actual owner and does not require a separately invented file.
#[test]
fn inline_commands_are_visible_as_logical_metadata_children() {
    let directory = support::TestDirectory::new();
    let root = support::inline_command_project(&directory.0, "");
    let mut requests = vec![
        initialize(),
        notification("initialized"),
        request(
            json!(2),
            "eska/metadata/read",
            json!({"snapshot":snapshot("0"),"source":"Catalogs/Sample/object.yaml"}),
        ),
    ];
    requests.extend(ending());
    let (output, responses) = session(&root, "en-US", &[], &requests);
    assert!(output.status.success());
    let objects = responses[1]["result"]["metadata"]["documents"][0]["objects"]
        .as_array()
        .unwrap();
    assert!(objects.iter().any(|object| object["kind"] == "command"));
}

/// Invalid saved text clears the previous model, and a stale refresh cannot change that state.
#[test]
fn refresh_replaces_failed_input_and_recovery_restores_only_current_bytes() {
    let (_directory, root) = support::project("// Body remains untouched.\r\n", false);
    let path = root.join("src/CommonModules/Tools/object.yaml");
    let bytes = fs::read(&path).unwrap();
    let mut client = Client::new(&root);
    let capabilities = client.ask(&initialize());
    assert_eq!(
        capabilities["result"]["capabilities"]["experimental"]["eska"]["documentSync"],
        true
    );
    client.send(&notification("initialized"));
    let read = request(
        json!(2),
        "eska/metadata/read",
        json!({"snapshot":snapshot("0"),"source":"CommonModules/Tools/object.yaml"}),
    );
    let first = client.ask(&read);
    assert_eq!(first["result"]["metadata"]["state"]["model"], "ready");
    fs::write(&path, "schema_version: broken\n").unwrap();
    let refresh = request(
        json!(3),
        "eska/metadata/refresh",
        json!({"snapshot":snapshot("1"),"source":"CommonModules/Tools/object.yaml"}),
    );
    let invalid = client.ask(&refresh);
    assert_eq!(invalid["result"]["snapshot"], snapshot("2"));
    assert_eq!(
        invalid["result"]["metadata"]["state"]["model"],
        "unavailable"
    );
    assert_eq!(invalid["result"]["metadata"]["documents"], json!([]));
    fs::write(&path, &bytes).unwrap();
    assert_eq!(
        client.ask(&refresh)["error"]["data"]["kind"],
        "stale_revision"
    );
    let recovered = client.ask(&request(
        json!(4),
        "eska/metadata/refresh",
        json!({"snapshot":snapshot("2"),"source":"CommonModules/Tools/object.yaml"}),
    ));
    assert_eq!(recovered["result"]["snapshot"], snapshot("3"));
    assert_eq!(recovered["result"]["metadata"], first["result"]["metadata"]);
    client.finish();
    assert_eq!(fs::read(path).unwrap(), bytes);
}

/// Equal revision counters from two processes cannot authorize requests against the wrong session.
#[test]
fn reopening_rejects_the_previous_process_token_before_loading_metadata() {
    let (_directory, root) = support::project("", false);
    let mut first = Client::new(&root);
    first.ask(&initialize());
    first.send(&notification("initialized"));
    let previous = first.session_id.clone().unwrap();
    first.finish();
    let mut second = Client::new(&root);
    second.ask(&initialize());
    second.send(&notification("initialized"));
    assert_ne!(Some(&previous), second.session_id.as_ref());
    let old = request(
        json!(2),
        "eska/metadata/read",
        json!({
        "snapshot":{"session_id":previous,"revision":"0"},
        "source":"CommonModules/Tools/object.yaml"}),
    );
    assert_eq!(second.ask(&old)["error"]["data"]["kind"], "unknown_session");
    let current = second.ask(&request(
        json!(3),
        "eska/metadata/owners",
        json!({"snapshot":snapshot("0"),"limit":200}),
    ));
    assert_eq!(current["result"]["resources"]["total_source_bytes"], 0);
    assert_eq!(current["result"]["snapshot"], snapshot("0"));
    second.finish();
}
