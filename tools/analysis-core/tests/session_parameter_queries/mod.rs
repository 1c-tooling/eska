//! Framed pulls publish configured native fields and withdraw invalid current input in both locales.

#[path = "../support/session_parameter_project.rs"]
mod fixture;

use super::*;
use std::collections::BTreeSet;

const OWNER: &str = "SessionParameters/CurrentUser/object.yaml";
const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";
const BODY: &str = "Procedure Run()\n Value = SessionParameters.CurrentUser;\nEndProcedure\n";

/// Every URI addresses a declared input in this test's own source tree.
fn uri(source: &Path, path: &str) -> String {
    url::Url::from_file_path(source.join(path)).unwrap().into()
}

/// Cold family preparation must not fabricate a semantic metadata manager.
fn completion(source: &Path, snap: &Value) -> Value {
    request(
        json!(31),
        "textDocument/completion",
        json!({
            "textDocument":{"uri":uri(source,MODULE)},
            "position":{"line":1,"character":" Value = SessionParameters.".len()},
            "eska":{"snapshot":snap}
        }),
    )
}

/// Optional native presentation does not affect configured member identity.
fn labels(response: &Value) -> BTreeSet<String> {
    response["result"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["label"].as_str().unwrap().into())
        .collect()
}

/// Existing metadata and BSL routes share snapshots, invalid-current retraction and saved recovery.
#[test]
fn session_parameter_framed_completion_and_metadata_follow_current_input() {
    let (directory, root) = support::project(BODY, false);
    fixture::add_parameters(&root, &directory.0.join("native"));
    let source = root.join("src-with-parameters");
    let saved = fs::read_to_string(source.join(OWNER)).unwrap();
    let invalid = saved.replace("x:string", "c:FutureType");
    assert_ne!(invalid, saved);
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &["--profile", "semantic"]);
        client.ask(&initialize());
        client.send(&notification("initialized"));
        let ready = client.ask(&completion(&source, &snapshot("0")));
        assert!(ready.get("error").is_none(), "{ready}");
        let names = labels(&ready);
        assert!(names.contains("CurrentUser") && names.contains("Neighbor"));
        let snap = ready["result"]["eska"]["snapshot"].clone();
        let metadata = client.ask(&request(
            json!(32),
            "eska/metadata/read",
            json!({"source":OWNER,"snapshot":snap}),
        ));
        assert_eq!(metadata["result"]["metadata"]["state"]["model"], "ready");
        let changed = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri(&source,OWNER),"languageId":"yaml","version":7,"text":invalid}
        }}));
        assert_eq!(changed["params"]["input"]["state"]["model"], "invalid");
        let snap = changed["params"]["snapshot"].clone();
        let current = labels(&client.ask(&completion(&source, &snap)));
        assert!(!current.contains("CurrentUser"));
        assert!(current.contains("Neighbor"));
        let diagnostic = client.ask(&request(json!(33), "textDocument/diagnostic", json!({
            "textDocument":{"uri":uri(&source,OWNER)},"identifier":"eska-semantic","eska":{"snapshot":snap}
        })));
        assert_eq!(
            diagnostic["result"]["items"][0]["code"],
            "analysis_adapter_invalid"
        );
        let mut finding = diagnostic["result"]["items"][0].clone();
        finding.as_object_mut().unwrap().remove("message");
        let closed = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri(&source,OWNER)}}}));
        assert_eq!(
            labels(&client.ask(&completion(&source, &closed["params"]["snapshot"]))),
            names
        );
        if let Some(previous) = &previous {
            assert_eq!(&(names.clone(), finding.clone()), previous);
        }
        previous = Some((names, finding));
        client.finish();
    }
    assert_eq!(fs::read_to_string(source.join(OWNER)).unwrap(), saved);
}
