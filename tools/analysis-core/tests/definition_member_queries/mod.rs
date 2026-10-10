//! Current typed BSL members open inline metadata declarations in their physical YAML.

use super::*;

const CALLER: &str = "Catalogs/Sample/Commands/Run/Ext/CommandModule.bsl";
const CATALOG: &str = "Catalogs/Sample/object.yaml";
const ENUM: &str = "Enums/Choice/object.yaml";

/// Expand only a task-owned native fixture and import it into another unique source directory.
pub fn fixture(directory: &Path) -> std::path::PathBuf {
    let root = support::inline_command_project(directory, "// Saved caller.\n");
    let native = directory.join("native");
    let configuration = fs::read_to_string(native.join("Configuration.xml")).unwrap();
    fs::write(
        native.join("Configuration.xml"),
        configuration.replace(
            "<Catalog>Sample</Catalog>",
            "<Catalog>Sample</Catalog><Enum>Choice</Enum>",
        ),
    )
    .unwrap();
    let catalog = fs::read_to_string(native.join("Catalogs/Sample.xml")).unwrap();
    let catalog = catalog.replace("version=\"2.20\"", "xmlns:v8=\"http://v8.1c.ru/8.1/data/core\" xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" version=\"2.20\"")
        .replace("<ChildObjects>", "<ChildObjects><Attribute uuid=\"60000000-0000-4000-8000-000000000001\"><Properties><Name>ExtraField</Name><Type><v8:Type>xs:string</v8:Type></Type></Properties></Attribute><TabularSection uuid=\"70000000-0000-4000-8000-000000000001\"><Properties><Name>Items</Name></Properties><ChildObjects><Attribute uuid=\"80000000-0000-4000-8000-000000000001\"><Properties><Name>Quantity</Name><Type><v8:Type>xs:decimal</v8:Type></Type></Properties></Attribute></ChildObjects></TabularSection>");
    fs::write(native.join("Catalogs/Sample.xml"), catalog).unwrap();
    fs::create_dir_all(native.join("Enums")).unwrap();
    fs::write(native.join("Enums/Choice.xml"), "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Enum uuid=\"50000000-0000-4000-8000-000000000001\"><Properties><Name>Choice</Name></Properties><ChildObjects><EnumValue uuid=\"60000000-0000-4000-8000-000000000002\"><Properties><Name>First</Name></Properties></EnumValue></ChildObjects></Enum></MetaDataObject>").unwrap();
    eska::project_core::source_format::import_designer(&native, &root.join("src-with-members"))
        .unwrap();
    let config = fs::read_to_string(root.join("eska.toml")).unwrap();
    fs::write(
        root.join("eska.toml"),
        config.replace("src-with-command", "src-with-members"),
    )
    .unwrap();
    root
}

/// Metadata is explicitly read; current editor BSL still shares the same monotonically ordered revision.
fn start(root: &Path, lang: &str) -> Client {
    let mut client = Client::with_options(root, lang, &[]);
    client.ask(&initialize());
    client.send(&notification("initialized"));
    for (revision, path) in [("0", CATALOG), ("1", ENUM)] {
        let result = client.ask(&request(
            json!("read"),
            "eska/metadata/read",
            json!({"snapshot":snapshot(revision),"source":path}),
        ));
        assert!(result.get("result").is_some(), "{result}");
    }
    client
}

/// Fixture field identifiers are ASCII; the leading UTF-16 cursor counts a non-BMP comment separately.
fn query(source: &Path, body: &str, name: &str, revision: &Value) -> Value {
    let before = &body[..=body.rfind(name).unwrap()];
    let line = before.bytes().filter(|&byte| byte == b'\n').count();
    let start = before.rfind('\n').map_or(0, |index| index + 1);
    request(
        json!("member"),
        "textDocument/definition",
        json!({"textDocument":{"uri":url::Url::from_file_path(source.join(CALLER)).unwrap().as_str()},
        "position":{"line":line,"character":before[start..].encode_utf16().count()},"eska":{"snapshot":revision}}),
    )
}

/// A confirmed nested definition must match independently selected UUID navigation in every field.
pub fn confirmed(
    client: &mut Client,
    source: &Path,
    body: &str,
    name: &str,
    owner: &str,
    uuid: &str,
    revision: &Value,
) -> Value {
    let result = client.ask(&query(source, body, name, revision));
    assert!(result.get("result").is_some(), "{body}: {result}");
    let direct = client.ask(&request(
        json!("direct"),
        "eska/metadata/locate",
        json!({"snapshot":revision,"source":owner,"uuid":uuid}),
    ));
    assert!(direct.get("result").is_some(), "{direct}");
    assert_eq!(result["result"]["uri"], direct["result"]["location"]["uri"]);
    assert_eq!(
        result["result"]["range"],
        direct["result"]["location"]["range"]
    );
    for key in ["uuid", "source", "source_sha256", "origin", "native_path"] {
        assert_eq!(
            result["result"]["eska"]["target"][key],
            direct["result"][key]
        );
    }
    assert_eq!(
        result["result"]["eska"]["target"]["start_byte"],
        direct["result"]["byte_range"]["start"]
    );
    assert_eq!(
        result["result"]["eska"]["target"]["end_byte"],
        direct["result"]["byte_range"]["end"]
    );
    assert_eq!(
        result["result"]["eska"]["resources"],
        direct["result"]["resources"]
    );
    result
}

/// Real framed language requests distinguish enum values, attributes, tables and their row columns.
#[test]
fn nested_definitions_resolve_typed_members_and_refuse_structure_fields_in_both_locales() {
    let directory = support::TestDirectory::new();
    let root = fixture(&directory.0);
    let source = root.join("src-with-members");
    let uri = url::Url::from_file_path(source.join(CALLER)).unwrap();
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = start(&root, lang);
        let mut trace = Vec::new();
        for (version, body, field, owner, uuid) in [
            (
                1,
                "Объект=Catalogs.Sample.CreateItem();\nЗначение=Объект.eXTRAfIELD;",
                "eXTRAfIELD",
                CATALOG,
                "60000000-0000-4000-8000-000000000001",
            ),
            (
                2,
                "Объект=Catalogs.Sample.CreateItem();\nЗначение=Объект.iTEMS;",
                "iTEMS",
                CATALOG,
                "70000000-0000-4000-8000-000000000001",
            ),
            (
                3,
                "Объект=Catalogs.Sample.CreateItem();\nСтрока=Объект.Items.Add();\nЗначение=Строка.qUANTITY;",
                "qUANTITY",
                CATALOG,
                "80000000-0000-4000-8000-000000000001",
            ),
            (
                4,
                "Значение=Enums.Choice.fIRST;",
                "fIRST",
                ENUM,
                "60000000-0000-4000-8000-000000000002",
            ),
        ] {
            let body = format!(
                "\u{feff}// Я😀\r\nПроцедура Тест()\r\n{}\r\nКонецПроцедуры\r\n",
                body.replace('\n', "\r\n")
            );
            let event = if version == 1 {
                client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri.as_str(),"languageId":"bsl","version":version,"text":body}}}))
            } else {
                client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri.as_str(),"version":version},"contentChanges":[{"text":body}]}}))
            };
            trace.push(confirmed(
                &mut client,
                &source,
                &body,
                field,
                owner,
                uuid,
                &event["params"]["snapshot"],
            ));
        }
        let body = "Процедура Тест()\nОбъект=Новый Структура(\"ExtraField\", 1);\nЗначение=Объект.ExtraField;\nКонецПроцедуры\n";
        let event = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri.as_str(),"version":5},"contentChanges":[{"text":body}]}}));
        let refused = client.ask(&query(
            &source,
            body,
            "ExtraField",
            &event["params"]["snapshot"],
        ));
        assert_eq!(refused["error"]["data"]["kind"], "navigation_incomplete");
        trace.push(refused);
        if let Some(previous) = &previous {
            assert_eq!(&trace, previous);
        }
        previous = Some(trace);
        assert_eq!(
            fs::read_to_string(source.join(CALLER)).unwrap(),
            "// Saved caller.\n"
        );
        client.finish();
    }
}

/// YAML renaming, invalidation and close update the semantic value's location without reading disk.
#[test]
fn enum_member_definition_follows_editor_rename_and_close_in_both_locales() {
    let directory = support::TestDirectory::new();
    let root = fixture(&directory.0);
    let source = root.join("src-with-members");
    let caller = url::Url::from_file_path(source.join(CALLER)).unwrap();
    let file = source.join(ENUM);
    let uri = url::Url::from_file_path(&file).unwrap();
    let saved = fs::read_to_string(&file).unwrap();
    let parsed = eska::project_core::source_format::SourceReader::open(&source)
        .unwrap()
        .read_owner(&eska::project_core::source_format::SourcePath::parse(ENUM).unwrap())
        .unwrap();
    let range = parsed
        .declarations
        .iter()
        .find(|item| item.name == "First")
        .unwrap()
        .name_range
        .clone();
    let mut renamed = saved.clone();
    renamed.replace_range(range, "Second");
    let draft = format!("# Я😀 current value\r\n{}", renamed.replace('\n', "\r\n"));
    let body = "Процедура Тест()\nЗначение=Enums.Choice.First;\nКонецПроцедуры\n";
    let current = body.replace("First", "Second");
    let uuid = "60000000-0000-4000-8000-000000000002";
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = start(&root, lang);
        client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":caller.as_str(),"languageId":"bsl","version":1,"text":body}}}));
        let first = confirmed(
            &mut client,
            &source,
            body,
            "First",
            ENUM,
            uuid,
            &snapshot("3"),
        );
        client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri.as_str(),"languageId":"eska-source","version":7,"text":draft}}}));
        let old = client.ask(&query(&source, body, "First", &snapshot("4")));
        assert_eq!(old["error"]["data"]["kind"], "navigation_incomplete");
        client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":caller.as_str(),"version":2},"contentChanges":[{"text":current}]}}));
        let editor = confirmed(
            &mut client,
            &source,
            &current,
            "Second",
            ENUM,
            uuid,
            &snapshot("5"),
        );
        assert_eq!(
            editor["result"]["eska"]["target"]["origin"],
            json!({"kind":"overlay","document_version":7})
        );
        assert_ne!(editor["result"]["range"], first["result"]["range"]);
        fs::remove_file(&file).unwrap();
        assert_eq!(
            confirmed(
                &mut client,
                &source,
                &current,
                "Second",
                ENUM,
                uuid,
                &snapshot("5")
            ),
            editor
        );
        fs::write(&file, &saved).unwrap();
        client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri.as_str(),"version":8},"contentChanges":[{"text":"broken: [\n"}]}}));
        let invalid = client.ask(&query(&source, &current, "Second", &snapshot("6")));
        assert_eq!(invalid["error"]["data"]["kind"], "navigation_incomplete");
        client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri.as_str()}}}));
        let absent = client.ask(&query(&source, &current, "Second", &snapshot("7")));
        assert_eq!(absent["error"]["data"]["kind"], "navigation_incomplete");
        client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":caller.as_str(),"version":3},"contentChanges":[{"text":body}]}}));
        let restored = confirmed(
            &mut client,
            &source,
            body,
            "First",
            ENUM,
            uuid,
            &snapshot("8"),
        );
        assert_eq!(
            restored["result"]["eska"]["target"],
            first["result"]["eska"]["target"]
        );
        let trace = [first, old, editor, invalid, absent, restored];
        if let Some(previous) = &previous {
            assert_eq!(&trace, previous);
        }
        previous = Some(trace);
        assert_eq!(fs::read_to_string(&file).unwrap(), saved);
        client.finish();
    }
}
