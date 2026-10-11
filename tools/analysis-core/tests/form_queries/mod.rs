//! Framed resource edits and saved notifications update the same form dependencies as the checker.

use super::*;

const OWNER: &str = "CommonForms/Dialog/object.yaml";
const DIALOG: &str = "CommonForms/Dialog/Ext/Form.xml";
const MODULE: &str = "CommonForms/Dialog/Ext/Form/Module.bsl";

/// URI construction uses only this test's imported source.
fn uri(source: &Path, path: &str) -> String {
    url::Url::from_file_path(source.join(path)).unwrap().into()
}

/// The resident diagnostic negotiates the same explicit semantic profile used by batch queries.
fn diagnostic(source: &Path, snap: &Value) -> Value {
    request(
        json!(30),
        "textDocument/diagnostic",
        json!({"textDocument":{"uri":uri(source,MODULE)},"identifier":"eska-semantic","eska":{"snapshot":snap}}),
    )
}

/// Reading the owner also prepares its pending resource without activating unrelated forms.
fn metadata(client: &mut Client, snap: &Value) -> Value {
    client.ask(&request(
        json!(31),
        "eska/metadata/read",
        json!({"source":OWNER,"snapshot":snap}),
    ))
}

/// Current invalid dialog remain editor-owned across refresh/watch; closing reloads the saved resource.
#[test]
fn form_dialog_document_lifecycle_changes_current_coverage_in_both_locales() {
    let (directory, root) = support::project("// Current module.\n", false);
    support::form_project::add_forms(&root, &directory.0.join("native"), false);
    let source = root.join("src-with-forms");
    let saved = fs::read(source.join(DIALOG)).unwrap();
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &["--profile", "semantic"]);
        let init = client.ask(&initialize());
        assert_eq!(
            init["result"]["capabilities"]["experimental"]["eska"]["formDialogSync"],
            true
        );
        client.send(&notification("initialized"));
        let lazy = metadata(&mut client, &snapshot("0"));
        assert_eq!(lazy["result"]["metadata"]["state"]["model"], "ready");
        assert_eq!(lazy["result"]["snapshot"], snapshot("2"));
        let ready = client.ask(&diagnostic(&source, &lazy["result"]["snapshot"]));
        assert_eq!(
            ready["result"]["eska"]["coverage"]["semantic_profile_complete"],
            true
        );
        let opened = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri(&source,DIALOG),"languageId":"xml","version":7,"text":"<NotForm/>"}}}));
        assert_eq!(opened["method"], "eska/inputChanged");
        assert_eq!(opened["params"]["input"]["kind"], "form_dialog");
        assert_eq!(opened["params"]["input"]["owner"], OWNER);
        assert_eq!(opened["params"]["input"]["state"]["input"]["kind"], "ready");
        let read = metadata(&mut client, &opened["params"]["snapshot"]);
        let state = &read["result"]["metadata"]["state"];
        assert_eq!(state["input"]["kind"], "ready");
        assert_eq!(state["model"], "unavailable");
        assert_eq!(
            state["model_issue"],
            json!({"code":"analysis_metadata_dependency_unavailable","failure":{"code":"analysis_form_dialog_invalid","location":[DIALOG]}})
        );
        let rejected = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri(&source,DIALOG),"version":7},"contentChanges":[{"text":"<Form/>"}]}}));
        assert_eq!(rejected["method"], "eska/inputRejected");
        assert_eq!(rejected["params"]["snapshot"], opened["params"]["snapshot"]);
        let watched = client.ask(&json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":uri(&source,DIALOG),"type":2}]}}));
        assert_eq!(watched["params"]["inputs"][0]["invalidated"], false);
        assert_eq!(watched["params"]["snapshot"], opened["params"]["snapshot"]);
        let partial = client.ask(&diagnostic(&source, &watched["params"]["snapshot"]));
        assert_eq!(
            partial["result"]["eska"]["coverage"]["semantic_profile_complete"],
            false
        );
        let closed = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri(&source,DIALOG)}}}));
        assert_eq!(
            closed["params"]["input"]["state"]["origin"]["kind"],
            "saved"
        );
        let restored = client.ask(&diagnostic(&source, &closed["params"]["snapshot"]));
        assert_eq!(
            restored["result"]["eska"]["coverage"]["semantic_profile_complete"],
            true
        );
        if let Some(previous) = &previous {
            assert_eq!(state, previous);
        }
        previous = Some(state.clone());
        client.finish();
    }
    assert_eq!(fs::read(source.join(DIALOG)).unwrap(), saved);
}

/// Registration includes only declared Form.xml resources; watcher invalidation clears the owner lazily.
#[test]
fn declared_dialogs_are_watched_and_saved_deletion_cannot_revive_a_form() {
    let (directory, root) = support::project("// Current module.\n", false);
    support::form_project::add_forms(&root, &directory.0.join("native"), false);
    let source = root.join("src-with-forms");
    let file = source.join(DIALOG);
    let saved = fs::read(&file).unwrap();
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        let mut init = initialize();
        init["params"]["capabilities"]["workspace"] = json!({"didChangeWatchedFiles":{"dynamicRegistration":true,"relativePatternSupport":true}});
        client.ask(&init);
        client.send(&notification("initialized"));
        let registration = client.receive();
        assert!(
            registration["params"]["registrations"][1]["registerOptions"]["watchers"]
                .as_array()
                .unwrap()
                .iter()
                .any(|watcher| watcher["globPattern"]["pattern"] == "**/Form.xml")
        );
        client.ask(&json!({"jsonrpc":"2.0","id":"eska/project-watch/1","result":null}));
        let loaded = metadata(&mut client, &snapshot("0"));
        fs::remove_file(&file).unwrap();
        let watched = client.ask(&json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":uri(&source,DIALOG),"type":3},{"uri":uri(&source,"Unknown/Form.xml"),"type":2}]}}));
        assert_eq!(watched["params"]["ignoredSources"], 1);
        assert_eq!(watched["params"]["inputs"][0]["invalidated"], true);
        assert_eq!(
            watched["params"]["inputs"][0]["input"]["state"]["input"]["kind"],
            "pending"
        );
        assert_ne!(watched["params"]["snapshot"], loaded["result"]["snapshot"]);
        let read = metadata(&mut client, &watched["params"]["snapshot"]);
        assert_eq!(read["result"]["metadata"]["state"]["model"], "unavailable");
        assert_eq!(
            read["result"]["metadata"]["state"]["model_issue"]["failure"]["code"],
            "source_file_missing"
        );
        fs::write(&file, &saved).unwrap();
        let changed = client.ask(&json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":uri(&source,DIALOG),"type":1}]}}));
        let restored = metadata(&mut client, &changed["params"]["snapshot"]);
        assert_eq!(restored["result"]["metadata"]["state"]["model"], "ready");
        client.finish();
    }
    assert_eq!(fs::read(file).unwrap(), saved);
}

/// Select the actual native rule rather than depending on the pinned default diagnostics set.
fn handler_settings(root: &Path) {
    use std::fmt::Write as _;
    let text = ide::all_diagnostic_codes().fold(String::new(), |mut text, code| {
        writeln!(
            text,
            "{}={}",
            code.as_str(),
            code == eska_analysis::DiagnosticCode::UnusedLocalMethod
        )
        .unwrap();
        text
    });
    fs::write(
        root.join("bsl-analyzer.toml"),
        "[diagnostics.parameters]\n".to_owned() + &text,
    )
    .unwrap();
}

/// Read UTF-16 ranges back through the ASCII method lines, verifying the affected handler's name.
fn method(result: &Value) -> &'static str {
    let items = result["result"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{result}");
    assert_eq!(items[0]["code"], "UnusedLocalMethod");
    let range = &items[0]["range"];
    let line = support::form_project::BODY
        .lines()
        .nth(usize::try_from(range["start"]["line"].as_u64().unwrap()).unwrap())
        .unwrap();
    let start = usize::try_from(range["start"]["character"].as_u64().unwrap()).unwrap();
    let end = usize::try_from(range["end"]["character"].as_u64().unwrap()).unwrap();
    // This prefix contains Cyrillic, so wire UTF-16 positions must not index UTF-8 bytes.
    let start = line.char_indices().nth(start).unwrap().0;
    let end = line
        .char_indices()
        .nth(end)
        .map_or(line.len(), |(index, _)| index);
    &line[start..end]
}

/// Full XML open/change/save/close changes native BSL semantics without modifying the saved module.
#[test]
fn current_dialog_events_change_unused_handler_diagnostics_in_both_locales() {
    let (directory, root) = support::project("// Shared module.\n", false);
    support::form_project::add_forms(&root, &directory.0.join("native"), false);
    handler_settings(&root);
    let source = root.join("src-with-forms");
    let saved = fs::read(source.join(DIALOG)).unwrap();
    let body = fs::read(source.join(MODULE)).unwrap();
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &["--profile", "semantic"]);
        client.ask(&initialize());
        client.send(&notification("initialized"));
        let ready = client.ask(&diagnostic(&source, &snapshot("0")));
        assert_eq!(method(&ready), "CurrentHandler");
        let changed = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri(&source,DIALOG),"languageId":"xml","version":1,"text":support::form_project::dialog("CurrentHandler")}}}));
        let current = client.ask(&diagnostic(&source, &changed["params"]["snapshot"]));
        assert_eq!(method(&current), "SavedHandler");
        let updated = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri(&source,DIALOG),"version":2},"contentChanges":[{"text":support::form_project::dialog("SavedHandler")}]}}));
        let current = client.ask(&diagnostic(&source, &updated["params"]["snapshot"]));
        assert_eq!(method(&current), "CurrentHandler");
        let saved_event = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didSave","params":{"textDocument":{"uri":uri(&source,DIALOG)},"text":support::form_project::dialog("CurrentHandler")}}));
        assert_eq!(
            saved_event["params"]["input"]["state"]["origin"]["kind"],
            "overlay"
        );
        let current = client.ask(&diagnostic(&source, &saved_event["params"]["snapshot"]));
        assert_eq!(method(&current), "CurrentHandler");
        assert_eq!(
            saved_event["params"]["snapshot"],
            updated["params"]["snapshot"]
        );
        let closed = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri(&source,DIALOG)}}}));
        let restored = client.ask(&diagnostic(&source, &closed["params"]["snapshot"]));
        assert_eq!(method(&restored), "CurrentHandler");
        let mut finding = current["result"]["items"][0].clone();
        finding.as_object_mut().unwrap().remove("message");
        if let Some(previous) = &previous {
            assert_eq!(&finding, previous);
        }
        previous = Some(finding);
        client.finish();
    }
    assert_eq!(fs::read(source.join(DIALOG)).unwrap(), saved);
    assert_eq!(fs::read(source.join(MODULE)).unwrap(), body);
}

/// Batch and IDE entry points consume equal saved inputs, findings and resource reservations.
#[test]
#[cfg(feature = "check-cli")]
fn checker_and_ide_consume_the_same_current_form_context() {
    let (directory, root) = support::project("// Shared module.\n", false);
    support::form_project::add_forms(&root, &directory.0.join("native"), false);
    handler_settings(&root);
    let source = root.join("src-with-forms");
    for lang in ["en-US", "ru-RU"] {
        let output = Command::new(env!("CARGO_BIN_EXE_eska-check"))
            .current_dir(&root)
            .env("PATH", "")
            .args(["--lang", lang, "--profile", "semantic", "--format", "json"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stderr.is_empty());
        let check: Value = serde_json::from_slice(&output.stdout).unwrap();
        let batch = &check["data"]["report"];
        assert_eq!(batch["coverage"]["metadata_models_complete"], true);
        let checked = batch["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|file| file["source"] == MODULE)
            .unwrap();
        let mut client = Client::with_options(&root, lang, &["--profile", "semantic"]);
        client.ask(&initialize());
        client.send(&notification("initialized"));
        let ready = client.ask(&diagnostic(&source, &snapshot("0")));
        assert_eq!(method(&ready), "CurrentHandler");
        let items = ready["result"]["items"].as_array().unwrap();
        assert_eq!(checked["findings"][0]["code"], items[0]["code"]);
        let checked_finding = &checked["findings"][0];
        assert_eq!(
            json!({"start":{"line":checked_finding["start_line"],"character":checked_finding["start_column"]},"end":{"line":checked_finding["end_line"],"character":checked_finding["end_column"]}}),
            items[0]["range"]
        );
        assert_eq!(
            checked["source_sha256"],
            ready["result"]["eska"]["source_sha256"]
        );
        assert_eq!(batch["resources"], ready["result"]["eska"]["resources"]);
        client.finish();
    }
}
