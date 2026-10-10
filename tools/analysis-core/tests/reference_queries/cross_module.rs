//! Cross-module occurrences preserve distinct current target buffers and caller ownership.

use super::*;
const CALLER: &str = "Catalogs/Sample/Commands/Run/Ext/CommandModule.bsl";
const CALLER_BODY: &str = "Процедура Вызов()\r\nTools.Значение(1);\r\nКонецПроцедуры\r\n";
const TARGET_BODY: &str =
    "Функция Значение(Аргумент) Экспорт\r\nВозврат Аргумент;\r\nКонецФункции\r\n";

/// Native hits from two files retain their own hashes and follow target overlays and metadata changes.
#[test]
fn cross_module_references_follow_current_target_coordinates_and_metadata_identity() {
    let directory = support::TestDirectory::new();
    let root = support::inline_command_project(&directory.0, CALLER_BODY);
    let source = root.join("src-with-command");
    let target_path = source.join(MODULE);
    let uri = url::Url::from_file_path(&target_path).unwrap();
    fs::write(&target_path, TARGET_BODY).unwrap();
    let owner = "CommonModules/Tools/object.yaml";
    let owner_path = source.join(owner);
    let owner_uri = url::Url::from_file_path(&owner_path).unwrap();
    let original = fs::read_to_string(&owner_path).unwrap();
    let renamed = original.replace("\"value\": \"Tools\"", "\"value\": \"Renamed\"");
    assert_ne!(renamed, original);
    let at = CALLER_BODY.find("Значение").unwrap();
    let changed = format!("\u{feff}\r\n{TARGET_BODY}");
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        client.ask(&initialize());
        client.send(&notification("initialized"));
        client.ask(&request(
            json!("owner"),
            "eska/metadata/read",
            json!({"snapshot":snapshot("0"),"source":owner}),
        ));
        let unread = client.ask(&pull(
            &root,
            CALLER,
            "src-with-command",
            "1",
            position(CALLER_BODY, at),
            true,
        ));
        assert_eq!(unread["error"]["data"]["kind"], "references_incomplete");
        client.ask(&request(
            json!("target"),
            "textDocument/documentSymbol",
            json!({"textDocument":{"uri":uri.as_str()},"eska":{"snapshot":snapshot("2")}}),
        ));
        let first = client.ask(&pull(
            &root,
            CALLER,
            "src-with-command",
            "3",
            position(CALLER_BODY, at),
            true,
        ));
        check_targets(&first, TARGET_BODY, "3");
        client.ask(
            &json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri.as_str(),"version":1,"languageId":"bsl","text":changed}}}),
        );
        let current = client.ask(&pull(
            &root,
            CALLER,
            "src-with-command",
            "4",
            position(CALLER_BODY, at),
            true,
        ));
        check_targets(&current, &changed, "4");
        client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":owner_uri.as_str(),"version":1,"languageId":"yaml","text":renamed}}}));
        let missing = client.ask(&pull(
            &root,
            CALLER,
            "src-with-command",
            "5",
            position(CALLER_BODY, at),
            true,
        ));
        assert_eq!(missing["error"]["data"]["kind"], "references_incomplete");
        client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":owner_uri.as_str()}}}));
        let restored = client.ask(&pull(
            &root,
            CALLER,
            "src-with-command",
            "6",
            position(CALLER_BODY, at),
            true,
        ));
        check_targets(&restored, &changed, "6");
        let trace = vec![unread, first, current, missing, restored];
        if let Some(previous) = &previous {
            assert_eq!(&trace, previous);
        }
        previous = Some(trace);
        client.finish();
    }
    assert_eq!(fs::read_to_string(target_path).unwrap(), TARGET_BODY);
    assert_eq!(fs::read_to_string(owner_path).unwrap(), original);
}

/// Grouped rendering preserves each native occurrence's file, coordinates and caller context.
fn check_targets(reply: &Value, target: &str, revision: &str) {
    let hits = reply["result"].as_array().unwrap();
    assert_eq!(hits.len(), 2, "{reply}");
    for hit in hits {
        let declaration = hit["eska"]["kind"] == "declaration";
        let (text, module) = if declaration {
            (target, MODULE)
        } else {
            (CALLER_BODY, CALLER)
        };
        verify(hit, CALLER, CALLER_BODY, text, revision, "Значение");
        assert_eq!(hit["eska"]["target"]["source"], module);
        assert_eq!(hit["eska"]["search"]["files_scanned"], 2);
    }
}
