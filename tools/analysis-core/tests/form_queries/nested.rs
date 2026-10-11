//! One nested dialog must not change its main object, siblings or equal short names in other owners.

use super::*;

const NESTED_OWNER: &str = "Catalogs/Items/object.yaml";
const NESTED_DIALOG: &str = "Catalogs/Items/Forms/Shared/Ext/Form.xml";
const NESTED_MODULE: &str = "Catalogs/Items/Forms/Shared/Ext/Form/Module.bsl";
const NESTED_NATIVE: &str = "Catalogs/Items/Forms/Shared.xml";

/// The metadata response groups independent nested states under their actual enclosing YAML.
fn read(client: &mut Client, snap: &Value) -> Value {
    client.ask(&request(
        json!(31),
        "eska/metadata/read",
        json!({"source":NESTED_OWNER,"snapshot":snap}),
    ))
}

/// Query the same native rule and body as the common-form tests through a full nested path.
fn check(client: &mut Client, source: &Path, snap: &Value) -> Value {
    client.ask(&request(json!(30), "textDocument/diagnostic", json!({"textDocument":{"uri":uri(source,NESTED_MODULE)},"identifier":"eska-semantic","eska":{"snapshot":snap}})))
}

/// The returned leaf's identity remains stable while its resource origin, model and revision change.
fn leaf(state: &Value) -> &Value {
    state["forms"]
        .as_array()
        .unwrap()
        .iter()
        .find(|form| form["native_path"] == NESTED_NATIVE)
        .unwrap()
}

/// Full document sync and saved deletion/recreation preserve current editor authority in both locales.
#[test]
fn nested_forms_have_independent_current_states_and_native_handler_diagnostics() {
    let (directory, root) = support::project("// Shared body.\n", false);
    support::form_project::add_nested_forms(&root, &directory.0.join("native"), false);
    handler_settings(&root);
    let source = root.join("src-with-nested-forms");
    let saved = fs::read(source.join(NESTED_DIALOG)).unwrap();
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &["--profile", "semantic"]);
        let mut options = initialize();
        options["params"]["capabilities"]["workspace"] = json!({"didChangeWatchedFiles":{"dynamicRegistration":true,"relativePatternSupport":true}});
        let init = client.ask(&options);
        assert_eq!(
            init["result"]["capabilities"]["experimental"]["eska"]["nestedFormModels"],
            true
        );
        client.send(&notification("initialized"));
        let registration = client.receive();
        assert_eq!(registration["method"], "client/registerCapability");
        client.ask(&json!({"jsonrpc":"2.0","id":"eska/project-watch/1","result":null}));
        let loaded = read(&mut client, &snapshot("0"));
        let state = &loaded["result"]["metadata"]["state"];
        assert_eq!(state["model"], "ready");
        assert_eq!(state["modeled_descriptors"], 3);
        assert_eq!(leaf(state)["model"], "ready");
        assert_eq!(leaf(state)["dialog"], NESTED_DIALOG);
        assert_eq!(loaded["result"]["snapshot"], snapshot("3"));
        let ready = check(&mut client, &source, &loaded["result"]["snapshot"]);
        assert_eq!(method(&ready), "CurrentHandler");
        let opened = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri(&source,NESTED_DIALOG),"languageId":"xml","version":7,"text":support::form_project::dialog("CurrentHandler")}}}));
        assert_eq!(opened["params"]["input"]["owner"], NESTED_OWNER);
        let current = check(&mut client, &source, &opened["params"]["snapshot"]);
        assert_eq!(method(&current), "SavedHandler");
        let failed = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri(&source,NESTED_DIALOG),"version":8},"contentChanges":[{"text":"<NotForm/>"}]}}));
        let read_failed = read(&mut client, &failed["params"]["snapshot"]);
        let state = &read_failed["result"]["metadata"]["state"];
        assert_eq!(state["model"], "ready");
        assert_eq!(state["modeled_descriptors"], 2);
        assert_eq!(leaf(state)["model"], "unavailable");
        assert_eq!(
            leaf(state)["model_issue"],
            json!({"code":"analysis_metadata_dependency_unavailable","failure":{"code":"analysis_form_dialog_invalid","location":[NESTED_DIALOG]}})
        );
        assert!(
            state["forms"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|form| form["native_path"] != NESTED_NATIVE)
                .all(|form| form["model"] == "ready")
        );
        if let Some(previous) = &previous {
            assert_eq!(state, previous);
        }
        previous = Some(state.clone());
        let watched = client.ask(&json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":uri(&source,NESTED_DIALOG),"type":2}]}}));
        assert_eq!(watched["params"]["inputs"][0]["invalidated"], false);
        assert_eq!(watched["params"]["snapshot"], failed["params"]["snapshot"]);
        let closed = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri(&source,NESTED_DIALOG)}}}));
        assert_eq!(
            method(&check(&mut client, &source, &closed["params"]["snapshot"])),
            "CurrentHandler"
        );
        fs::remove_file(source.join(NESTED_DIALOG)).unwrap();
        let deleted = client.ask(&json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":uri(&source,NESTED_DIALOG),"type":3},{"uri":uri(&source,"Catalogs/Items/Forms/Stray/Ext/Form.xml"),"type":2}]}}));
        assert_eq!(deleted["params"]["ignoredSources"], 1);
        let unavailable = read(&mut client, &deleted["params"]["snapshot"]);
        assert_eq!(
            leaf(&unavailable["result"]["metadata"]["state"])["model_issue"]["failure"]["code"],
            "source_file_missing"
        );
        fs::write(source.join(NESTED_DIALOG), &saved).unwrap();
        let created = client.ask(&json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":uri(&source,NESTED_DIALOG),"type":1}]}}));
        assert_eq!(
            method(&check(&mut client, &source, &created["params"]["snapshot"])),
            "CurrentHandler"
        );
        client.finish();
    }
    assert_eq!(fs::read(source.join(NESTED_DIALOG)).unwrap(), saved);
}

/// Checker and IDE use equal saved nested descriptors, dialogs, fingerprints and native findings.
#[test]
#[cfg(feature = "check-cli")]
fn nested_checker_and_ide_have_equal_saved_context() {
    let (directory, root) = support::project("// Shared body.\n", false);
    support::form_project::add_nested_forms(&root, &directory.0.join("native"), false);
    handler_settings(&root);
    let source = root.join("src-with-nested-forms");
    for lang in ["en-US", "ru-RU"] {
        let output = Command::new(env!("CARGO_BIN_EXE_eska-check"))
            .current_dir(&root)
            .env("PATH", "")
            .args(["--lang", lang, "--profile", "semantic", "--format", "json"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stderr.is_empty());
        let batch: Value = serde_json::from_slice(&output.stdout).unwrap();
        let batch = &batch["data"]["report"];
        assert!(batch["metadata_issues"].as_array().unwrap().is_empty());
        assert_eq!(batch["coverage"]["metadata_models_complete"], true);
        let finding = batch["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|file| file["source"] == NESTED_MODULE)
            .unwrap();
        let mut client = Client::with_options(&root, lang, &["--profile", "semantic"]);
        client.ask(&initialize());
        client.send(&notification("initialized"));
        let ready = check(&mut client, &source, &snapshot("0"));
        assert_eq!(method(&ready), "CurrentHandler");
        assert_eq!(
            finding["source_sha256"],
            ready["result"]["eska"]["source_sha256"]
        );
        assert_eq!(batch["resources"], ready["result"]["eska"]["resources"]);
        let checked = &finding["findings"][0];
        assert_eq!(checked["code"], ready["result"]["items"][0]["code"]);
        assert_eq!(
            json!({"start":{"line":checked["start_line"],"character":checked["start_column"]},"end":{"line":checked["end_line"],"character":checked["end_column"]}}),
            ready["result"]["items"][0]["range"]
        );
        client.finish();
    }
}
