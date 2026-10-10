//! Client file events invalidate declared saved inputs before the next lazy request in both locales.

use super::*;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use url::Url;

const METHOD: &str = "workspace/didChangeWatchedFiles";
const OWNER: &str = "CommonModules/Tools/object.yaml";
const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";

/// Convert owned fixture paths to local URIs without depending on their character repertoire.
fn uri(root: &Path, path: &str) -> String {
    Url::from_file_path(root.join("src").join(path))
        .unwrap()
        .to_string()
}

/// One bounded standard notification receives one independent ESKA acknowledgement.
fn watched(changes: Value) -> Value {
    let mut event = json!({"jsonrpc":"2.0","method":METHOD,"params":{}});
    event["params"]["changes"] = changes;
    event
}

/// No static or dynamic LSP watcher registration is advertised by this experimental transport.
fn start(root: &Path, lang: &str) -> Client {
    let mut client = Client::with_options(root, lang, &[]);
    let result = client.ask(&initialize());
    let capability = &result["result"]["capabilities"]["experimental"]["eska"]["watchedFiles"];
    assert_eq!(capability["method"], METHOD);
    assert_eq!(capability["scope"], "declared-saved-inputs");
    assert_eq!(capability["maxChanges"], 200);
    assert_eq!(capability["lazyRead"], true);
    assert_eq!(capability["dynamicRegistration"], false);
    assert_eq!(capability["compositionUpdates"], false);
    client.send(&notification("initialized"));
    client
}

/// Explicit owner reads publish current disk bytes only when the input has become pending.
fn read(client: &mut Client, revision: &str) -> Value {
    client.ask(&request(
        json!("read"),
        "eska/metadata/read",
        json!({"source":OWNER,"snapshot":snapshot(revision)}),
    ))
}

/// Syntax pulls share machine data between locales; localized messages and opaque report IDs differ.
fn diagnostic(client: &mut Client, root: &Path) -> Value {
    let mut result = client.ask(&request(
        json!("syntax"),
        "textDocument/diagnostic",
        json!({"textDocument":{"uri":uri(root,MODULE)}}),
    ));
    if let Some(fields) = result.get_mut("result").and_then(Value::as_object_mut) {
        fields.remove("resultId");
        if let Some(items) = fields.get_mut("items").and_then(Value::as_array_mut) {
            for item in items {
                item.as_object_mut().unwrap().remove("message");
            }
        }
    }
    result
}

/// Fingerprints include every saved UTF-8 byte, including BOM and CRLF.
fn hash(text: &str) -> String {
    let mut result = String::with_capacity(64);
    for byte in Sha256::digest(text.as_bytes()) {
        write!(result, "{byte:02x}").unwrap();
    }
    result
}

/// Locate one declared input in the sorted batch acknowledgement without relying on array positions.
fn input<'a>(event: &'a Value, path: &str) -> &'a Value {
    event["params"]["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|input| input["source"] == path)
        .unwrap()
}

/// A rejected batch has no partial side effects and retains its ordered snapshot.
fn rejected(event: &Value, kind: &str, revision: &str) {
    assert_eq!(event["method"], "eska/inputRejected");
    assert_eq!(event["params"]["operation"], METHOD);
    assert_eq!(event["params"]["error"]["data"]["kind"], kind);
    assert_eq!(event["params"]["snapshot"], snapshot(revision));
}

/// Changed/deleted/recreated saved inputs never serve their previous model or syntax under the new clock.
#[test]
fn watched_saved_yaml_and_bsl_refresh_lazily_and_recover_in_both_locales() {
    let body = "\u{feff}// Я😀 saved\r\nProcedure Probe()\r\nEndProcedure\r\n";
    let (_directory, root) = support::project(body, false);
    let file = root.join("src").join(OWNER);
    let saved = fs::read_to_string(&file).unwrap();
    let next = format!("# Я😀 new saved ranges\r\n{saved}");
    let broken = "\u{feff}// Я😀 broken\r\nProcedure Probe(\r\n";
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = start(&root, lang);
        read(&mut client, "0");
        let first = diagnostic(&mut client, &root);
        assert_eq!(first["result"]["items"], json!([]));
        fs::write(&file, &next).unwrap();
        fs::write(root.join("src").join(MODULE), broken).unwrap();
        let events = json!([{"uri":uri(&root,MODULE),"type":2},{"uri":uri(&root,OWNER),"type":2},{"uri":uri(&root,MODULE),"type":3}]);
        let changed = client.ask(&watched(events.clone()));
        assert_eq!(changed["method"], "eska/inputsChanged");
        assert_eq!(changed["params"]["snapshot"], snapshot("4"));
        assert_eq!(changed["params"]["inputs"].as_array().unwrap().len(), 2);
        assert_eq!(changed["params"]["resources"]["total_source_bytes"], 0);
        for path in [OWNER, MODULE] {
            let input = input(&changed, path);
            assert_eq!(input["invalidated"], true);
            assert_eq!(input["input"]["state"]["input"]["kind"], "pending");
            assert_eq!(input["input"]["state"]["source_sha256"], Value::Null);
        }
        let stale = read(&mut client, "2");
        assert_eq!(stale["error"]["data"]["kind"], "stale_revision");
        let pending = client.ask(&watched(events));
        assert_eq!(pending["params"]["snapshot"], snapshot("4"));
        assert!(
            pending["params"]["inputs"]
                .as_array()
                .unwrap()
                .iter()
                .all(|input| input["invalidated"] == false)
        );
        let refreshed = read(&mut client, "4");
        assert_eq!(
            refreshed["result"]["metadata"]["state"]["source_sha256"],
            hash(&next)
        );
        let syntax = diagnostic(&mut client, &root);
        assert_eq!(syntax["result"]["eska"]["source_sha256"], hash(broken));
        assert!(!syntax["result"]["items"].as_array().unwrap().is_empty());
        fs::remove_file(&file).unwrap();
        fs::remove_file(root.join("src").join(MODULE)).unwrap();
        let deleted = client.ask(&watched(
            json!([{"uri":uri(&root,OWNER),"type":3},{"uri":uri(&root,MODULE),"type":3}]),
        ));
        assert_eq!(deleted["params"]["snapshot"], snapshot("8"));
        let missing_yaml = read(&mut client, "8");
        assert_eq!(
            missing_yaml["result"]["metadata"]["state"]["input"]["failure"]["code"],
            "source_file_missing"
        );
        let missing_bsl = diagnostic(&mut client, &root);
        assert_eq!(missing_bsl["error"]["data"]["kind"], "module_unavailable");
        fs::write(&file, &saved).unwrap();
        fs::write(root.join("src").join(MODULE), body).unwrap();
        let created = client.ask(&watched(
            json!([{"uri":uri(&root,OWNER),"type":1},{"uri":uri(&root,MODULE),"type":1}]),
        ));
        assert_eq!(created["params"]["snapshot"], snapshot("12"));
        let restored = read(&mut client, "12");
        assert_eq!(
            restored["result"]["metadata"]["state"]["source_sha256"],
            hash(&saved)
        );
        let clean = diagnostic(&mut client, &root);
        assert_eq!(clean["result"]["items"], json!([]));
        assert_eq!(clean["result"]["eska"]["source_sha256"], hash(body));
        let trace = vec![
            first,
            changed,
            stale,
            pending,
            refreshed,
            syntax,
            deleted,
            missing_yaml,
            missing_bsl,
            created,
            restored,
            clean,
        ];
        if let Some(previous) = &previous {
            assert_eq!(&trace, previous);
        }
        previous = Some(trace);
        client.finish();
    }
    assert_eq!(fs::read_to_string(&file).unwrap(), saved);
    assert_eq!(
        fs::read_to_string(root.join("src").join(MODULE)).unwrap(),
        body
    );
}

/// File notifications do not advance an editor document or replace its bytes even after disk deletion.
#[test]
fn watched_yaml_and_bsl_keep_editor_authority_until_close_in_both_locales() {
    let body = "// saved\n";
    let (_directory, root) = support::project(body, false);
    let file = root.join("src").join(OWNER);
    let saved = fs::read_to_string(&file).unwrap();
    let draft = format!("# Я😀 editor YAML\r\n{saved}");
    let bsl = "\u{feff}// Я😀 editor BSL\r\n";
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = start(&root, lang);
        let mut trace = Vec::new();
        for (path, text, version) in [(OWNER, draft.as_str(), 7), (MODULE, bsl, 8)] {
            trace.push(client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri(&root,path),"languageId":"eska-source","version":version,"text":text}}})));
        }
        fs::remove_file(&file).unwrap();
        fs::remove_file(root.join("src").join(MODULE)).unwrap();
        let event = client.ask(&watched(
            json!([{"uri":uri(&root,OWNER),"type":3},{"uri":uri(&root,MODULE),"type":3}]),
        ));
        assert_eq!(event["params"]["snapshot"], snapshot("2"));
        assert_eq!(
            event["params"]["resources"]["total_source_bytes"],
            draft.len() + bsl.len()
        );
        for (path, text, version) in [(OWNER, draft.as_str(), 7), (MODULE, bsl, 8)] {
            let input = input(&event, path);
            assert_eq!(input["invalidated"], false);
            assert_eq!(
                input["input"]["state"]["origin"],
                json!({"kind":"overlay","document_version":version})
            );
            assert_eq!(input["input"]["state"]["source_sha256"], hash(text));
        }
        trace.push(event);
        let yaml = read(&mut client, "2");
        assert_eq!(
            yaml["result"]["metadata"]["state"]["source_sha256"],
            hash(&draft)
        );
        trace.push(yaml);
        let syntax = diagnostic(&mut client, &root);
        assert_eq!(syntax["result"]["eska"]["source_sha256"], hash(bsl));
        trace.push(syntax);
        for path in [OWNER, MODULE] {
            let event = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri(&root,path)}}}));
            assert_eq!(
                event["params"]["input"]["state"]["input"]["kind"],
                "unavailable"
            );
            trace.push(event);
        }
        if let Some(previous) = &previous {
            assert_eq!(&trace, previous);
        }
        previous = Some(trace);
        client.finish();
        fs::write(&file, &saved).unwrap();
        fs::write(root.join("src").join(MODULE), body).unwrap();
    }
}

/// Invalid types, malformed URIs, undeclared composition and oversized batches reject every input first.
#[test]
fn watched_file_batches_preflight_before_mutation_in_both_locales() {
    let (_directory, root) = support::project("// saved\n", false);
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = start(&root, lang);
        let current = read(&mut client, "0");
        let valid = json!({"uri":uri(&root,OWNER),"type":2});
        let invalid = [
            (
                json!([valid,{"uri":uri(&root,OWNER),"type":4}]),
                "file_event_type_invalid",
            ),
            (
                json!([valid,{"uri":"https://example.invalid/file.yaml","type":2}]),
                "document_uri_invalid",
            ),
            (
                json!([valid,{"uri":uri(&root,"Unknown/object.yaml"),"type":1}]),
                "unknown_source",
            ),
            (
                json!([valid,{"uri":format!("{}#fragment",uri(&root,OWNER)),"type":2}]),
                "document_uri_invalid",
            ),
            (
                json!([valid,{"uri":uri(&root,OWNER),"type":2,"extra":true}]),
                "rpc_invalid_params",
            ),
            (json!(vec![valid; 201]), "file_event_limit_exceeded"),
        ];
        let mut trace = Vec::new();
        for (changes, kind) in invalid {
            let event = client.ask(&watched(changes));
            rejected(&event, kind, "1");
            assert_eq!(read(&mut client, "1"), current);
            trace.push(event);
        }
        let empty = client.ask(&watched(json!([])));
        assert_eq!(empty["params"]["snapshot"], snapshot("1"));
        assert_eq!(empty["params"]["inputs"], json!([]));
        trace.push(empty);
        if let Some(previous) = &previous {
            assert_eq!(&trace, previous);
        }
        previous = Some(trace);
        client.finish();
    }
}

/// A protected body keeps its explicit binary state and rejects the whole mixed batch.
#[test]
fn watched_protected_module_does_not_invalidate_a_ready_owner_in_both_locales() {
    let (_directory, root) = support::project("opaque body", true);
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = start(&root, lang);
        let current = read(&mut client, "0");
        let event = client.ask(&watched(json!([
            {"uri":uri(&root,OWNER),"type":2},
            {"uri":uri(&root,"CommonModules/Tools/Ext/Module.bin"),"type":2}
        ])));
        rejected(&event, "binary_module", "1");
        assert_eq!(read(&mut client, "1"), current);
        if let Some(previous) = &previous {
            assert_eq!(&event, previous);
        }
        previous = Some(event);
        client.finish();
    }
}
