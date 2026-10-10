//! Actual stdio locations validate Unicode, editor authority, explicit incompleteness and controls.

use super::*;

const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";
const BODY: &str = "\u{feff}// Я😀\r\nПроцедура Цель()\r\nКонецПроцедуры\r\nЦель();\r\n";

/// The cursor refers to the same declared local body and optional exact ESKA snapshot.
fn pull(root: &Path, revision: &str) -> Value {
    let uri = url::Url::from_file_path(root.join("src").join(MODULE)).unwrap();
    request(
        json!("переход"),
        "textDocument/definition",
        json!({"textDocument":{"uri":uri.as_str()},"position":{"line":3,"character":2},
        "eska":{"snapshot":snapshot(revision)},"workDoneToken":"accepted","partialResultToken":1}),
    )
}

/// A standard location and its extension describe the current target, not a stale saved file.
#[test]
fn definition_uses_current_unicode_bsl_and_close_restores_saved_target_in_both_locales() {
    let (_directory, root) = support::project(BODY, false);
    let path = root.join("src").join(MODULE);
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        let mut init = initialize();
        init["params"]["capabilities"] =
            json!({"textDocument":{"definition":{"linkSupport":true}}});
        let initialized = client.ask(&init);
        assert_eq!(
            initialized["result"]["capabilities"]["definitionProvider"],
            true
        );
        assert_eq!(
            initialized["result"]["capabilities"]["experimental"]["eska"]["definition"]["scope"],
            "ready-bsl-modules"
        );
        client.send(&notification("initialized"));
        let first = client.ask(&pull(&root, "0"));
        assert_eq!(first["result"]["eska"]["snapshot"], snapshot("1"));
        assert_eq!(
            first["result"]["range"]["start"],
            json!({"line":1,"character":0})
        );
        assert_eq!(first["result"]["eska"]["target"]["source"], MODULE);
        assert_eq!(
            first["result"]["eska"]["target"]["source_sha256"],
            first["result"]["eska"]["source_sha256"]
        );
        assert_eq!(
            first["result"]["eska"]["coverage"]["semantic_complete"],
            false
        );
        let changed = format!(
            "\u{feff}// Another current line.\r\n{}",
            BODY.trim_start_matches('\u{feff}')
        );
        let uri = url::Url::from_file_path(&path).unwrap();
        let opened = client.ask(
            &json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri.as_str(),"version":1,"languageId":"bsl","text":changed}}}),
        );
        let mut current = pull(&root, "2");
        current["params"]["position"]["line"] = json!(4);
        let location = client.ask(&current);
        assert_eq!(
            location["result"]["range"]["start"],
            json!({"line":2,"character":0})
        );
        assert_eq!(
            location["result"]["eska"]["resources"],
            opened["params"]["resources"]
        );
        assert_ne!(
            location["result"]["eska"]["target"]["source_sha256"],
            first["result"]["eska"]["target"]["source_sha256"]
        );
        let closed = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri.as_str()}}}));
        assert_eq!(closed["params"]["snapshot"], snapshot("3"));
        let restored = client.ask(&pull(&root, "3"));
        assert_eq!(restored["result"]["range"], first["result"]["range"]);
        assert_eq!(
            restored["result"]["eska"]["target"],
            first["result"]["eska"]["target"]
        );
        let trace = vec![first, location, restored];
        if let Some(previous) = &previous {
            assert_eq!(&trace, previous);
        }
        previous = Some(trace);
        assert_eq!(fs::read_to_string(&path).unwrap(), BODY);
        client.finish();
    }
}

/// Unresolved names return a coded incomplete result until all visible inputs are actually ready.
#[test]
fn definition_reports_incomplete_without_loading_unrelated_metadata_or_guessing_xml() {
    let (_directory, root) = support::project(BODY, false);
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        client.ask(&initialize());
        client.send(&notification("initialized"));
        let mut query = pull(&root, "0");
        query["params"]["position"] = json!({"line":0,"character":0});
        let incomplete = client.ask(&query);
        assert_eq!(incomplete["error"]["data"]["kind"], "navigation_incomplete");
        assert_eq!(
            incomplete["error"]["data"]["details"]["snapshot"],
            snapshot("1")
        );
        assert_eq!(
            incomplete["error"]["data"]["details"]["resources"]["metadata_source_bytes"],
            0
        );
        for (revision, source) in [
            ("1", "configuration.yaml"),
            ("2", "CommonModules/Tools/object.yaml"),
        ] {
            assert!(
                client
                    .ask(&request(
                        json!("metadata"),
                        "eska/metadata/read",
                        json!({"snapshot":snapshot(revision),"source":source})
                    ))
                    .get("result")
                    .is_some()
            );
        }
        query["params"]["eska"]["snapshot"] = snapshot("3");
        assert_eq!(client.ask(&query)["result"], Value::Null);
        query["params"]["position"] = json!({"line":0,"character":6});
        assert_eq!(
            client.ask(&query)["error"]["data"]["kind"],
            "position_invalid"
        );
        query["params"]["eska"]["snapshot"] = snapshot("1");
        assert_eq!(
            client.ask(&query)["error"]["data"]["kind"],
            "stale_revision"
        );
        client.finish();
    }
}

/// Stop controls refuse before lazy reading and preserve metadata access and current revisions.
#[test]
fn definition_zero_and_overflow_do_not_read_input_or_request_automatic_retrigger() {
    let (_directory, root) = support::project(BODY, false);
    for lang in ["en-US", "ru-RU"] {
        for (timeout, expected) in [
            ("0", "analysis_deadline_exceeded"),
            ("18446744073709551615", "analysis_timeout_out_of_range"),
        ] {
            let mut client = Client::with_options(&root, lang, &["--timeout-seconds", timeout]);
            client.ask(&initialize());
            client.send(&notification("initialized"));
            let error = client.ask(&pull(&root, "0"));
            assert_eq!(error["error"]["code"], -32000);
            assert_eq!(error["error"]["data"]["kind"], expected);
            assert_eq!(error["error"]["data"]["details"], snapshot("0"));
            assert!(error["error"]["data"].get("retriggerRequest").is_none());
            let owners = client.ask(&request(
                json!(3),
                "eska/metadata/owners",
                json!({"snapshot":snapshot("0"),"limit":1}),
            ));
            assert_eq!(owners["result"]["snapshot"], snapshot("0"));
            assert_eq!(owners["result"]["resources"]["module_text_bytes"], 0);
            client.finish();
        }
    }
}

/// Cross-file ranges use the target's bytes; invalid current YAML clears access to an old model.
#[test]
fn cross_module_definition_tracks_target_bsl_and_current_metadata_authority() {
    let directory = support::TestDirectory::new();
    let caller = "Процедура Вызов()\nTools.Цель();\nКонецПроцедуры\n";
    let root = support::inline_command_project(&directory.0, caller);
    let source = root.join("src-with-command");
    let owner = "CommonModules/Tools/object.yaml";
    let path = "Catalogs/Sample/Commands/Run/Ext/CommandModule.bsl";
    let owner_uri = url::Url::from_file_path(source.join(owner)).unwrap();
    let target_uri = url::Url::from_file_path(source.join(MODULE)).unwrap();
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        client.ask(&initialize());
        client.send(&notification("initialized"));
        client.ask(&request(
            json!("owner"),
            "eska/metadata/read",
            json!({"snapshot":snapshot("0"),"source":owner}),
        ));
        let mut query = request(
            json!("cross"),
            "textDocument/definition",
            json!({
            "textDocument":{"uri":url::Url::from_file_path(source.join(path)).unwrap().as_str()},
            "position":{"line":1,"character":8},"eska":{"snapshot":snapshot("1")}}),
        );
        assert_eq!(
            client.ask(&query)["error"]["data"]["kind"],
            "navigation_incomplete"
        );
        client.ask(
            &json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":target_uri.as_str(),"version":1,"languageId":"bsl",
                "text":"\u{feff}// Я😀\r\nПроцедура Цель() Экспорт\r\nКонецПроцедуры\r\n"}}}),
        );
        query["params"]["eska"]["snapshot"] = snapshot("3");
        let location = client.ask(&query);
        assert_eq!(location["result"]["uri"], target_uri.as_str());
        assert_eq!(
            location["result"]["range"]["start"],
            json!({"line":1,"character":0})
        );
        assert_eq!(location["result"]["eska"]["source"], path);
        assert_eq!(location["result"]["eska"]["target"]["source"], MODULE);
        assert_ne!(
            location["result"]["eska"]["source_sha256"],
            location["result"]["eska"]["target"]["source_sha256"]
        );
        let invalid = client.ask(
            &json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":owner_uri.as_str(),"version":1,"languageId":"yaml","text":"{"}}}),
        );
        assert_eq!(invalid["method"], "eska/inputChanged");
        assert_eq!(
            invalid["params"]["input"]["state"]["input"]["kind"],
            "unavailable"
        );
        query["params"]["eska"]["snapshot"] = snapshot("4");
        assert_eq!(
            client.ask(&query)["error"]["data"]["kind"],
            "navigation_incomplete"
        );
        client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":owner_uri.as_str()}}}));
        query["params"]["eska"]["snapshot"] = snapshot("5");
        assert_eq!(
            client.ask(&query)["result"]["eska"]["target"],
            location["result"]["eska"]["target"]
        );
        assert_eq!(fs::read_to_string(source.join(path)).unwrap(), caller);
        assert_eq!(
            fs::read_to_string(source.join(MODULE)).unwrap(),
            "// A complete common module.\n"
        );
        client.finish();
    }
}
