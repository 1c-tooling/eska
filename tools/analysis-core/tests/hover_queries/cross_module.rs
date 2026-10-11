//! A caller's card uses current metadata and the current ready body of a different common module.

use super::*;

const CALLER: &str = "Catalogs/Sample/Commands/Run/Ext/CommandModule.bsl";
const CALLER_BODY: &str = "Процедура Вызов()\r\nTools.Значение();\r\nКонецПроцедуры\r\n";
const TARGET_BODY: &str =
    "// Карточка другого модуля.\r\nФункция Значение() Экспорт\r\nВозврат 1;\r\nКонецФункции\r\n";

/// Loading/changing a different module never substitutes its hash or range for the caller's token.
#[test]
fn cross_module_hover_uses_current_target_and_refuses_renamed_metadata_without_xml_fallback() {
    let directory = support::TestDirectory::new();
    let root = support::inline_command_project(&directory.0, CALLER_BODY);
    let source = root.join("src-with-command");
    let target_path = source.join(MODULE);
    let target_uri = url::Url::from_file_path(&target_path).unwrap();
    fs::write(&target_path, TARGET_BODY).unwrap();
    let owner = "CommonModules/Tools/object.yaml";
    let owner_path = source.join(owner);
    let owner_uri = url::Url::from_file_path(&owner_path).unwrap();
    let owner_text = fs::read_to_string(&owner_path).unwrap();
    let renamed = owner_text.replace("\"value\": \"Tools\"", "\"value\": \"Renamed\"");
    assert_ne!(renamed, owner_text);
    for (lang, number, string) in [("en-US", "Number", "String"), ("ru-RU", "Число", "Строка")]
    {
        let mut client = Client::with_options(&root, lang, &[]);
        client.ask(&initialize());
        client.send(&notification("initialized"));
        let at = CALLER_BODY.find("Значение").unwrap();
        let first = client.ask(&pull(
            &root,
            CALLER,
            "src-with-command",
            "0",
            position(CALLER_BODY, at),
        ));
        verify(&first["result"], CALLER_BODY, CALLER, "4", at, "Значение");
        assert!(
            first["result"]["contents"]["value"]
                .as_str()
                .unwrap()
                .contains(number)
        );
        let changed = TARGET_BODY.replace("Возврат 1;", "Возврат \"новый тип\";");
        client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":target_uri.as_str(),"version":1,"languageId":"bsl","text":changed}}}));
        let current = client.ask(&pull(
            &root,
            CALLER,
            "src-with-command",
            "5",
            position(CALLER_BODY, at),
        ));
        verify(&current["result"], CALLER_BODY, CALLER, "5", at, "Значение");
        assert!(
            current["result"]["contents"]["value"]
                .as_str()
                .unwrap()
                .contains(string)
        );
        client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":owner_uri.as_str(),"version":1,"languageId":"yaml","text":renamed}}}));
        let missing = client.ask(&pull(
            &root,
            CALLER,
            "src-with-command",
            "6",
            position(CALLER_BODY, at),
        ));
        assert_eq!(missing["error"]["data"]["kind"], "hover_incomplete");
        client.ask(
            &json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{
            "textDocument":{"uri":owner_uri.as_str()}}}),
        );
        let restored = client.ask(&pull(
            &root,
            CALLER,
            "src-with-command",
            "7",
            position(CALLER_BODY, at),
        ));
        assert_eq!(
            restored["result"]["contents"],
            current["result"]["contents"]
        );
        client.finish();
    }
    assert_eq!(fs::read_to_string(target_path).unwrap(), TARGET_BODY);
    assert_eq!(fs::read_to_string(owner_path).unwrap(), owner_text);
}
