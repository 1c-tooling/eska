//! Detailed model failures share current state across pages, reads and editor acknowledgements.

use super::*;

const OWNER: &str = "CommonModules/Tools/object.yaml";

/// Invalid native property bytes are imported only into an owned fixture, without changing production corpora.
fn project() -> (support::TestDirectory, std::path::PathBuf) {
    let (directory, root) = support::project("// Saved caller.\n", false);
    let file = directory.0.join("native/CommonModules/Tools.xml");
    let native = fs::read_to_string(&file).unwrap();
    fs::write(
        &file,
        native.replace("<Server>true</Server>", "<Server>Future</Server>"),
    )
    .unwrap();
    eska::project_core::source_format::import_designer(
        &directory.0.join("native"),
        &root.join("src-invalid"),
    )
    .unwrap();
    let config = fs::read_to_string(root.join("eska.toml")).unwrap();
    fs::write(
        root.join("eska.toml"),
        config.replace(
            "source_format='eska'",
            "source_format='eska'\nsource='src-invalid'",
        ),
    )
    .unwrap();
    (directory, root)
}

/// Reads preserve editor authority while reporting the same detailed issue as notifications and owner pages.
fn metadata(client: &mut Client, snap: &Value) -> Value {
    let result = client.ask(&request(
        json!("read"),
        "eska/metadata/read",
        json!({"snapshot":snap,"source":OWNER}),
    ));
    assert!(result.get("error").is_none(), "{result}");
    result["result"].clone()
}

/// Domain failures keep byte ranges of the current YAML and do not hide the independently parsed metadata tree.
fn invalid(state: &Value, body: &str) -> Value {
    assert_eq!(state["input"]["kind"], "ready");
    assert_eq!(state["model"], "invalid");
    let issue = &state["model_issue"];
    assert_eq!(issue["code"], "analysis_adapter_invalid");
    assert_eq!(
        issue["issue"],
        json!({"kind":"invalid_property","name":"Server"})
    );
    let start = usize::try_from(issue["range"]["start"].as_u64().unwrap()).unwrap();
    let end = usize::try_from(issue["range"]["end"].as_u64().unwrap()).unwrap();
    assert!(body[start..end].contains("Future"));
    issue.clone()
}

/// Cause details are locale independent, follow current editor ranges, clear on correction and return after saved close.
#[test]
fn detailed_adapter_issue_is_current_in_all_metadata_state_surfaces() {
    let (_directory, root) = project();
    let file = root.join("src-invalid").join(OWNER);
    let saved = fs::read_to_string(&file).unwrap();
    assert!(saved.contains("Future"));
    let draft = format!("# Я😀 current issue\r\n{saved}");
    let fixed = draft.replace("Future", "true");
    let uri = url::Url::from_file_path(&file).unwrap();
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        client.ask(&initialize());
        client.send(&notification("initialized"));
        let mut read = metadata(&mut client, &snapshot("0"));
        let mut snap = read["snapshot"].clone();
        let first = invalid(&read["metadata"]["state"], &saved);
        let mut trace = vec![first.clone()];
        assert!(!read["metadata"]["documents"].as_array().unwrap().is_empty());
        let page = client.ask(&request(
            json!("page"),
            "eska/metadata/owners",
            json!({"snapshot":snap,"limit":200}),
        ));
        let owner = page["result"]["owners"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["source"] == OWNER)
            .unwrap();
        assert_eq!(owner["state"]["model_issue"], first);
        let event=client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri.as_str(),"languageId":"eska-source","version":7,"text":draft}}}));
        assert_eq!(event["method"], "eska/inputChanged");
        snap = event["params"]["snapshot"].clone();
        let current = invalid(&event["params"]["input"]["state"], &draft);
        assert_ne!(current["range"], first["range"]);
        read = metadata(&mut client, &snap);
        assert_eq!(read["metadata"]["state"]["model_issue"], current);
        trace.push(current);
        let event=client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri.as_str(),"version":8},"contentChanges":[{"text":fixed}]}}));
        snap = event["params"]["snapshot"].clone();
        assert_eq!(event["params"]["input"]["state"]["model"], "ready");
        assert!(event["params"]["input"]["state"]["model_issue"].is_null());
        read = metadata(&mut client, &snap);
        assert!(read["metadata"]["state"]["model_issue"].is_null());
        let event=client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri.as_str()}}}));
        let restored = invalid(&event["params"]["input"]["state"], &saved);
        assert_eq!(restored, first);
        trace.push(restored);
        assert_eq!(fs::read_to_string(&file).unwrap(), saved);
        if let Some(previous) = &previous {
            assert_eq!(&trace, previous);
        }
        previous = Some(trace);
        client.finish();
    }
}
