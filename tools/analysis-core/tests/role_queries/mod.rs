//! Framed resource edits and saved notifications update the same role dependencies as the checker.

use super::*;

const OWNER: &str = "Roles/Reader/object.yaml";
const RIGHTS: &str = "Roles/Reader/Ext/Rights.xml";
const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";

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

/// Reading the owner also prepares its pending resource without activating unrelated roles.
fn metadata(client: &mut Client, snap: &Value) -> Value {
    client.ask(&request(
        json!(31),
        "eska/metadata/read",
        json!({"source":OWNER,"snapshot":snap}),
    ))
}

/// Current invalid rights remain editor-owned across refresh/watch; closing reloads the saved resource.
#[test]
fn role_rights_document_lifecycle_changes_current_coverage_in_both_locales() {
    let (directory, root) = support::project("// Current module.\n", false);
    support::role_project::add_roles(&root, &directory.0.join("native"));
    let source = root.join("src-with-roles");
    let saved = fs::read(source.join(RIGHTS)).unwrap();
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &["--profile", "semantic"]);
        let init = client.ask(&initialize());
        assert_eq!(
            init["result"]["capabilities"]["experimental"]["eska"]["roleRightsSync"],
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
        let opened = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri(&source,RIGHTS),"languageId":"xml","version":7,"text":"<NotRights/>"}}}));
        assert_eq!(opened["method"], "eska/inputChanged");
        assert_eq!(opened["params"]["input"]["kind"], "role_rights");
        assert_eq!(opened["params"]["input"]["owner"], OWNER);
        assert_eq!(opened["params"]["input"]["state"]["input"]["kind"], "ready");
        let read = metadata(&mut client, &opened["params"]["snapshot"]);
        let state = &read["result"]["metadata"]["state"];
        assert_eq!(state["input"]["kind"], "ready");
        assert_eq!(state["model"], "unavailable");
        assert_eq!(
            state["model_issue"],
            json!({"code":"analysis_metadata_dependency_unavailable","failure":{"code":"analysis_role_rights_invalid","location":[RIGHTS]}})
        );
        let rejected = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri(&source,RIGHTS),"version":7},"contentChanges":[{"text":"<Rights/>"}]}}));
        assert_eq!(rejected["method"], "eska/inputRejected");
        assert_eq!(rejected["params"]["snapshot"], opened["params"]["snapshot"]);
        let watched = client.ask(&json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":uri(&source,RIGHTS),"type":2}]}}));
        assert_eq!(watched["params"]["inputs"][0]["invalidated"], false);
        assert_eq!(watched["params"]["snapshot"], opened["params"]["snapshot"]);
        let partial = client.ask(&diagnostic(&source, &watched["params"]["snapshot"]));
        assert_eq!(
            partial["result"]["eska"]["coverage"]["semantic_profile_complete"],
            false
        );
        let closed = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri(&source,RIGHTS)}}}));
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
    assert_eq!(fs::read(source.join(RIGHTS)).unwrap(), saved);
}

/// Registration includes only declared Rights.xml resources; watcher invalidation clears the owner lazily.
#[test]
fn declared_rights_are_watched_and_saved_deletion_cannot_revive_a_role() {
    let (directory, root) = support::project("// Current module.\n", false);
    support::role_project::add_roles(&root, &directory.0.join("native"));
    let source = root.join("src-with-roles");
    let file = source.join(RIGHTS);
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
                .any(|watcher| watcher["globPattern"]["pattern"] == "**/Rights.xml")
        );
        client.ask(&json!({"jsonrpc":"2.0","id":"eska/project-watch/1","result":null}));
        let loaded = metadata(&mut client, &snapshot("0"));
        fs::remove_file(&file).unwrap();
        let watched = client.ask(&json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":uri(&source,RIGHTS),"type":3},{"uri":uri(&source,"Unknown/Rights.xml"),"type":2}]}}));
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
        let changed = client.ask(&json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":uri(&source,RIGHTS),"type":1}]}}));
        let restored = metadata(&mut client, &changed["params"]["snapshot"]);
        assert_eq!(restored["result"]["metadata"]["state"]["model"], "ready");
        client.finish();
    }
    assert_eq!(fs::read(file).unwrap(), saved);
}
