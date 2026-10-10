//! BSL collection members navigate to the current physical YAML, with explicit input authority.

use super::*;

const OWNER: &str = "Catalogs/Sample/object.yaml";
const UUID: &str = "30000000-0000-4000-8000-000000000001";
const CALLER: &str = "Catalogs/Sample/Commands/Run/Ext/CommandModule.bsl";
const BODY: &str = "\u{feff}// Я😀\r\nПроцедура Тест()\r\nОбъект=СПРАВОЧНИКИ.sAmPlE.СоздатьЭлемент();\r\nКонецПроцедуры\r\n";

/// Select a byte inside the actual metadata identifier and derive its independent UTF-16 cursor.
fn definition(source: &Path, body: &str, name: &str, revision: &Value) -> Value {
    let at = body.find(name).unwrap() + name.chars().next().unwrap().len_utf8();
    let before = &body[..at];
    let line = before.bytes().filter(|&byte| byte == b'\n').count();
    let start = before.rfind('\n').map_or(0, |index| index + 1);
    request(
        json!("цель"),
        "textDocument/definition",
        json!({
        "textDocument":{"uri":url::Url::from_file_path(source.join(CALLER)).unwrap().as_str()},
        "position":{"line":line,"character":before[start..].encode_utf16().count()},
        "eska":{"snapshot":revision}}),
    )
}

/// The standard definition and direct UUID navigation must agree on the same ready owner.
fn confirmed(client: &mut Client, source: &Path, body: &str, revision: &Value) -> Value {
    let location = client.ask(&request(
        json!("uuid"),
        "eska/metadata/locate",
        json!({"snapshot":revision,"source":OWNER,"uuid":UUID}),
    ));
    assert!(location.get("result").is_some(), "{location}");
    let response = client.ask(&definition(source, body, "sAmPlE", revision));
    let mut boundary = definition(source, body, "sAmPlE", revision);
    let character = boundary["params"]["position"]["character"]
        .as_u64()
        .unwrap();
    boundary["params"]["position"]["character"] = json!(character + 5);
    let at_end = client.ask(&boundary);
    assert_eq!(at_end["result"], response["result"]);
    let result = &response["result"];
    let direct = &location["result"];
    assert_eq!(result["uri"], direct["location"]["uri"]);
    assert_eq!(result["range"], direct["location"]["range"]);
    assert_eq!(result["eska"]["target"]["kind"], "metadata");
    assert_eq!(result["eska"]["target"]["source"], OWNER);
    for key in ["uuid", "native_path", "source_sha256", "origin"] {
        assert_eq!(result["eska"]["target"][key], direct[key]);
    }
    assert_eq!(
        result["eska"]["target"]["start_byte"],
        direct["byte_range"]["start"]
    );
    assert_eq!(
        result["eska"]["target"]["end_byte"],
        direct["byte_range"]["end"]
    );
    assert_eq!(result["eska"]["resources"], direct["resources"]);
    assert_eq!(result["eska"]["snapshot"], direct["snapshot"]);
    assert_eq!(result["eska"]["coverage"]["semantic_complete"], false);
    response
}

/// Saved/editor/invalid/close states cannot publish old YAML coordinates or trigger a lazy owner read.
#[test]
fn definitions_follow_current_yaml_and_clear_invalid_targets_in_both_locales() {
    let directory = support::TestDirectory::new();
    let root = support::inline_command_project(&directory.0, BODY);
    let source = root.join("src-with-command");
    let file = source.join(OWNER);
    let saved = fs::read_to_string(&file).unwrap();
    let uri = url::Url::from_file_path(&file).unwrap();
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        let init = client.ask(&initialize());
        assert_eq!(
            init["result"]["capabilities"]["experimental"]["eska"]["definition"]["metadataTargets"],
            "typed-objects-and-members"
        );
        client.send(&notification("initialized"));
        let pending = client.ask(&definition(&source, BODY, "sAmPlE", &snapshot("0")));
        assert_eq!(pending["error"]["data"]["kind"], "navigation_incomplete");
        assert_eq!(
            pending["error"]["data"]["details"]["resources"]["metadata_source_bytes"],
            0
        );
        let read = client.ask(&request(
            json!("owner"),
            "eska/metadata/read",
            json!({"snapshot":&snapshot("1"),"source":OWNER}),
        ));
        assert!(read.get("result").is_some(), "{read}");
        let first = confirmed(&mut client, &source, BODY, &snapshot("2"));
        let draft = format!(
            "# Я😀 sAmPlE is only a comment\r\n{}",
            saved.replace('\n', "\r\n")
        );
        let opened = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri.as_str(),"version":7,"languageId":"eska-source","text":draft}}}));
        let editor = confirmed(&mut client, &source, BODY, &snapshot("3"));
        assert_ne!(editor["result"]["range"], first["result"]["range"]);
        assert_eq!(
            editor["result"]["eska"]["target"]["origin"],
            json!({"kind":"overlay","document_version":7})
        );
        assert_eq!(
            editor["result"]["eska"]["source_sha256"],
            first["result"]["eska"]["source_sha256"]
        );
        assert_eq!(
            editor["result"]["eska"]["resources"],
            opened["params"]["resources"]
        );
        fs::remove_file(&file).unwrap();
        assert_eq!(
            confirmed(&mut client, &source, BODY, &snapshot("3")),
            editor
        );
        fs::write(&file, &saved).unwrap();
        let invalid = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
            "textDocument":{"uri":uri.as_str(),"version":8},"contentChanges":[{"text":"broken: [\n"}]}}));
        assert_eq!(invalid["method"], "eska/inputChanged");
        let missing = client.ask(&definition(&source, BODY, "sAmPlE", &snapshot("4")));
        assert_eq!(missing["error"]["data"]["kind"], "navigation_incomplete");
        assert!(missing.get("result").is_none());
        client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri.as_str()}}}));
        let restored = confirmed(&mut client, &source, BODY, &snapshot("5"));
        assert_eq!(
            restored["result"]["eska"]["target"],
            first["result"]["eska"]["target"]
        );
        let stale = client.ask(&definition(&source, BODY, "sAmPlE", &snapshot("2")));
        assert_eq!(stale["error"]["data"]["kind"], "stale_revision");
        let trace = [
            pending, read, first, editor, invalid, missing, restored, stale,
        ];
        if let Some(previous) = &previous {
            assert_eq!(&trace, previous);
        }
        previous = Some(trace);
        assert_eq!(fs::read_to_string(&file).unwrap(), saved);
        assert_eq!(fs::read_to_string(source.join(CALLER)).unwrap(), BODY);
        client.finish();
    }
}

/// Declared locals and parameters keep their identity; inferred aliases use the actual collection.
#[test]
fn definitions_respect_shadowing_and_collection_aliases_in_both_locales() {
    let directory = support::TestDirectory::new();
    let root = support::inline_command_project(&directory.0, BODY);
    let source = root.join("src-with-command");
    let uri = url::Url::from_file_path(source.join(CALLER)).unwrap();
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        client.ask(&initialize());
        client.send(&notification("initialized"));
        client.ask(&request(
            json!("owner"),
            "eska/metadata/read",
            json!({"snapshot":&snapshot("0"),"source":OWNER}),
        ));
        let mut trace = Vec::new();
        for (version, body, ready) in [
            (
                1,
                "Процедура Тест()\nПерем Справочники;\nСправочники=Новый Структура;\nЗначение=Справочники.sAmPlE;\nКонецПроцедуры\n",
                false,
            ),
            (
                2,
                "Процедура Тест(Справочники)\nЗначение=Справочники.sAmPlE;\nКонецПроцедуры\n",
                false,
            ),
            (
                3,
                "Перем Справочники;\nПроцедура Тест()\nЗначение=Справочники.sAmPlE;\nКонецПроцедуры\n",
                false,
            ),
            (
                4,
                "Процедура Тест()\nКоллекция=Справочники;\nЗначение=Коллекция.sAmPlE;\nКонецПроцедуры\n",
                true,
            ),
            (
                5,
                "Процедура Тест()\nПерем Справочники;\nСправочники=Catalogs;\nЗначение=Справочники.sAmPlE;\nКонецПроцедуры\n",
                true,
            ),
        ] {
            let event = if version == 1 {
                client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                    "textDocument":{"uri":uri.as_str(),"version":version,"languageId":"bsl","text":body}}}))
            } else {
                client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
                    "textDocument":{"uri":uri.as_str(),"version":version},"contentChanges":[{"text":body}]}}))
            };
            let revision = event["params"]["snapshot"].clone();
            let result = if ready {
                confirmed(&mut client, &source, body, &revision)
            } else {
                let result = client.ask(&definition(&source, body, "sAmPlE", &revision));
                assert_eq!(
                    result["error"]["data"]["kind"], "navigation_incomplete",
                    "{body}: {result}"
                );
                result
            };
            trace.push(result);
        }
        if let Some(previous) = &previous {
            assert_eq!(&trace, previous);
        }
        previous = Some(trace);
        assert_eq!(fs::read_to_string(source.join(CALLER)).unwrap(), BODY);
        client.finish();
    }
}
