//! Real framed requests expose the added canonical object families and exact task-addressing locations.

mod fixture;

use super::definition_member_queries::confirmed;
use super::*;

const CALLER: &str = "Catalogs/Sample/Commands/Run/Ext/CommandModule.bsl";

/// Extend and import only the test's owned fixture; existing demos and repository sources are never destinations.
fn project(directory: &Path) -> std::path::PathBuf {
    let root = support::inline_command_project(directory, "// Saved caller.\n");
    let native = directory.join("native");
    fixture::extend(&native);
    eska::project_core::source_format::import_designer(&native, &root.join("src-process")).unwrap();
    let config = fs::read_to_string(root.join("eska.toml")).unwrap();
    fs::write(
        root.join("eska.toml"),
        config.replace("src-with-command", "src-process"),
    )
    .unwrap();
    root
}

/// Saved owner models are selected explicitly; remaining unrelated metadata stays pending.
fn start(root: &Path, lang: &str) -> (Client, Value) {
    let mut client = Client::with_options(root, lang, &[]);
    client.ask(&initialize());
    client.send(&notification("initialized"));
    let mut snap = snapshot("0");
    for case in &fixture::CASES {
        let result = client.ask(&request(
            json!("owner"),
            "eska/metadata/read",
            json!({"snapshot":snap,"source":format!("{}/{}/object.yaml",case.folder,case.name)}),
        ));
        assert_eq!(
            result["result"]["metadata"]["state"]["model"], "ready",
            "{result}"
        );
        snap = result["result"]["snapshot"].clone();
    }
    (client, snap)
}

/// Full-sync caller updates publish one current revision before navigation.
fn bsl(client: &mut Client, uri: &url::Url, version: i32, body: &str) -> Value {
    let value = if version == 1 {
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri.as_str(),"languageId":"bsl","version":version,"text":body}}})
    } else {
        json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri.as_str(),"version":version},"contentChanges":[{"text":body}]}})
    };
    let changed = client.ask(&value);
    assert_eq!(changed["method"], "eska/inputChanged");
    changed["params"]["snapshot"].clone()
}

/// Each typed root/member matches a separate metadata locate, including the task's physical addressing kind.
#[test]
fn process_task_exchange_definitions_match_current_yaml_in_both_locales() {
    let directory = support::TestDirectory::new();
    let root = project(&directory.0);
    let source = root.join("src-process");
    let uri = url::Url::from_file_path(source.join(CALLER)).unwrap();
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let (mut client, _) = start(&root, lang);
        let mut trace = Vec::new();
        let mut version = 1;
        for case in &fixture::CASES {
            let owner = format!("{}/{}/object.yaml", case.folder, case.name);
            for (receiver, field, part) in [
                ("", case.name, 0),
                ("", "ExtraField", 1),
                ("", "Rows", 2),
                ("Object=Object.Rows.Add();", "Quantity", 3),
            ] {
                let body =
                    fixture::body(case, receiver, if part == 0 { "ExtraField" } else { field });
                let snap = bsl(&mut client, &uri, version, &body);
                version += 1;
                trace.push(confirmed(
                    &mut client,
                    &source,
                    &body,
                    field,
                    &owner,
                    &fixture::uuid(case, part),
                    &snap,
                ));
            }
        }
        let case = &fixture::CASES[1];
        let body = fixture::body(case, "", "Assignee");
        let snap = bsl(&mut client, &uri, version, &body);
        trace.push(confirmed(
            &mut client,
            &source,
            &body,
            "Assignee",
            "Tasks/Work/object.yaml",
            &fixture::uuid(case, 4),
            &snap,
        ));
        assert_eq!(
            trace.last().unwrap()["result"]["eska"]["target"]["native_path"],
            "Tasks/Work.xml"
        );
        if let Some(previous) = &previous {
            assert_eq!(&trace, previous);
        }
        previous = Some(trace);
        client.finish();
    }
}
