//! Own metadata validation keeps the existing diagnostic route despite absent owning-base semantics.

use super::*;

/// The scoped capability retains the selected profile identifier while restricting input to owner YAML.
fn pull(root: &Path, owner: &SourcePath, snapshot: &Value) -> Value {
    request(
        json!(50),
        "textDocument/diagnostic",
        json!({"textDocument":{"uri":uri(root,owner)},"identifier":"eska-semantic","eska":{"snapshot":snapshot}}),
    )
}

/// Every full-document change acknowledges exactly one current source revision without writing saved bytes.
fn change(root: &Path, owner: &SourcePath, text: &str, version: u32, open: bool) -> Value {
    let uri = uri(root, owner);
    if open {
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"eska-source","version":version,"text":text}}})
    } else {
        json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":version},"contentChanges":[{"text":text}]}})
    }
}

/// Both external owners report current adapter failures, syntax refusal and repairs independently of full BSL rules.
#[test]
fn external_metadata_diagnostics_keep_their_scoped_capability_and_invalid_repair_contract() {
    for kind in [ProjectType::Processing, ProjectType::Report] {
        let directory = TestDirectory::new();
        let root = fixture::project(&directory.0, kind);
        let paths = fixture::paths(&root);
        let physical = root.join("src").join(paths.owner.as_path());
        let saved = fs::read_to_string(&physical).unwrap();
        let invalid = saved.replace("xs:boolean", "xs:FutureType");
        assert_ne!(invalid, saved);
        let mut previous = None;
        for lang in ["ru-RU", "en-US"] {
            let (mut client, opened) = start(&root, lang);
            let cap = &opened["result"]["capabilities"]["experimental"]["eska"];
            assert_eq!(cap["analysis"]["metadataEnabled"], true);
            assert_eq!(cap["analysis"]["semanticEnabled"], false);
            assert_eq!(
                cap["metadata"]["diagnostics"]["method"],
                "textDocument/diagnostic"
            );
            assert_eq!(
                cap["metadata"]["diagnostics"]["scope"],
                "declared-owner-yaml"
            );
            let changed = client.ask(&change(&root, &paths.owner, &invalid, 1, true));
            let checked = client.ask(&pull(&root, &paths.owner, &changed["params"]["snapshot"]));
            assert!(checked.get("error").is_none(), "{checked}");
            assert_eq!(checked["result"]["eska"]["profile"], "metadata");
            assert_eq!(
                checked["result"]["eska"]["coverage"]["metadata_context_complete"],
                false
            );
            assert_eq!(
                checked["result"]["eska"]["coverage"]["metadata_model_complete"],
                false
            );
            let mut items = checked["result"]["items"].clone();
            assert_eq!(items.as_array().unwrap().len(), 1, "{checked}");
            assert_eq!(items[0]["code"], "analysis_adapter_invalid");
            items[0].as_object_mut().unwrap().remove("message");
            if let Some(previous) = &previous {
                assert_eq!(previous, &items);
            }
            previous = Some(items);
            let changed = client.ask(&change(&root, &paths.owner, "broken: [\n", 2, false));
            let refused = client.ask(&pull(&root, &paths.owner, &changed["params"]["snapshot"]));
            assert_eq!(
                refused["error"]["data"]["kind"],
                "metadata_input_unavailable"
            );
            let changed = client.ask(&change(&root, &paths.owner, &saved, 3, false));
            let repaired = client.ask(&pull(&root, &paths.owner, &changed["params"]["snapshot"]));
            assert_eq!(repaired["result"]["items"], json!([]));
            assert_eq!(
                repaired["result"]["eska"]["coverage"]["metadata_model_complete"],
                true
            );
            assert_eq!(
                repaired["result"]["eska"]["coverage"]["metadata_context_complete"],
                false
            );
            assert_eq!(fs::read_to_string(&physical).unwrap(), saved);
            client.finish();
        }
        assert!(!root.join(".eska").exists());
    }
}
