//! A selected control-file event retires the host; no old tree, text or edit plan can survive it.

use super::*;
use url::Url;

const OWNER: &str = "CommonModules/Tools/object.yaml";
const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";

/// Watcher inputs are advertised explicitly, including absent conventional settings candidates.
fn start(root: &Path, lang: &str) -> (Client, Value) {
    let mut client = Client::with_options(root, lang, &[]);
    let initialized = client.ask(&initialize());
    let controls =
        initialized["result"]["capabilities"]["experimental"]["eska"]["projectChanges"].clone();
    assert_eq!(controls["method"], "workspace/didChangeWatchedFiles");
    assert_eq!(controls["policy"], "restart");
    assert_eq!(controls["notification"], "eska/restartRequired");
    client.send(&notification("initialized"));
    (client, controls)
}

/// Standard file events carry no correlation ID and use the explicit inventory's local URI.
fn watch(changes: Value) -> Value {
    let mut message =
        json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{}});
    message["params"]["changes"] = changes;
    message
}

/// The selected small owner makes a stale tree observable before the host is retired.
fn read(client: &mut Client, revision: &str) -> Value {
    client.ask(&request(
        json!("read"),
        "eska/metadata/read",
        json!({"source":OWNER,"snapshot":snapshot(revision)}),
    ))
}

/// Retired servers reject all project work before parameter validation, disk I/O or kernel queries.
fn blocked(client: &mut Client, state: &Value) -> Vec<Value> {
    let mut trace = Vec::new();
    for method in [
        "eska/metadata/owners",
        "eska/metadata/read",
        "eska/metadata/refresh",
        "eska/metadata/locate",
        "eska/metadata/preparePropertyEdit",
        "textDocument/diagnostic",
        "textDocument/completion",
        "textDocument/definition",
    ] {
        let response = client.ask(&request(json!("blocked"), method, Value::Null));
        assert_eq!(
            response["error"]["data"]["kind"],
            "project_restart_required"
        );
        assert_eq!(response["error"]["data"]["details"], *state);
        trace.push(response);
    }
    for method in [
        "textDocument/didOpen",
        "textDocument/didChange",
        "textDocument/didSave",
        "textDocument/didClose",
        "workspace/didChangeWatchedFiles",
    ] {
        let response = client.ask(&json!({"jsonrpc":"2.0","method":method,"params":null}));
        assert_eq!(response["method"], "eska/inputRejected");
        assert_eq!(
            response["params"]["error"]["data"]["kind"],
            "project_restart_required"
        );
        assert_eq!(response["params"]["snapshot"], state["snapshot"]);
        trace.push(response);
    }
    trace
}

/// Creation, modification and deletion of every advertised standalone control path require restart.
#[test]
fn each_control_file_event_retires_all_queries_and_editor_inputs_in_both_locales() {
    let (_directory, root) = support::project("// saved\n", false);
    let controls = [
        (
            "source-manifest",
            root.join("src/source.json"),
            "source_manifest",
        ),
        (
            "root-configuration",
            root.join("src/configuration.yaml"),
            "root_configuration",
        ),
        ("project", root.join("eska.toml"), "project_settings"),
        ("toml", root.join("bsl-analyzer.toml"), "analysis_settings"),
        ("json", root.join(".bsl-analyzer.json"), "analysis_settings"),
        (
            "legacy",
            root.join(".bsl-language-server.json"),
            "analysis_settings",
        ),
    ];
    for (_, path, kind) in controls {
        let uri = Url::from_file_path(&path).unwrap().to_string();
        let original = fs::read(&path).ok();
        for change_type in [1, 2, 3] {
            let mut previous = None;
            for lang in ["en-US", "ru-RU"] {
                let (mut client, capability) = start(&root, lang);
                assert_eq!(capability["inputs"].as_array().unwrap().len(), 6);
                assert!(
                    capability["inputs"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|input| input["kind"] == kind && input["uri"] == uri)
                );
                read(&mut client, "0");
                let module_uri = Url::from_file_path(root.join("src").join(MODULE)).unwrap();
                client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":module_uri.as_str(),"languageId":"bsl","version":7,"text":"// Я😀 editor\r\n"}}}));
                fs::write(&path, "unreadable new content for a retired host").unwrap();
                if change_type == 3 {
                    fs::remove_file(&path).unwrap();
                }
                let event = client.ask(&watch(
                    json!([{"uri":uri,"type":change_type},{"uri":uri,"type":change_type}]),
                ));
                assert_eq!(event["method"], "eska/restartRequired");
                let state = &event["params"];
                assert_eq!(state["snapshot"], snapshot("2"));
                assert_eq!(state["changes"], json!([{"kind":kind,"uri":uri}]));
                let mut trace = blocked(&mut client, state);
                trace.insert(0, event);
                if let Some(previous) = &previous {
                    assert_eq!(&trace, previous);
                }
                previous = Some(trace);
                client.finish();
                if let Some(bytes) = &original {
                    fs::write(&path, bytes).unwrap();
                } else if path.exists() {
                    fs::remove_file(&path).unwrap();
                }
            }
        }
    }
}

/// Invalid wire data is rejected first; a valid manifest-plus-new-file batch retires without reading it.
#[test]
fn control_batches_validate_wire_data_and_accept_new_composition_paths_in_both_locales() {
    let (_directory, root) = support::project("// saved\n", false);
    let manifest = Url::from_file_path(root.join("src/source.json")).unwrap();
    let yaml = Url::from_file_path(root.join("src").join(OWNER)).unwrap();
    let missing = Url::from_file_path(root.join("src/NewOwner/object.yaml")).unwrap();
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let (mut client, _) = start(&root, lang);
        let ready = read(&mut client, "0");
        let control = json!({"uri":manifest.as_str(),"type":2});
        let mut trace = Vec::new();
        for (bad, kind) in [
            (
                json!({"uri":yaml.as_str(),"type":4}),
                "file_event_type_invalid",
            ),
            (
                json!({"uri":"https://example.invalid/file.yaml","type":2}),
                "document_uri_invalid",
            ),
            (
                json!({"uri":format!("{}#bad",manifest.as_str()),"type":2}),
                "document_uri_invalid",
            ),
            (
                json!({"uri":Url::from_file_path(root.parent().unwrap().join("outside.yaml")).unwrap().as_str(),"type":2}),
                "document_outside_source",
            ),
        ] {
            let event = client.ask(&watch(json!([control, bad])));
            assert_eq!(event["method"], "eska/inputRejected");
            assert_eq!(event["params"]["error"]["data"]["kind"], kind);
            assert_eq!(event["params"]["snapshot"], snapshot("1"));
            assert_eq!(read(&mut client, "1"), ready);
            trace.push(event);
        }
        let event = client.ask(&watch(
            json!([{"uri":missing.as_str(),"type":1},control,{"uri":yaml.as_str(),"type":2}]),
        ));
        assert_eq!(event["method"], "eska/restartRequired");
        assert_eq!(event["params"]["snapshot"], snapshot("1"));
        assert_eq!(event["params"]["changes"].as_array().unwrap().len(), 1);
        trace.push(event);
        if let Some(previous) = &previous {
            assert_eq!(&trace, previous);
        }
        previous = Some(trace);
        client.finish();
        let (mut reopened, _) = start(&root, lang);
        assert_eq!(read(&mut reopened, "0"), ready);
        reopened.finish();
    }
    assert!(!root.join("src/NewOwner").exists());
}

/// A member session watches the inherited workspace manifest but not unrelated sibling sources.
#[test]
fn selected_workspace_manifest_is_an_explicit_restart_input_in_both_locales() {
    let (_directory, member) = support::project("// saved\n", false);
    let workspace = member.parent().unwrap();
    fs::write(
        workspace.join("eska.toml"),
        "[workspace]\nmembers=['project']\n",
    )
    .unwrap();
    let uri = Url::from_file_path(workspace.join("eska.toml")).unwrap();
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let (mut client, capability) = start(&member, lang);
        assert_eq!(capability["inputs"].as_array().unwrap().len(), 7);
        assert!(
            capability["inputs"]
                .as_array()
                .unwrap()
                .iter()
                .any(|input| input["kind"] == "workspace_settings" && input["uri"] == uri.as_str())
        );
        read(&mut client, "0");
        let event = client.ask(&watch(json!([{"uri":uri.as_str(),"type":2}])));
        assert_eq!(event["method"], "eska/restartRequired");
        assert_eq!(
            event["params"]["changes"],
            json!([{"kind":"workspace_settings","uri":uri.as_str()}])
        );
        let trace = [vec![event.clone()], blocked(&mut client, &event["params"])].concat();
        if let Some(previous) = &previous {
            assert_eq!(&trace, previous);
        }
        previous = Some(trace);
        client.finish();
    }
}
