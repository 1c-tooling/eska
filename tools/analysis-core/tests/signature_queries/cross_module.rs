//! The caller's signature uses current target declarations and canonical common-module identity.

use super::*;

const CALLER: &str = "Catalogs/Sample/Commands/Run/Ext/CommandModule.bsl";
const CALLER_BODY: &str = "Процедура Вызов()\r\nTools.Значение(1, );\r\nКонецПроцедуры\r\n";
const TARGET_BODY: &str = "// Другой модуль.\r\nФункция Значение(Первый, Второй = 0) Экспорт\r\nВозврат Первый;\r\nКонецФункции\r\n";

/// Renaming then invalidating one current owner must never reuse its previous common-module model.
fn refuse_owner(
    client: &mut Client,
    root: &Path,
    uri: &url::Url,
    renamed: &str,
    at: usize,
) -> Vec<Value> {
    let mut refused = Vec::new();
    for (version, text, revision) in [(1, renamed, "6"), (2, "invalid: [", "7")] {
        let update = if version == 1 {
            json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument":{"uri":uri.as_str(),"version":version,"languageId":"yaml","text":text}}})
        } else {
            json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
                "textDocument":{"uri":uri.as_str(),"version":version},"contentChanges":[{"text":text}]}})
        };
        assert_eq!(
            client.ask(&update)["params"]["snapshot"],
            snapshot(revision)
        );
        let missing = client.ask(&pull(
            root,
            CALLER,
            "src-with-command",
            revision,
            position(CALLER_BODY, at),
        ));
        assert_eq!(
            missing["error"]["data"]["kind"],
            "signature_help_incomplete"
        );
        refused.push(missing);
    }
    refused
}

/// Target changes update signatures without replacing the caller's hash or reading old XML.
#[test]
fn cross_module_signatures_follow_current_target_and_refuse_renamed_or_invalid_metadata() {
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
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        client.ask(&initialize_signature("plaintext", false));
        client.send(&notification("initialized"));
        let at = CALLER_BODY.find(", );").unwrap() + 2;
        let first = client.ask(&pull(
            &root,
            CALLER,
            "src-with-command",
            "0",
            position(CALLER_BODY, at),
        ));
        verify(&first["result"], CALLER_BODY, CALLER, "4", at, false);
        assert_eq!(first["result"]["activeParameter"], 1);
        assert!(
            first["result"]["signatures"][0]["label"]
                .as_str()
                .unwrap()
                .contains("Второй")
        );
        let changed = TARGET_BODY.replace("Второй =", "Актуальный =");
        client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":target_uri.as_str(),"version":1,"languageId":"bsl","text":changed}}}));
        let current = client.ask(&pull(
            &root,
            CALLER,
            "src-with-command",
            "5",
            position(CALLER_BODY, at),
        ));
        verify(&current["result"], CALLER_BODY, CALLER, "5", at, false);
        assert!(
            current["result"]["signatures"][0]["label"]
                .as_str()
                .unwrap()
                .contains("Актуальный")
        );
        let refused = refuse_owner(&mut client, &root, &owner_uri, &renamed, at);
        client.ask(
            &json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{
            "textDocument":{"uri":owner_uri.as_str()}}}),
        );
        let restored = client.ask(&pull(
            &root,
            CALLER,
            "src-with-command",
            "8",
            position(CALLER_BODY, at),
        ));
        assert_eq!(
            restored["result"]["signatures"],
            current["result"]["signatures"]
        );
        let trace = (first, current, refused, restored);
        if let Some(previous) = &previous {
            assert_eq!(&trace, previous);
        }
        previous = Some(trace);
        client.finish();
    }
    assert_eq!(fs::read_to_string(target_path).unwrap(), TARGET_BODY);
    assert_eq!(fs::read_to_string(owner_path).unwrap(), owner_text);
}
