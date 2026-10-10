//! Actual stdio outlines verify current coordinates, negotiated shape and read-only input controls.

use sha2::{Digest, Sha256};
use std::fmt::Write as _;

use super::*;

const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";
const BODY: &str = "\u{feff}// Я😀\r\n#Область Интерфейс\r\nПерем Состояние Экспорт;\r\n#Область Вычисления\r\nФункция Значение(Параметр = 1) Экспорт\r\nВозврат Параметр;\r\nКонецФункции\r\n#КонецОбласти\r\nПроцедура Вызов()\r\n#Область Локальная\r\n#КонецОбласти\r\nКонецПроцедуры\r\n#КонецОбласти\r\n";

/// Hash verbatim bytes independently of the server's source-input implementation.
fn hash(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .fold(String::new(), |mut hash, byte| {
            write!(hash, "{byte:02x}").unwrap();
            hash
        })
}

/// Optional ESKA guards pin a declared local module before lazy reading starts.
fn pull(root: &Path, module: &str, revision: &str) -> Value {
    request(
        json!("структура"),
        "textDocument/documentSymbol",
        json!({
        "textDocument":{"uri":url::Url::from_file_path(root.join("src").join(module)).unwrap().as_str()},
        "eska":{"snapshot":snapshot(revision)},"workDoneToken":1,"partialResultToken":"accepted"}),
    )
}

/// Enable hierarchy only through the standard client option.
fn initialize_symbols(hierarchy: &Value) -> Value {
    let mut value = initialize();
    value["params"]["capabilities"] = json!({"textDocument":{"documentSymbol":{
        "hierarchicalDocumentSymbolSupport":hierarchy,"symbolKind":{}}}});
    value
}

/// Each node's selection is independently checked against current bytes and its UTF-16 position.
fn check_symbols(items: &Value, text: &str, revision: &str, hierarchical: bool) {
    for item in items.as_array().unwrap() {
        let context = &item["eska"];
        assert_eq!(context["snapshot"], snapshot(revision));
        assert_eq!(context["profile"], "structure");
        assert_eq!(context["source"], MODULE);
        assert_eq!(context["source_sha256"], hash(text));
        assert_eq!(context["coverage"]["semantic_complete"], false);
        assert_eq!(context["resources"]["metadata_source_bytes"], 0);
        let start = usize::try_from(context["selection_start_byte"].as_u64().unwrap()).unwrap();
        let end = usize::try_from(context["selection_end_byte"].as_u64().unwrap()).unwrap();
        assert_eq!(&text[start..end], item["name"].as_str().unwrap());
        let before = &text[..start];
        let line = before.bytes().filter(|byte| *byte == b'\n').count();
        let column = before.rsplit('\n').next().unwrap().encode_utf16().count();
        let selection = if hierarchical {
            &item["selectionRange"]
        } else {
            &item["location"]["range"]
        };
        assert_eq!(selection["start"], json!({"line":line,"character":column}));
        assert_eq!(
            selection["end"]["character"],
            column + text[start..end].encode_utf16().count()
        );
        if hierarchical && item.get("children").is_some() {
            check_symbols(&item["children"], text, revision, true);
        }
    }
}

/// A nested outline follows editor bytes and close, with byte-identical machine replies in RU/EN.
#[test]
fn hierarchy_tracks_current_bsl_and_preserves_saved_bytes_in_both_locales() {
    let (_directory, root) = support::project(BODY, false);
    let path = root.join("src").join(MODULE);
    let uri = url::Url::from_file_path(&path).unwrap();
    let changed = format!(
        "\u{feff}// Current Я😀\r\n{}",
        BODY.trim_start_matches('\u{feff}')
    )
    .replace("Значение", "НовоеЗначение");
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        let initialized = client.ask(&initialize_symbols(&json!(true)));
        assert_eq!(
            initialized["result"]["capabilities"]["documentSymbolProvider"],
            true
        );
        assert_eq!(
            initialized["result"]["capabilities"]["experimental"]["eska"]["documentSymbols"]["result"],
            "hierarchical"
        );
        client.send(&notification("initialized"));
        let first = client.ask(&pull(&root, MODULE, "0"));
        check_symbols(&first["result"], BODY, "1", true);
        let children = &first["result"][0]["children"];
        assert_eq!(first["result"][0]["name"], "Интерфейс");
        assert_eq!(children[0]["name"], "Состояние");
        assert_eq!(children[1]["children"][0]["name"], "Значение");
        assert_eq!(children[2]["name"], "Вызов");
        assert!(children[2].get("children").is_none());
        client.ask(
            &json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri.as_str(),"version":1,"languageId":"bsl","text":changed}}}),
        );
        let stale = client.ask(&pull(&root, MODULE, "1"));
        assert_eq!(stale["error"]["data"]["kind"], "stale_revision");
        let current = client.ask(&pull(&root, MODULE, "2"));
        check_symbols(&current["result"], &changed, "2", true);
        client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri.as_str()}}}));
        let closed = client.ask(&pull(&root, MODULE, "3"));
        check_symbols(&closed["result"], BODY, "3", true);
        let trace = vec![first, stale, current, closed];
        if let Some(previous) = &previous {
            assert_eq!(&trace, previous);
        }
        previous = Some(trace);
        client.finish();
    }
    assert_eq!(fs::read_to_string(path).unwrap(), BODY);
}

/// Missing/false/malformed hierarchy capability yields one ordered flat list with real containers.
#[test]
fn flat_clients_receive_symbol_information_without_hierarchy_fields() {
    let (_directory, root) = support::project(BODY, false);
    let uri = url::Url::from_file_path(root.join("src").join(MODULE)).unwrap();
    for lang in ["en-US", "ru-RU"] {
        let mut previous = None;
        for hierarchy in [Value::Null, json!(false), json!("true")] {
            let mut client = Client::with_options(&root, lang, &[]);
            let initialized = client.ask(&initialize_symbols(&hierarchy));
            assert_eq!(
                initialized["result"]["capabilities"]["experimental"]["eska"]["documentSymbols"]["result"],
                "flat"
            );
            client.send(&notification("initialized"));
            let result = client.ask(&pull(&root, MODULE, "0"));
            let items = result["result"].as_array().unwrap();
            assert_eq!(
                items
                    .iter()
                    .map(|item| item["name"].as_str().unwrap())
                    .collect::<Vec<_>>(),
                ["Интерфейс", "Состояние", "Вычисления", "Значение", "Вызов"]
            );
            assert_eq!(
                items
                    .iter()
                    .map(|item| item["kind"].as_u64().unwrap())
                    .collect::<Vec<_>>(),
                [3, 13, 3, 12, 12]
            );
            assert_eq!(items[3]["containerName"], "Вычисления");
            assert_eq!(items[4]["eska"]["kind"], "procedure");
            for item in items {
                assert!(
                    item.get("range").is_none()
                        && item.get("selectionRange").is_none()
                        && item.get("children").is_none()
                );
                assert_eq!(item["location"]["uri"], uri.as_str());
            }
            check_symbols(&result["result"], BODY, "1", false);
            if let Some(previous) = &previous {
                assert_eq!(&result, previous);
            }
            previous = Some(result);
            client.finish();
        }
    }
}

/// A ready empty module succeeds independently of pending metadata; missing/binary/quota never do.
#[test]
fn unavailable_sources_do_not_become_empty_successful_outlines() {
    let (_directory, root) = support::project("", false);
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        client.ask(&initialize());
        client.send(&notification("initialized"));
        assert_eq!(client.ask(&pull(&root, MODULE, "0"))["result"], json!([]));
        let unknown = client.ask(&pull(&root, "CommonModules/Unknown/Ext/Module.bsl", "1"));
        assert_eq!(unknown["error"]["data"]["kind"], "unknown_source");
        client.finish();
    }
    for (binary, missing, options) in [
        (true, false, vec![]),
        (false, true, vec![]),
        (false, false, vec!["--max-retained-source-bytes", "0"]),
    ] {
        let (_directory, root) = support::project(BODY, binary);
        let module = if binary {
            "CommonModules/Tools/Ext/Module.bin"
        } else {
            MODULE
        };
        if missing {
            fs::remove_file(root.join("src").join(module)).unwrap();
        }
        for lang in ["en-US", "ru-RU"] {
            let mut client = Client::with_options(&root, lang, &options);
            client.ask(&initialize());
            client.send(&notification("initialized"));
            let result = client.ask(&pull(&root, module, "0"));
            assert!(result.get("result").is_none(), "{result}");
            assert_eq!(result["error"]["data"]["kind"], "module_unavailable");
            client.finish();
        }
    }
}

/// Deadline and snapshot/URI guards run before lazy IO, preserving the unread source generation.
#[test]
fn symbols_preserve_preflight_and_timeout_contract_in_both_locales() {
    let (_directory, root) = support::project(BODY, false);
    for (timeout, expected) in [
        ("0", "analysis_deadline_exceeded"),
        ("18446744073709551615", "analysis_timeout_out_of_range"),
    ] {
        for lang in ["en-US", "ru-RU"] {
            let mut client = Client::with_options(&root, lang, &["--timeout-seconds", timeout]);
            client.ask(&initialize());
            client.send(&notification("initialized"));
            let mut stale = pull(&root, MODULE, "5");
            assert_eq!(
                client.ask(&stale)["error"]["data"]["kind"],
                "stale_revision"
            );
            stale["params"]["eska"]["snapshot"] = json!({"session_id":"other","revision":"0"});
            assert_eq!(
                client.ask(&stale)["error"]["data"]["kind"],
                "unknown_session"
            );
            let result = client.ask(&pull(&root, MODULE, "0"));
            assert_eq!(result["error"]["code"], -32000);
            assert_eq!(result["error"]["data"]["kind"], expected);
            assert_eq!(result["error"]["data"]["details"], snapshot("0"));
            assert!(result["error"]["data"].get("retriggerRequest").is_none());
            let mut owner = pull(&root, "CommonModules/Tools/object.yaml", "0");
            assert_eq!(
                client.ask(&owner)["error"]["data"]["kind"],
                "source_not_module"
            );
            owner["params"]["textDocument"]["uri"] = json!("https://example.invalid/module.bsl");
            assert_eq!(
                client.ask(&owner)["error"]["data"]["kind"],
                "document_uri_invalid"
            );
            let owners = client.ask(&request(
                json!("state"),
                "eska/metadata/owners",
                json!({"snapshot":snapshot("0"),"limit":1}),
            ));
            assert_eq!(owners["result"]["snapshot"], snapshot("0"));
            assert_eq!(owners["result"]["resources"]["module_text_bytes"], 0);
            client.finish();
        }
    }
}

/// Disabled syntax checks do not disable structure; invalid frozen settings give an explicit error.
#[test]
fn symbols_share_language_settings_without_claiming_semantic_completeness() {
    let (_directory, root) = support::project("Procedure Unfinished()\n", false);
    for (config, expected) in [
        ("[diagnostics]\nParseError=false\n", None),
        ("invalid= [", Some("analysis_config_invalid")),
    ] {
        fs::write(root.join("bsl-analyzer.toml"), config).unwrap();
        for lang in ["en-US", "ru-RU"] {
            let mut client = Client::with_options(&root, lang, &[]);
            let initialized = client.ask(&initialize());
            assert_eq!(
                initialized["result"]["capabilities"]
                    .get("documentSymbolProvider")
                    .is_some(),
                expected.is_none()
            );
            client.send(&notification("initialized"));
            let result = client.ask(&pull(&root, MODULE, "0"));
            if let Some(expected) = expected {
                assert_eq!(result["error"]["data"]["kind"], expected);
            } else {
                assert_eq!(result["result"][0]["name"], "Unfinished");
                assert_eq!(
                    result["result"][0]["eska"]["coverage"]["semantic_complete"],
                    false
                );
            }
            client.finish();
        }
    }
}
