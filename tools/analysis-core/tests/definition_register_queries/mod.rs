//! Framed requests navigate register members using one current canonical project session.

use super::definition_member_queries::confirmed;
use super::*;

mod fixture;

const CALLER: &str = "Catalogs/Sample/Commands/Run/Ext/CommandModule.bsl";
const OWNER: &str = "InformationRegisters/Rates/object.yaml";

/// Import only the extended test-owned native copy into its own source directory.
fn project(directory: &Path) -> std::path::PathBuf {
    let root = support::inline_command_project(directory, "// Saved caller.\n");
    fixture::extend(&directory.join("native"));
    eska::project_core::source_format::import_designer(
        &directory.join("native"),
        &root.join("src-with-registers"),
    )
    .unwrap();
    let file = root.join("eska.toml");
    let config = fs::read_to_string(&file).unwrap();
    fs::write(
        file,
        config.replace("src-with-command", "src-with-registers"),
    )
    .unwrap();
    root
}

/// Explicitly load the four selected canonical models; queries do not discover them on disk.
fn start(root: &Path, lang: &str) -> Client {
    let mut client = Client::with_options(root, lang, &[]);
    client.ask(&initialize());
    client.send(&notification("initialized"));
    for (revision, case) in fixture::CASES.iter().enumerate() {
        let owner = format!("{}/{}/object.yaml", case.folder, case.name);
        let read = client.ask(&request(
            json!("read"),
            "eska/metadata/read",
            json!({"snapshot":snapshot(&revision.to_string()),"source":owner}),
        ));
        assert_eq!(
            read["result"]["metadata"]["state"]["model"], "ready",
            "{read}"
        );
    }
    client
}

/// A full current BSL overlay is acknowledged before any request uses the returned revision.
fn overlay(client: &mut Client, source: &Path, version: i32, body: &str) -> Value {
    let uri = url::Url::from_file_path(source.join(CALLER)).unwrap();
    let event = if version == 1 {
        client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri.as_str(),"version":version,"languageId":"bsl","text":body}}}))
    } else {
        client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri.as_str(),"version":version},"contentChanges":[{"text":body}]}}))
    };
    event["params"]["snapshot"].clone()
}

/// Fixture field tokens are ASCII; their preceding line content is counted in UTF-16.
fn query(source: &Path, body: &str, field: &str, revision: &Value) -> Value {
    let uri = url::Url::from_file_path(source.join(CALLER)).unwrap();
    let before = &body[..=body.rfind(field).unwrap()];
    let line = before.bytes().filter(|&b| b == b'\n').count();
    let start = before.rfind('\n').map_or(0, |at| at + 1);
    request(
        json!("member"),
        "textDocument/definition",
        json!({"textDocument":{"uri":uri.as_str()},"position":{"line":line,"character":before[start..].encode_utf16().count()},"eska":{"snapshot":revision}}),
    )
}

/// Same-named fields in different register flavours resolve their own UUID and current YAML target.
#[test]
fn register_record_members_and_filter_dimensions_resolve_in_both_locales() {
    let directory = support::TestDirectory::new();
    let root = project(&directory.0);
    let source = root.join("src-with-registers");
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = start(&root, lang);
        let mut trace = Vec::new();
        let mut version = 1;
        for case in &fixture::CASES {
            let owner = format!("{}/{}/object.yaml", case.folder, case.name);
            for (receiver, field, part) in [
                ("Record=Set.Add();", "kEY", 1),
                ("Record=Set.Add();", "aMOUNT", 2),
                ("Record=Set.Add();", "nOTE", 3),
                ("Record=Set.Filter;", "kEY", 1),
            ] {
                let body = fixture::body(case, receiver, field);
                let revision = overlay(&mut client, &source, version, &body);
                version += 1;
                trace.push(confirmed(
                    &mut client,
                    &source,
                    &body,
                    field,
                    &owner,
                    &fixture::uuid(case, part),
                    &revision,
                ));
            }
            for (receiver, field) in [
                ("Record=Set;", "Key"),
                ("Record=Set.Filter;", "Amount"),
                ("Record=Set.Add();", "Period"),
                ("Record=Set.Filter;", "Period"),
                ("Record=New Structure(\"Key\",1);", "Key"),
            ] {
                let body = fixture::body(case, receiver, field);
                let revision = overlay(&mut client, &source, version, &body);
                version += 1;
                let refused = client.ask(&query(&source, &body, field, &revision));
                assert_eq!(
                    refused["error"]["data"]["kind"], "navigation_incomplete",
                    "{refused}"
                );
                trace.push(refused);
            }
        }
        let case = &fixture::CASES[0];
        for (field, part) in [("kEY", 1), ("aMOUNT", 2), ("nOTE", 3)] {
            let body = fixture::body(
                case,
                "Record=Collection.Rates.CreateRecordManager();",
                field,
            );
            let revision = overlay(&mut client, &source, version, &body);
            version += 1;
            trace.push(confirmed(
                &mut client,
                &source,
                &body,
                field,
                OWNER,
                &fixture::uuid(case, part),
                &revision,
            ));
        }
        if let Some(previous) = &previous {
            assert_eq!(&trace, previous);
        }
        previous = Some(trace);
        client.finish();
    }
}

/// A dimension rename updates the filter facade immediately; invalidation and close preserve no old target.
#[test]
fn register_filter_dimension_tracks_current_yaml_rename_without_disk_read_in_both_locales() {
    let directory = support::TestDirectory::new();
    let root = project(&directory.0);
    let source = root.join("src-with-registers");
    let file = source.join(OWNER);
    let uri = url::Url::from_file_path(&file).unwrap();
    let saved = fs::read_to_string(&file).unwrap();
    let parsed = eska::project_core::source_format::SourceReader::open(&source)
        .unwrap()
        .read_owner(&eska::project_core::source_format::SourcePath::parse(OWNER).unwrap())
        .unwrap();
    let range = parsed
        .declarations
        .iter()
        .find(|d| d.name == "Key")
        .unwrap()
        .name_range
        .clone();
    let mut renamed = saved.clone();
    renamed.replace_range(range, "ChangedKey");
    let draft = format!(
        "# Я😀 current dimension\r\n{}",
        renamed.replace('\n', "\r\n")
    );
    let case = &fixture::CASES[0];
    let uuid = fixture::uuid(case, 1);
    let body = fixture::body(case, "Record=Set.Filter;", "Key");
    let current = body.replace("Record.Key", "Record.ChangedKey");
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = start(&root, lang);
        let revision = overlay(&mut client, &source, 1, &body);
        let first = confirmed(&mut client, &source, &body, "Key", OWNER, &uuid, &revision);
        let event=client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri.as_str(),"version":7,"languageId":"eska-source","text":draft}}}));
        let old = client.ask(&query(&source, &body, "Key", &event["params"]["snapshot"]));
        assert_eq!(old["error"]["data"]["kind"], "navigation_incomplete");
        let revision = overlay(&mut client, &source, 2, &current);
        let editor = confirmed(
            &mut client,
            &source,
            &current,
            "ChangedKey",
            OWNER,
            &uuid,
            &revision,
        );
        assert_eq!(
            editor["result"]["eska"]["target"]["origin"],
            json!({"kind":"overlay","document_version":7})
        );
        fs::remove_file(&file).unwrap();
        assert_eq!(
            confirmed(
                &mut client,
                &source,
                &current,
                "ChangedKey",
                OWNER,
                &uuid,
                &revision
            ),
            editor
        );
        fs::write(&file, &saved).unwrap();
        let event=client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri.as_str(),"version":8},"contentChanges":[{"text":"broken: [\n"}]}}));
        let invalid = client.ask(&query(
            &source,
            &current,
            "ChangedKey",
            &event["params"]["snapshot"],
        ));
        assert_eq!(invalid["error"]["data"]["kind"], "navigation_incomplete");
        let event=client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri.as_str()}}}));
        let absent = client.ask(&query(
            &source,
            &current,
            "ChangedKey",
            &event["params"]["snapshot"],
        ));
        assert_eq!(absent["error"]["data"]["kind"], "navigation_incomplete");
        let revision = overlay(&mut client, &source, 3, &body);
        let restored = confirmed(&mut client, &source, &body, "Key", OWNER, &uuid, &revision);
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
