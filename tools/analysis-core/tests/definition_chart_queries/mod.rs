//! Framed requests navigate real chart declarations while refusing synthetic YAML targets.

mod fixture;

use super::definition_member_queries::confirmed;
use super::*;

const CALLER: &str = "Catalogs/Sample/Commands/Run/Ext/CommandModule.bsl";

/// Import only private native chart fixtures, retaining the existing declared caller module.
fn project(directory: &Path) -> std::path::PathBuf {
    let root = support::inline_command_project(directory, "// Saved caller.\n");
    fixture::extend(&directory.join("native"));
    eska::project_core::source_format::import_designer(
        &directory.join("native"),
        &root.join("src-charts"),
    )
    .unwrap();
    let config = fs::read_to_string(root.join("eska.toml")).unwrap();
    fs::write(
        root.join("eska.toml"),
        config.replace("src-with-command", "src-charts"),
    )
    .unwrap();
    root
}

/// All new chart owners are loaded explicitly before publishing editor BSL.
fn start(root: &Path, lang: &str) -> Client {
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
    client
}

/// Full-document updates preserve the current caller's monotonic version and snapshot.
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

/// Complete first characters keep Unicode fixture cursors on valid UTF-8/UTF-16 boundaries.
fn refused(client: &mut Client, uri: &url::Url, body: &str, field: &str, snap: &Value) -> Value {
    let end = body.rfind(field).unwrap() + field.chars().next().unwrap().len_utf8();
    let before = &body[..end];
    let line = before.bytes().filter(|&byte| byte == b'\n').count();
    let start = before.rfind('\n').map_or(0, |at| at + 1);
    let result=client.ask(&request(json!("missing"),"textDocument/definition",json!({"textDocument":{"uri":uri.as_str()},"position":{"line":line,"character":before[start..].encode_utf16().count()},"eska":{"snapshot":snap}})));
    assert_eq!(
        result["error"]["data"]["kind"], "navigation_incomplete",
        "{body}: {result}"
    );
    result
}

/// Object/row declarations and implicit-row real flags match locate; implicit sections/columns never invent a target.
#[test]
fn chart_definitions_and_implicit_member_refusals_are_locale_independent() {
    let directory = support::TestDirectory::new();
    let root = project(&directory.0);
    let source = root.join("src-charts");
    let uri = url::Url::from_file_path(source.join(CALLER)).unwrap();
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = start(&root, lang);
        let mut version = 1;
        let mut trace = Vec::new();
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
        for (receiver, field, part) in [
            ("", "Accounting", 4),
            (
                "Object=Object.ExtDimensionTypes.Add();",
                "QuantityAccounting",
                5,
            ),
        ] {
            let body = fixture::body(case, receiver, field);
            let snap = bsl(&mut client, &uri, version, &body);
            version += 1;
            trace.push(confirmed(
                &mut client,
                &source,
                &body,
                field,
                "ChartsOfAccounts/Accounts/object.yaml",
                &fixture::uuid(case, part),
                &snap,
            ));
        }
        for (case, receiver, field) in [
            (&fixture::CASES[1], "", "ВидыСубконто"),
            (
                &fixture::CASES[1],
                "Object=Object.ExtDimensionTypes.Add();",
                "ExtDimensionType",
            ),
            (
                &fixture::CASES[1],
                "Object=Object.ExtDimensionTypes.Add();",
                "Accounting",
            ),
            (&fixture::CASES[1], "", "QuantityAccounting"),
            (&fixture::CASES[2], "", "БазовыеВидыРасчета"),
            (
                &fixture::CASES[2],
                "Object=Object.BaseCalculationTypes.Add();",
                "CalculationType",
            ),
        ] {
            let body = fixture::body(case, receiver, field);
            let snap = bsl(&mut client, &uri, version, &body);
            version += 1;
            trace.push(refused(&mut client, &uri, &body, field, &snap));
        }
        if let Some(previous) = &previous {
            assert_eq!(&trace, previous);
        }
        previous = Some(trace);
        client.finish();
    }
}
