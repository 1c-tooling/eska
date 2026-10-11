//! The actual IDE process consumes subscription inputs through the shared canonical session.

#[path = "../support/event_subscription_project.rs"]
mod fixture;

use super::*;
use std::collections::BTreeSet;

const OWNER: &str = "EventSubscriptions/BeforeSave/object.yaml";
const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";
const BODY: &str =
    "Procedure Run()\n Value = Metadata.EventSubscriptions.BeforeSave;\nEndProcedure\n";

/// Source URI construction uses the declared imported paths only.
fn uri(source: &Path, path: &str) -> String {
    url::Url::from_file_path(source.join(path)).unwrap().into()
}

/// Every pull shares the negotiated profile and the last acknowledged input revision.
fn diagnostic(source: &Path, path: &str, snap: &Value) -> Value {
    request(
        json!(20),
        "textDocument/diagnostic",
        json!({"textDocument":{"uri":uri(source,path)},
        "identifier":"eska-semantic","eska":{"snapshot":snap}}),
    )
}

/// Query at a valid expression's subscription collection without constructing invalid BSL.
fn completion(source: &Path, snap: &Value) -> Value {
    request(
        json!(21),
        "textDocument/completion",
        json!({"textDocument":{"uri":uri(source,MODULE)},
        "position":{"line":1,"character":" Value = Metadata.EventSubscriptions.".len()},
        "eska":{"snapshot":snap}}),
    )
}

/// Compare actual semantic candidates separately from optional native presentation labels.
fn labels(response: &Value) -> BTreeSet<String> {
    response["result"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["label"].as_str().unwrap().into())
        .collect()
}

/// Metadata edits, invalid source diagnostics and close affect the same BSL completion/coverage state.
#[test]
fn subscription_inputs_reach_ide_completions_and_current_metadata_diagnostics() {
    let (directory, root) = support::project(BODY, false);
    fixture::add_subscriptions(&root, &directory.0.join("native"));
    let source = root.join("src-with-subscriptions");
    let text = fs::read_to_string(source.join(OWNER)).unwrap();
    let invalid = text.replace("cfg:CatalogObject.Items", "unbound:CatalogObject.Items");
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &["--profile", "semantic"]);
        client.ask(&initialize());
        client.send(&notification("initialized"));
        let ready = client.ask(&diagnostic(&source, MODULE, &snapshot("0")));
        assert!(ready.get("error").is_none(), "{ready}");
        assert_eq!(
            ready["result"]["eska"]["coverage"]["semantic_profile_complete"],
            true
        );
        let snap = &ready["result"]["eska"]["snapshot"];
        let candidates = client.ask(&completion(&source, snap));
        let names = labels(&candidates);
        assert!(
            names.contains("BeforeSave") && names.contains("Neighbor"),
            "{candidates}"
        );
        let metadata = client.ask(&request(
            json!(22),
            "eska/metadata/read",
            json!({"source":OWNER,"snapshot":snap}),
        ));
        assert_eq!(metadata["result"]["metadata"]["state"]["model"], "ready");
        let changed = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri(&source,OWNER),"languageId":"yaml","version":7,"text":invalid}}}));
        let snap = &changed["params"]["snapshot"];
        assert_eq!(changed["params"]["input"]["state"]["model"], "invalid");
        let partial = client.ask(&diagnostic(&source, MODULE, snap));
        assert_eq!(
            partial["result"]["eska"]["coverage"]["semantic_profile_complete"],
            false
        );
        let metadata = client.ask(&diagnostic(&source, OWNER, snap));
        let item = &metadata["result"]["items"][0];
        assert_eq!(item["code"], "analysis_adapter_invalid");
        assert_eq!(item["data"]["issue"]["kind"], "unresolved_type_name");
        let mut finding = item.clone();
        finding.as_object_mut().unwrap().remove("message");
        let closed = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri(&source,OWNER)}}}));
        let restored = client.ask(&diagnostic(&source, MODULE, &closed["params"]["snapshot"]));
        assert_eq!(
            restored["result"]["eska"]["coverage"]["semantic_profile_complete"],
            true
        );
        let current = client.ask(&completion(
            &source,
            &restored["result"]["eska"]["snapshot"],
        ));
        assert_eq!(labels(&current), names);
        if let Some(previous) = &previous {
            assert_eq!(&(names.clone(), finding.clone()), previous);
        }
        previous = Some((names, finding));
        client.finish();
    }
    assert_eq!(fs::read_to_string(source.join(OWNER)).unwrap(), text);
}
