//! Direct common-module completion prepares current flags before it may read the selected body.

use super::*;

#[path = "../support/common_module_project.rs"]
mod project;

const CALLER: &str = "CommonModules/Tools/Ext/Module.bsl";
const TARGET: &str = "CommonModules/Utility/object.yaml";
const MODULE: &str = "CommonModules/Utility/Ext/Module.bsl";
const PREFIX: &str = "\u{feff}// Я😀\r\nProcedure Run()\r\n Value = Utility.";

/// The actual server resolves one cold namespace and consumes the same editor body without touching private methods.
#[test]
fn cold_common_module_completion_uses_current_exports_in_both_locales() {
    let body = format!("{PREFIX};\r\nEndProcedure\r\n");
    let (directory, root) = support::project(&body, false);
    project::add_common_modules(&root, &directory.0.join("native"));
    let source = root.join("src-with-common-modules");
    let uri = url::Url::from_file_path(source.join(MODULE))
        .unwrap()
        .to_string();
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        let init = client.ask(&initialize());
        assert_eq!(
            init["result"]["capabilities"]["experimental"]["eska"]["commonModuleCompletion"],
            true
        );
        client.send(&notification("initialized"));
        let saved = client.ask(&pull(&source, CALLER, "0", PREFIX));
        assert!(labels(&saved).contains("SavedUtility"));
        assert!(!labels(&saved).contains("PrivateUtility"));
        assert!(!labels(&saved).contains("NeighborUtility"));
        assert_eq!(saved["result"]["eska"]["snapshot"], snapshot("4"));
        assert_eq!(
            saved["result"]["eska"]["resources"]["module_text_bytes"],
            body.len() + project::SAVED.len()
        );
        assert_eq!(client.ask(&pull(&source, CALLER, "4", PREFIX)), saved);
        let opened=client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"bsl","version":1,"text":project::CURRENT}}}));
        let current = client.ask(&pull(
            &source,
            CALLER,
            opened["params"]["snapshot"]["revision"].as_str().unwrap(),
            PREFIX,
        ));
        assert!(labels(&current).contains("CurrentUtility"));
        assert!(!labels(&current).contains("SavedUtility"));
        let changed=client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":2},"contentChanges":[{"text":"???"}]}}));
        let invalid = client.ask(&pull(
            &source,
            CALLER,
            changed["params"]["snapshot"]["revision"].as_str().unwrap(),
            PREFIX,
        ));
        assert!(!labels(&invalid).contains("CurrentUtility"));
        assert!(!labels(&invalid).contains("SavedUtility"));
        let closed=client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri}}}));
        let restored = client.ask(&pull(
            &source,
            CALLER,
            closed["params"]["snapshot"]["revision"].as_str().unwrap(),
            PREFIX,
        ));
        assert_eq!(labels(&restored), labels(&saved));
        client.finish();
    }
    assert_eq!(
        fs::read_to_string(source.join(MODULE)).unwrap(),
        project::SAVED
    );
}

/// Current flags prevent a cold body read, later permit it, and hide an already loaded body when YAML becomes invalid.
#[test]
fn current_common_module_visibility_controls_cold_completion_body_loading_in_both_locales() {
    let body = format!("{PREFIX};\r\nEndProcedure\r\n");
    let (directory, root) = support::project(&body, false);
    project::add_common_modules(&root, &directory.0.join("native"));
    let source = root.join("src-with-common-modules");
    let uri = url::Url::from_file_path(source.join(TARGET))
        .unwrap()
        .to_string();
    let saved = fs::read_to_string(source.join(TARGET)).unwrap();
    let denied = saved.replace(
        "<ServerCall>true</ServerCall>",
        "<ServerCall>false</ServerCall>",
    );
    assert_ne!(denied, saved);
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        client.ask(&initialize());
        client.send(&notification("initialized"));
        let opened=client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"yaml","version":1,"text":denied}}}));
        let first = client.ask(&pull(
            &source,
            CALLER,
            opened["params"]["snapshot"]["revision"].as_str().unwrap(),
            PREFIX,
        ));
        assert!(!labels(&first).contains("SavedUtility"));
        assert_eq!(
            first["result"]["eska"]["resources"]["module_text_bytes"],
            body.len()
        );
        let changed=client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":2},"contentChanges":[{"text":saved}]}}));
        let visible = client.ask(&pull(
            &source,
            CALLER,
            changed["params"]["snapshot"]["revision"].as_str().unwrap(),
            PREFIX,
        ));
        assert!(labels(&visible).contains("SavedUtility"));
        assert_eq!(
            visible["result"]["eska"]["resources"]["module_text_bytes"],
            body.len() + project::SAVED.len()
        );
        let invalid=client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":3},"contentChanges":[{"text":"???"}]}}));
        let unavailable = client.ask(&pull(
            &source,
            CALLER,
            invalid["params"]["snapshot"]["revision"].as_str().unwrap(),
            PREFIX,
        ));
        assert!(!labels(&unavailable).contains("SavedUtility"));
        assert_eq!(
            unavailable["result"]["eska"]["resources"]["module_text_bytes"],
            body.len() + project::SAVED.len()
        );
        let closed=client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri}}}));
        let restored = client.ask(&pull(
            &source,
            CALLER,
            closed["params"]["snapshot"]["revision"].as_str().unwrap(),
            PREFIX,
        ));
        assert_eq!(labels(&restored), labels(&visible));
        client.finish();
    }
}
