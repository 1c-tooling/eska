//! Framed completion prepares one current manager body without a prior metadata or module read.

use super::*;
use crate::manager_project as project;

const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";
const MANAGER: &str = "Catalogs/Items/Ext/ManagerModule.bsl";
const PREFIX: &str = "\u{feff}// Я😀\r\nProcedure Run()\r\n Value = Catalogs.Items.";

/// Resource, metadata and module preparation advance one shared snapshot and preserve current export visibility.
#[test]
fn cold_manager_completion_prepares_current_exports_and_tracks_editor_body_in_both_locales() {
    let body = format!("{PREFIX};\r\nEndProcedure\r\n");
    let (directory, root) = support::project(&body, false);
    project::add_managers(&root, &directory.0.join("native"));
    let source = root.join("src-with-managers");
    let manager_uri = url::Url::from_file_path(source.join(MANAGER))
        .unwrap()
        .to_string();
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        let init = client.ask(&initialize());
        assert_eq!(
            init["result"]["capabilities"]["experimental"]["eska"]["managerModuleCompletion"],
            true
        );
        client.send(&notification("initialized"));
        let saved = client.ask(&pull(&source, MODULE, "0", PREFIX));
        let saved_labels = labels(&saved);
        assert!(saved_labels.contains("SavedMethod"));
        assert!(!saved_labels.contains("HiddenMethod"));
        assert!(saved_labels.contains("Saved"));
        assert_eq!(saved["result"]["eska"]["snapshot"], snapshot("5"));
        assert_eq!(
            saved["result"]["eska"]["resources"]["module_text_bytes"],
            body.len() + project::SAVED.len()
        );
        let repeated = client.ask(&pull(&source, MODULE, "5", PREFIX));
        assert_eq!(repeated, saved);
        let opened=client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":manager_uri,"languageId":"bsl","version":1,"text":project::CURRENT}}}));
        assert_eq!(opened["params"]["input"]["kind"], "module");
        let current = client.ask(&pull(
            &source,
            MODULE,
            opened["params"]["snapshot"]["revision"].as_str().unwrap(),
            PREFIX,
        ));
        let current_labels = labels(&current);
        assert!(current_labels.contains("CurrentMethod"));
        assert!(!current_labels.contains("SavedMethod"));
        let watched=client.ask(&json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":manager_uri,"type":2}]}}));
        assert_eq!(watched["params"]["inputs"][0]["invalidated"], false);
        assert_eq!(
            client.ask(&pull(
                &source,
                MODULE,
                current["result"]["eska"]["snapshot"]["revision"]
                    .as_str()
                    .unwrap(),
                PREFIX
            )),
            current
        );
        let changed=client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":manager_uri,"version":2},"contentChanges":[{"text":"???"}]}}));
        let invalid = client.ask(&pull(
            &source,
            MODULE,
            changed["params"]["snapshot"]["revision"].as_str().unwrap(),
            PREFIX,
        ));
        assert!(!labels(&invalid).contains("SavedMethod"));
        assert!(!labels(&invalid).contains("CurrentMethod"));
        assert!(labels(&invalid).contains("Saved"));
        let closed=client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":manager_uri}}}));
        let restored = client.ask(&pull(
            &source,
            MODULE,
            closed["params"]["snapshot"]["revision"].as_str().unwrap(),
            PREFIX,
        ));
        assert_eq!(labels(&restored), saved_labels);
        client.finish();
    }
    assert_eq!(
        fs::read_to_string(source.join(MANAGER)).unwrap(),
        project::SAVED
    );
}

/// Missing saved text removes exports, never retries on its own, and recovers after an explicit watched change.
#[test]
fn missing_manager_completion_preserves_saved_failure_until_watched_recreation() {
    let body = format!("{PREFIX};\r\nEndProcedure\r\n");
    let (directory, root) = support::project(&body, false);
    project::add_managers(&root, &directory.0.join("native"));
    let source = root.join("src-with-managers");
    let manager = source.join(MANAGER);
    let uri = url::Url::from_file_path(&manager).unwrap().to_string();
    for lang in ["en-US", "ru-RU"] {
        fs::remove_file(&manager).unwrap();
        let mut client = Client::with_options(&root, lang, &[]);
        client.ask(&initialize());
        client.send(&notification("initialized"));
        let missing = client.ask(&pull(&source, MODULE, "0", PREFIX));
        assert!(!labels(&missing).contains("SavedMethod"));
        assert!(labels(&missing).contains("Saved"));
        assert_eq!(missing["result"]["eska"]["snapshot"], snapshot("5"));
        assert_eq!(
            missing["result"]["eska"]["resources"]["module_text_bytes"],
            body.len()
        );
        fs::write(&manager, project::SAVED).unwrap();
        assert_eq!(client.ask(&pull(&source, MODULE, "5", PREFIX)), missing);
        let watched=client.ask(&json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":uri,"type":1}]}}));
        let ready = client.ask(&pull(
            &source,
            MODULE,
            watched["params"]["snapshot"]["revision"].as_str().unwrap(),
            PREFIX,
        ));
        assert!(labels(&ready).contains("SavedMethod"));
        client.finish();
    }
    assert_eq!(fs::read_to_string(manager).unwrap(), project::SAVED);
}
