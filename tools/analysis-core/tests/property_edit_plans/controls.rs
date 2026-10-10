//! Plan requests stop without changing the current editor input or weakening accepted trust.

use super::*;

/// Versioned edits and cancellation advertise separate contracts from language methods.
fn open(client: &mut Client, root: &Path, text: &str) -> Value {
    let initialized = client.ask(&initialize_edits());
    let capabilities = &initialized["result"]["capabilities"]["experimental"]["eska"];
    assert_eq!(
        capabilities["languageMethods"],
        json!([
            "textDocument/diagnostic",
            "textDocument/completion",
            "textDocument/definition",
            "textDocument/documentSymbol",
            "textDocument/hover",
            "textDocument/signatureHelp",
            "textDocument/references"
        ])
    );
    assert_eq!(
        capabilities["cancellableMethods"],
        json!([
            "textDocument/diagnostic",
            "textDocument/completion",
            "textDocument/definition",
            "textDocument/documentSymbol",
            "textDocument/hover",
            "textDocument/signatureHelp",
            "textDocument/references",
            METHOD
        ])
    );
    client.send(&notification("initialized"));
    let uri = url::Url::from_file_path(root.join("src-plans").join(OWNER)).unwrap();
    client.ask(
        &json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri.as_str(),"languageId":"yaml","version":1,"text":text}}}),
    )
}

/// Zero and out-of-range deadlines are refused before absent baseline IO after valid input guards.
#[test]
fn property_plan_deadline_preserves_preflight_editor_and_metadata_in_both_locales() {
    let (_directory, root, reference) = fixture::project(false);
    let path = root.join("src-plans").join(OWNER);
    let text = fs::read_to_string(&path).unwrap();
    fs::remove_dir_all(root.join(".eska/accepted")).unwrap();
    for (timeout, expected) in [
        ("0", "analysis_deadline_exceeded"),
        ("18446744073709551615", "analysis_timeout_out_of_range"),
    ] {
        let mut previous = None;
        for lang in ["en-US", "ru-RU"] {
            let mut client = Client::with_options(
                &root,
                lang,
                &[
                    "--accepted-base-sha256",
                    &reference.sha256,
                    "--timeout-seconds",
                    timeout,
                ],
            );
            let opened = open(&mut client, &root, &text);
            let error = client.ask(&prepare(&text, "1", 1, "Updated"));
            assert!(error.get("result").is_none(), "{error}");
            assert_eq!(error["error"]["code"], -32000);
            assert_eq!(error["error"]["data"]["kind"], expected);
            assert_eq!(error["error"]["data"]["details"], snapshot("1"));
            assert!(error["error"]["data"].get("retriggerRequest").is_none());
            if let Some(previous) = &previous {
                assert_eq!(&error, previous);
            }
            previous = Some(error);
            let version = client.ask(&prepare(&text, "1", 2, "Updated"));
            assert_eq!(
                version["error"]["data"]["kind"],
                "source_edit_document_version"
            );
            let metadata = client.ask(&request(
                json!("state"),
                "eska/metadata/owners",
                json!({"snapshot":snapshot("1"),"limit":1}),
            ));
            assert_eq!(metadata["result"]["snapshot"], snapshot("1"));
            assert_eq!(
                metadata["result"]["resources"],
                opened["params"]["resources"]
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), text);
            client.finish();
        }
    }
}

/// An independently pinned base remains usable after cancellation; unknown/late IDs cannot poison it.
#[test]
fn cancelled_property_plan_preserves_input_and_allows_id_reuse_in_both_locales() {
    let (_directory, root, reference) = fixture::project_with_payload(false, 8 * 1024 * 1024);
    let path = root.join("src-plans").join(OWNER);
    let text = fs::read_to_string(&path).unwrap();
    for (lang, id) in [("en-US", json!(-9)), ("ru-RU", json!("отмена плана"))] {
        let mut client =
            Client::with_options(&root, lang, &["--accepted-base-sha256", &reference.sha256]);
        let opened = open(&mut client, &root, &text);
        let mut plan = prepare(&text, "1", 1, "Updated");
        plan["id"] = id.clone();
        client.send(&plan);
        client.send(&json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":id}}));
        let error = client.receive();
        assert_eq!(error["id"], id);
        assert!(error.get("result").is_none(), "{error}");
        assert_eq!(error["error"]["code"], -32800);
        assert_eq!(error["error"]["data"]["kind"], "analysis_cancelled");
        assert_eq!(error["error"]["data"]["details"], snapshot("1"));
        let metadata = client.ask(&request(
            id.clone(),
            "eska/metadata/owners",
            json!({"snapshot":snapshot("1"),"limit":1}),
        ));
        assert_eq!(metadata["result"]["snapshot"], snapshot("1"));
        assert_eq!(
            metadata["result"]["resources"],
            opened["params"]["resources"]
        );
        client.send(&json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":id}}));
        let later = client.ask(&plan);
        assert!(later.get("result").is_some(), "{later}");
        assert_eq!(later["result"]["snapshot"], snapshot("1"));
        assert_eq!(later["result"]["document_version"], 1);
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
        client.finish();
    }
}
