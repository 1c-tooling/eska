//! Module namespace navigation shares current physical YAML authority with direct UUID locations.

use std::{fs, path::Path};

use serde_json::{Value, json};
use url::Url;

use super::{Client, initialize, notification, request, snapshot, support};

const OWNER: &str = "CommonModules/Tools/object.yaml";
const UUID: &str = "20000000-0000-4000-8000-000000000001";
const CALLER: &str = "Catalogs/Sample/Commands/Run/Ext/CommandModule.bsl";
const BODY: &str = "\u{feff}// Я😀\r\nПроцедура Тест()\r\nToOlS.Цель();\r\nКонецПроцедуры\r\n";

/// The selected occurrence's byte offset is converted independently to LSP UTF-16.
fn definition(source: &Path, body: &str, name: &str, revision: &Value) -> Value {
    let at = body.rfind(name).unwrap() + name.chars().next().unwrap().len_utf8();
    let before = &body[..at];
    let line = before.bytes().filter(|&byte| byte == b'\n').count();
    let start = before.rfind('\n').map_or(0, |at| at + 1);
    request(
        json!("definition"),
        "textDocument/definition",
        json!({"textDocument":{"uri":Url::from_file_path(source.join(CALLER)).unwrap().as_str()},"position":{"line":line,"character":before[start..].encode_utf16().count()},"eska":{"snapshot":revision}}),
    )
}

/// Open/change acknowledgements publish full current bytes through the ordinary document controller.
fn edit(client: &mut Client, uri: &Url, version: i32, text: &str, open: bool) -> Value {
    let language = if Path::new(uri.path())
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("bsl"))
    {
        "bsl"
    } else {
        "yaml"
    };
    let message = if open {
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri.as_str(),"version":version,"languageId":language,"text":text}}})
    } else {
        json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri.as_str(),"version":version},"contentChanges":[{"text":text}]}})
    };
    let event = client.ask(&message);
    assert_eq!(event["method"], "eska/inputChanged");
    event["params"]["snapshot"].clone()
}

/// A namespace and direct metadata locate must return identical UUID, ranges, hashes and resource coverage.
fn confirmed(client: &mut Client, source: &Path, body: &str, revision: &Value) -> Value {
    let direct = client.ask(&request(
        json!("locate"),
        "eska/metadata/locate",
        json!({"snapshot":revision,"source":OWNER,"uuid":UUID}),
    ))["result"]
        .clone();
    let response = client.ask(&definition(source, body, "ToOlS", revision));
    let location = &response["result"];
    assert!(response.get("result").is_some(), "{response}");
    assert_eq!(location["uri"], direct["location"]["uri"]);
    assert_eq!(location["range"], direct["location"]["range"]);
    assert_eq!(location["eska"]["target"]["kind"], "metadata");
    assert_eq!(location["eska"]["target"]["source"], OWNER);
    for key in ["uuid", "native_path", "source_sha256", "origin"] {
        assert_eq!(location["eska"]["target"][key], direct[key]);
    }
    assert_eq!(
        location["eska"]["target"]["start_byte"],
        direct["byte_range"]["start"]
    );
    assert_eq!(
        location["eska"]["target"]["end_byte"],
        direct["byte_range"]["end"]
    );
    assert_eq!(location["eska"]["resources"], direct["resources"]);
    assert_eq!(location["eska"]["snapshot"], *revision);
    assert_eq!(location["eska"]["coverage"]["semantic_complete"], false);
    assert_eq!(
        location["eska"]["resources"]["module_text_bytes"],
        u64::try_from(body.len()).unwrap()
    );
    response
}

/// Ready YAML can be navigated while its BSL body is unread; overlay/invalid/close share the same snapshot.
#[test]
fn module_namespaces_follow_current_yaml_without_loading_target_bsl_in_both_locales() {
    let directory = support::TestDirectory::new();
    let root = support::inline_command_project(&directory.0, BODY);
    let source = root.join("src-with-command");
    let file = source.join(OWNER);
    let saved = fs::read_to_string(&file).unwrap();
    let uri = Url::from_file_path(&file).unwrap();
    let editor = format!("# Я😀\r\n{}", saved.replace('\n', "\r\n"));
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        client.ask(&initialize());
        client.send(&notification("initialized"));
        let pending = client.ask(&definition(&source, BODY, "ToOlS", &snapshot("0")));
        assert_eq!(pending["error"]["data"]["kind"], "navigation_incomplete");
        assert_eq!(
            pending["error"]["data"]["details"]["resources"]["metadata_source_bytes"],
            0
        );
        client.ask(&request(
            json!("read"),
            "eska/metadata/read",
            json!({"snapshot":snapshot("1"),"source":OWNER}),
        ));
        let mut trace = vec![
            pending,
            confirmed(&mut client, &source, BODY, &snapshot("2")),
        ];
        let revision = edit(&mut client, &uri, 7, &editor, true);
        fs::remove_file(&file).unwrap();
        let current = confirmed(&mut client, &source, BODY, &revision);
        assert_ne!(current["result"]["range"], trace[1]["result"]["range"]);
        trace.push(current);
        let revision = edit(&mut client, &uri, 8, "broken: [\n", false);
        let invalid = client.ask(&definition(&source, BODY, "ToOlS", &revision));
        assert_eq!(invalid["error"]["data"]["kind"], "navigation_incomplete");
        trace.push(invalid);
        fs::write(&file, &saved).unwrap();
        let closed=client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri.as_str()}}}));
        trace.push(confirmed(
            &mut client,
            &source,
            BODY,
            &closed["params"]["snapshot"],
        ));
        let stale = client.ask(&definition(&source, BODY, "ToOlS", &snapshot("2")));
        assert_eq!(stale["error"]["code"], -32801);
        trace.push(stale);
        if let Some(previous) = &previous {
            assert_eq!(&trace, previous);
        }
        previous = Some(trace);
        client.finish();
    }
    assert_eq!(fs::read_to_string(file).unwrap(), saved);
}

/// Explicit declarations, methods and aliases keep their existing BSL target despite a ready same-named module.
#[test]
fn shadowed_and_aliased_namespace_names_keep_bsl_ownership_in_both_locales() {
    let directory = support::TestDirectory::new();
    let root = support::inline_command_project(&directory.0, BODY);
    let source = root.join("src-with-command");
    let uri = Url::from_file_path(source.join(CALLER)).unwrap();
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        client.ask(&initialize());
        client.send(&notification("initialized"));
        client.ask(&request(
            json!("read"),
            "eska/metadata/read",
            json!({"snapshot":snapshot("0"),"source":OWNER}),
        ));
        let mut trace = Vec::new();
        for (version,(body,name)) in [
            ("Процедура Тест()\nПерем Tools;\nTools=Новый Структура;\nTools.Цель();\nКонецПроцедуры\n","Tools"),
            ("Процедура Тест(Tools)\nTools.Цель();\nКонецПроцедуры\n","Tools"),
            ("Процедура Tools()\nКонецПроцедуры\nTools();\n","Tools"),
            ("Процедура Тест()\nToolsAlias=Tools;\nToolsAlias.Цель();\nКонецПроцедуры\n","ToolsAlias"),
        ].into_iter().enumerate() {
            let revision=edit(&mut client,&uri,i32::try_from(version+1).unwrap(),body,version==0);
            let result=client.ask(&definition(&source,body,name,&revision));assert!(result.get("result").is_some(),"{result}");
            assert_eq!(result["result"]["eska"]["target"]["source"],CALLER);
            assert!(result["result"]["eska"]["target"]["kind"].is_null());trace.push(result);
        }
        if let Some(previous) = &previous {
            assert_eq!(&trace, previous);
        }
        previous = Some(trace);
        client.finish();
    }
}
