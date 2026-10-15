//! Framed query pulls prove negotiated current input, exact ranges and the existing retirement boundary.

use std::fs;

use serde_json::{Value, json};

use super::{Client, initialize, notification, request, support::query_project as fixture};

/// A query client explicitly opts into the standalone syntax contract while retaining normal BSL negotiation.
fn start(
    project: &fixture::QueryProject,
    lang: &str,
    extra: &[&str],
    enabled: bool,
) -> (Client, Value) {
    let mut client = Client::with_options(&project.root, lang, extra);
    let mut initialize = initialize();
    initialize["params"]["initializationOptions"]["eska"]["querySyntax"] = json!(enabled);
    let response = client.ask(&initialize);
    assert!(response.get("error").is_none(), "{response}");
    client.send(&notification("initialized"));
    (client, response)
}

/// Native query filenames are irrelevant; URI identity comes from the verified physical source path.
fn uri(project: &fixture::QueryProject) -> String {
    url::Url::from_file_path(project.root.join("src").join(project.source.as_path()))
        .unwrap()
        .to_string()
}

/// Full sync opens exactly one declared plain query and receives its acknowledged host state.
fn open(client: &mut Client, uri: &str, version: i32, text: &str) -> Value {
    client.send(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"sdbl","version":version,"text":text}}}));
    client.receive()
}

/// The negotiated query identifier remains syntax-only even in a process with semantic BSL settings.
fn pull(client: &mut Client, uri: &str) -> Value {
    client.ask(&request(
        json!(20),
        "textDocument/diagnostic",
        json!({"textDocument":{"uri":uri},"identifier":"eska-query-syntax"}),
    ))
}

/// Machine diagnostic identity excludes only human prose; byte offsets and UTF-16 coordinates are invariant.
fn machine(response: &Value) -> Value {
    let mut items = response["result"]["items"].clone();
    for item in items.as_array_mut().unwrap() {
        item.as_object_mut().unwrap().remove("message");
    }
    items
}

/// All four artifact types support direct query syntax without loading a neighboring query or claiming context.
#[test]
fn query_text_all_four_profiles_negotiate_current_syntax_and_locale_invariant_ranges() {
    for kind in fixture::TYPES {
        let project = fixture::project(kind);
        assert_ne!(project.source, project.neighbor);
        let before = fixture::snapshot(&project);
        let uri = uri(&project);
        let mut outputs = Vec::new();
        for lang in ["ru-RU", "en-US"] {
            let (mut client, initialized) = start(&project, lang, &[], true);
            let capabilities = &initialized["result"]["capabilities"]["experimental"]["eska"];
            assert_eq!(capabilities["querySyntax"]["negotiated"], true);
            assert_eq!(capabilities["querySyntax"]["scope"], "syntax_only");
            assert_eq!(capabilities["querySyntax"]["metadataComplete"], false);
            assert_eq!(
                capabilities["sourceCoreVersion"],
                eska::tool_cli::PACKAGE_VERSION
            );
            let ack = open(&mut client, &uri, 1, fixture::BAD);
            assert_eq!(ack["method"], "eska/inputChanged");
            assert_eq!(ack["params"]["input"]["kind"], "query");
            assert_eq!(ack["params"]["input"]["state"]["origin"]["kind"], "overlay");
            assert_eq!(
                ack["params"]["resources"]["query_text_bytes"],
                fixture::BAD.len()
            );
            let response = pull(&mut client, &uri);
            assert!(response.get("error").is_none(), "{response}");
            let result = &response["result"];
            assert_eq!(result["eska"]["scope"], "syntax_only");
            assert_eq!(result["eska"]["coverage"]["query_syntax_complete"], true);
            assert_eq!(result["eska"]["coverage"]["metadata_complete"], false);
            assert_eq!(result["eska"]["coverage"]["consumer_complete"], false);
            assert_eq!(
                result["eska"]["source_sha256"],
                ack["params"]["input"]["state"]["source_sha256"]
            );
            for item in result["items"].as_array().unwrap() {
                assert_eq!(item["code"], "QueryParseError");
                let start = usize::try_from(item["data"]["start_byte"].as_u64().unwrap()).unwrap();
                let end = usize::try_from(item["data"]["end_byte"].as_u64().unwrap()).unwrap();
                let coordinates = eska::project_core::source_format::TextCoordinates::new(
                    fixture::BAD,
                    &[start, end],
                )
                .unwrap();
                assert_eq!(
                    item["range"],
                    serde_json::to_value(coordinates.range(start..end).unwrap()).unwrap()
                );
            }
            assert!(!result["items"].as_array().unwrap().is_empty());
            outputs.push(machine(&response));
            let locate = client.ask(&request(
                json!(21),
                "eska/metadata/read",
                json!({"snapshot":result["eska"]["snapshot"],"source":project.source}),
            ));
            assert_eq!(locate["error"]["data"]["kind"], "unknown_source");
            let definition = client.ask(&request(
                json!(22),
                "textDocument/definition",
                json!({"textDocument":{"uri":uri},"position":{"line":1,"character":0}}),
            ));
            assert!(definition.get("error").is_some());
            client.finish();
        }
        assert_eq!(outputs[0], outputs[1]);
        assert_eq!(fixture::snapshot(&project), before);
    }
}

/// A client that did not opt in cannot publish query text, load saved text or advance the host clock.
#[test]
fn query_text_unnegotiated_sync_and_pulls_refuse_without_mutation() {
    let project = fixture::project("configuration");
    let (mut client, initialized) = start(&project, "en-US", &[], false);
    let snapshot = &initialized["result"]["capabilities"]["experimental"]["eska"]["snapshot"];
    let uri = uri(&project);
    let ack = open(&mut client, &uri, 1, fixture::BAD);
    assert_eq!(ack["method"], "eska/inputRejected");
    assert_eq!(
        ack["params"]["error"]["data"]["kind"],
        "query_syntax_not_negotiated"
    );
    assert_eq!(ack["params"]["snapshot"], *snapshot);
    let error = pull(&mut client, &uri);
    assert_eq!(
        error["error"]["data"]["kind"],
        "query_syntax_not_negotiated"
    );
    client.finish();
}

/// Saved events preserve current drafts; stale versions, failed text and close never revive a saved success.
#[test]
fn query_text_current_dirty_versions_saved_watch_and_deleted_close_have_no_fallback() {
    let project = fixture::project("report");
    let (mut client, _) = start(&project, "en-US", &[], true);
    let uri = uri(&project);
    let ack = open(&mut client, &uri, 4, fixture::BAD);
    let current = ack["params"]["snapshot"].clone();
    client.send(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":4},"contentChanges":[{"text":"SELECT 2"}]}}));
    let refused = client.receive();
    assert_eq!(
        refused["params"]["error"]["data"]["kind"],
        "stale_document_version"
    );
    assert_eq!(refused["params"]["snapshot"], current);
    fs::remove_file(project.root.join("src").join(project.source.as_path())).unwrap();
    client.send(&json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":uri,"type":3}]}}));
    let watched = client.receive();
    assert_eq!(watched["method"], "eska/inputsChanged");
    assert_eq!(watched["params"]["inputs"][0]["invalidated"], false);
    assert_eq!(watched["params"]["snapshot"], current);
    assert!(
        !pull(&mut client, &uri)["result"]["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    client.send(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri}}}));
    let closed = client.receive();
    assert_eq!(
        closed["params"]["input"]["state"]["origin"]["kind"],
        "saved"
    );
    assert_eq!(
        closed["params"]["input"]["state"]["input"]["kind"],
        "unavailable"
    );
    assert!(closed["params"]["input"]["state"]["source_sha256"].is_null());
    assert_eq!(closed["params"]["resources"]["query_text_bytes"], 0);
    assert_eq!(
        pull(&mut client, &uri)["error"]["data"]["kind"],
        "query_unavailable"
    );
    client.finish();
}

/// A failed editor quota keeps its document version/authority; a zero deadline cannot load a saved query.
#[test]
fn query_text_quota_and_deadline_refusals_are_explicit_and_do_not_publish_saved_success() {
    let project = fixture::project("processing");
    let uri = uri(&project);
    let (mut client, _) = start(&project, "en-US", &["--max-document-bytes", "8"], true);
    let ack = open(&mut client, &uri, 1, fixture::BAD);
    assert_eq!(
        ack["params"]["input"]["state"]["input"]["kind"],
        "unavailable"
    );
    assert_eq!(ack["params"]["input"]["state"]["origin"]["kind"], "overlay");
    assert_eq!(ack["params"]["resources"]["query_text_bytes"], 0);
    assert_eq!(
        pull(&mut client, &uri)["error"]["data"]["kind"],
        "query_unavailable"
    );
    client.finish();
    let (mut client, _) = start(&project, "en-US", &["--timeout-seconds", "0"], true);
    let error = pull(&mut client, &uri);
    assert_eq!(error["error"]["code"], -32802);
    assert_eq!(error["error"]["data"]["kind"], "analysis_deadline_exceeded");
    assert_eq!(error["error"]["data"]["details"]["revision"], "0");
    client.finish();
}

/// A control change retires the shared host, so a prior query draft cannot be replayed into the old identity.
#[test]
fn query_text_project_control_changes_retire_query_drafts_until_restart() {
    let project = fixture::project("configuration");
    let (mut client, _) = start(&project, "en-US", &[], true);
    let uri = uri(&project);
    open(&mut client, &uri, 1, fixture::BAD);
    let manifest = project.root.join("eska.toml");
    let mut text = fs::read_to_string(&manifest).unwrap();
    text.push_str("\n# control change\n");
    fs::write(manifest, text).unwrap();
    let manifest_uri = url::Url::from_file_path(project.root.join("eska.toml"))
        .unwrap()
        .to_string();
    client.send(&json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":manifest_uri,"type":2}]}}));
    let retired = client.receive();
    assert_eq!(retired["method"], "eska/restartRequired");
    let error = pull(&mut client, &uri);
    assert_eq!(error["error"]["data"]["kind"], "project_restart_required");
    client.finish();
    let (mut restarted, initialized) = start(&project, "en-US", &[], true);
    assert_eq!(
        initialized["result"]["capabilities"]["experimental"]["eska"]["snapshot"]["revision"],
        "0"
    );
    let replay = open(&mut restarted, &uri, 1, fixture::BAD);
    assert_eq!(replay["method"], "eska/inputChanged");
    assert!(
        !pull(&mut restarted, &uri)["result"]["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    restarted.finish();
}

/// The existing input/token deadline cancels standalone native work at its publication boundary.
#[test]
fn query_text_cancelled_pulls_keep_current_editor_bytes_and_allow_id_reuse() {
    let project = fixture::project("configuration");
    let (mut client, _) = start(&project, "en-US", &[], true);
    let uri = uri(&project);
    let large = format!("{}SELECT 1", "// Я😀 current query\r\n".repeat(80_000));
    let opened = open(&mut client, &uri, 1, &large);
    client.send(&request(
        json!(80),
        "textDocument/diagnostic",
        json!({"textDocument":{"uri":uri}}),
    ));
    client.send(&json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":80}}));
    let cancelled = client.receive();
    assert_eq!(cancelled["error"]["code"], -32800, "{cancelled}");
    assert_eq!(
        cancelled["error"]["data"]["details"],
        opened["params"]["snapshot"]
    );
    client.send(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":2},"contentChanges":[{"text":"SELECT 2"}]}}));
    let changed = client.receive();
    assert_eq!(
        changed["params"]["input"]["state"]["origin"]["document_version"],
        2
    );
    let recovered = client.ask(&request(
        json!(80),
        "textDocument/diagnostic",
        json!({"textDocument":{"uri":uri}}),
    ));
    assert_eq!(recovered["result"]["items"], json!([]));
    assert_eq!(
        recovered["result"]["eska"]["source_sha256"],
        changed["params"]["input"]["state"]["source_sha256"]
    );
    client.finish();
}

/// Lazy saved pulls read only the selected query; a watcher clears its hash before the new disk read.
#[test]
fn query_text_saved_watch_loads_current_selected_bytes_without_neighbor_reads() {
    for kind in fixture::TYPES {
        let project = fixture::project(kind);
        fs::write(
            project.root.join("src").join(project.source.as_path()),
            fixture::BAD,
        )
        .unwrap();
        fs::remove_file(project.root.join("src").join(project.neighbor.as_path())).unwrap();
        let (mut client, _) = start(&project, "en-US", &[], true);
        let uri = uri(&project);
        let old = pull(&mut client, &uri);
        assert!(old.get("error").is_none(), "{old}");
        assert!(!old["result"]["items"].as_array().unwrap().is_empty());
        assert_eq!(old["result"]["eska"]["origin"]["kind"], "saved");
        assert_eq!(
            old["result"]["eska"]["resources"]["query_text_bytes"],
            fixture::BAD.len()
        );
        fs::write(
            project.root.join("src").join(project.source.as_path()),
            "SELECT 8",
        )
        .unwrap();
        client.send(&json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":uri,"type":2}]}}));
        let changed = client.receive();
        assert_eq!(changed["params"]["inputs"][0]["invalidated"], true);
        assert_eq!(
            changed["params"]["inputs"][0]["input"]["state"]["input"]["kind"],
            "pending"
        );
        assert!(changed["params"]["inputs"][0]["input"]["state"]["source_sha256"].is_null());
        assert_eq!(changed["params"]["resources"]["query_text_bytes"], 0);
        let current = pull(&mut client, &uri);
        assert_eq!(current["result"]["items"], json!([]));
        assert_ne!(
            current["result"]["eska"]["source_sha256"],
            old["result"]["eska"]["source_sha256"]
        );
        client.finish();
    }
}
