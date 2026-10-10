//! Framed requests preserve current YAML authority and refuse ambiguous union field targets.

use std::{fs, path::Path};

use serde_json::{Value, json};
use url::Url;

use super::{
    Client, definition_member_queries, initialize, notification, request, snapshot, support,
};

const CALLER: &str = "Catalogs/Sample/Commands/Run/Ext/CommandModule.bsl";
const OWNER: &str = "Catalogs/Sample/object.yaml";
const UUID: &str = "60000000-0000-4000-8000-000000000001";

/// A second owner declares the same field with a distinct UUID, so spelling cannot resolve ambiguity.
fn fixture(directory: &Path) -> std::path::PathBuf {
    let root = definition_member_queries::fixture(directory);
    let native = directory.join("native");
    let config = fs::read_to_string(native.join("Configuration.xml")).unwrap();
    fs::write(
        native.join("Configuration.xml"),
        config.replace(
            "<Catalog>Sample</Catalog>",
            "<Catalog>Sample</Catalog><Catalog>Other</Catalog>",
        ),
    )
    .unwrap();
    fs::write(native.join("Catalogs/Other.xml"),"<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" xmlns:v8=\"http://v8.1c.ru/8.1/data/core\" xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" version=\"2.20\"><Catalog uuid=\"90000000-0000-4000-8000-000000000001\"><Properties><Name>Other</Name></Properties><ChildObjects><Attribute uuid=\"90000000-0000-4000-8000-000000000002\"><Properties><Name>ExtraField</Name><Type><v8:Type>xs:string</v8:Type></Type></Properties></Attribute></ChildObjects></Catalog></MetaDataObject>").unwrap();
    eska::project_core::source_format::import_designer(&native, &root.join("src-with-unions"))
        .unwrap();
    let config = fs::read_to_string(root.join("eska.toml")).unwrap();
    fs::write(
        root.join("eska.toml"),
        config.replace("src-with-members", "src-with-unions"),
    )
    .unwrap();
    root
}

/// Only both involved owners are read; all unrelated module texts remain lazy.
fn start(root: &Path, lang: &str) -> Client {
    let mut client = Client::with_options(root, lang, &[]);
    client.ask(&initialize());
    client.send(&notification("initialized"));
    for (revision, source) in [("0", OWNER), ("1", "Catalogs/Other/object.yaml")] {
        let response = client.ask(&request(
            json!("read"),
            "eska/metadata/read",
            json!({"snapshot":snapshot(revision),"source":source}),
        ));
        assert!(response.get("result").is_some(), "{response}");
    }
    client
}

/// The kernel infers the return contract of a local function rather than guessing a variable's spelling.
fn body(types: &str, field: &str) -> String {
    format!(
        "\u{feff}// Я😀\r\n// Возвращаемое значение:\r\n//  {types} - Тест.\r\nФункция Данные()\r\nКонецФункции\r\nПроцедура Тест()\r\nОбъект=Данные();\r\nЗначение=Объект.{field};\r\nКонецПроцедуры\r\n"
    )
}

/// Full-sync edits return the one current revision, including malformed YAML.
fn edit(client: &mut Client, uri: &Url, text: &str, version: i32, open: bool) -> Value {
    let event = if open {
        let language = if Path::new(uri.path())
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("bsl"))
        {
            "bsl"
        } else {
            "yaml"
        };
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri.as_str(),"languageId":language,"version":version,"text":text}}})
    } else {
        json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri.as_str(),"version":version},"contentChanges":[{"text":text}]}})
    };
    let result = client.ask(&event);
    assert_eq!(result["method"], "eska/inputChanged");
    result["params"]["snapshot"].clone()
}

/// An ASCII fixture field follows a non-BMP comment; the cursor is calculated from current UTF-16 text.
fn query(source: &Path, body: &str, field: &str, revision: &Value) -> Value {
    let before = &body[..=body.rfind(field).unwrap()];
    let line = before.bytes().filter(|&byte| byte == b'\n').count();
    let start = before.rfind('\n').map_or(0, |at| at + 1);
    request(
        json!("union"),
        "textDocument/definition",
        json!({"textDocument":{"uri":Url::from_file_path(source.join(CALLER)).unwrap().as_str()},"position":{"line":line,"character":before[start..].encode_utf16().count()},"eska":{"snapshot":revision}}),
    )
}

/// Nullable and object/reference unions navigate one declaration through saved/editor/invalid/close states.
#[test]
fn common_union_fields_use_current_yaml_and_hash_in_both_locales() {
    let directory = support::TestDirectory::new();
    let root = fixture(&directory.0);
    let source = root.join("src-with-unions");
    let caller = Url::from_file_path(source.join(CALLER)).unwrap();
    let file = source.join(OWNER);
    let owner = Url::from_file_path(&file).unwrap();
    let saved = fs::read_to_string(&file).unwrap();
    let editor = format!("# Я😀\r\n{}", saved.replace('\n', "\r\n"));
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = start(&root, lang);
        let mut trace = Vec::new();
        for (version, types) in [
            "СправочникОбъект.Sample, Неопределено",
            "Null, СправочникОбъект.Sample",
            "СправочникОбъект.Sample, СправочникСсылка.sAMPLE",
        ]
        .into_iter()
        .enumerate()
        {
            let current = body(types, "eXTRAfIELD");
            let revision = edit(
                &mut client,
                &caller,
                &current,
                i32::try_from(version + 1).unwrap(),
                version == 0,
            );
            trace.push(definition_member_queries::confirmed(
                &mut client,
                &source,
                &current,
                "eXTRAfIELD",
                OWNER,
                UUID,
                &revision,
            ));
        }
        let current = body(
            "СправочникОбъект.Sample, СправочникСсылка.Sample",
            "ExtraField",
        );
        edit(&mut client, &caller, &current, 4, false);
        let revision = edit(&mut client, &owner, &editor, 7, true);
        fs::remove_file(&file).unwrap();
        let target = definition_member_queries::confirmed(
            &mut client,
            &source,
            &current,
            "ExtraField",
            OWNER,
            UUID,
            &revision,
        );
        assert_ne!(target["result"]["range"], trace[0]["result"]["range"]);
        trace.push(target);
        let revision = edit(&mut client, &owner, "broken: [\n", 8, false);
        let invalid = client.ask(&query(&source, &current, "ExtraField", &revision));
        assert_eq!(invalid["error"]["data"]["kind"], "navigation_incomplete");
        trace.push(invalid);
        fs::write(&file, &saved).unwrap();
        let closed=client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":owner.as_str()}}}));
        trace.push(definition_member_queries::confirmed(
            &mut client,
            &source,
            &current,
            "ExtraField",
            OWNER,
            UUID,
            &closed["params"]["snapshot"],
        ));
        let stale = client.ask(&query(&source, &current, "ExtraField", &snapshot("2")));
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

/// A same-named child on another owner, structure or primitive cannot produce a first-arm YAML target.
#[test]
fn ambiguous_union_fields_remain_incomplete_in_both_locales() {
    let directory = support::TestDirectory::new();
    let root = fixture(&directory.0);
    let source = root.join("src-with-unions");
    let caller = Url::from_file_path(source.join(CALLER)).unwrap();
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = start(&root, lang);
        let mut trace = Vec::new();
        for (version, types) in [
            "СправочникОбъект.Sample, СправочникОбъект.Other",
            "СправочникОбъект.Other, СправочникОбъект.Sample",
            "СправочникОбъект.Sample, Структура",
            "Число, СправочникОбъект.Sample",
            "Null, Неопределено",
        ]
        .into_iter()
        .enumerate()
        {
            let current = body(types, "ExtraField");
            let revision = edit(
                &mut client,
                &caller,
                &current,
                i32::try_from(version + 1).unwrap(),
                version == 0,
            );
            let result = client.ask(&query(&source, &current, "ExtraField", &revision));
            assert_eq!(
                result["error"]["data"]["kind"], "navigation_incomplete",
                "{current}: {result}"
            );
            trace.push(result);
        }
        if let Some(previous) = &previous {
            assert_eq!(&trace, previous);
        }
        previous = Some(trace);
        client.finish();
    }
}
