//! Independent framed clients exercise query RPCs without reusing production DTO/framing code.

mod buffers;
mod contexts;
mod xml;

use super::*;
use crate::backend::project::metadata_edit::snapshot;

const INPUT: &str = "\u{feff}// 😀 prefix\r\nПроцедура Тест()\r\n\tДругоеИмя = \"ВЫБРАТЬ \"\"😀\"\"\r\n\t// owner comment\r\n\t|ИЗ Таблица\"; // outside\r\nКонецПроцедуры\r\n";
const SOURCE: &str = "Ext/ManagedApplicationModule.bsl";

/// Keep the module and policy real, small and isolated from existing user projects.
fn fixture_query(root: &std::path::Path) {
    fixture(root, "configuration");
    std::fs::write(root.join("src").join(SOURCE), INPUT).unwrap();
    std::fs::write(
        root.join("src/Ext/ParentConfigurations.bin"),
        "{6,0,0,0,0,0}",
    )
    .unwrap();
}

/// Query capabilities require API 1.13; no analyzer binary is present or started by these tests.
fn initialize(client: &mut Client, locale: &str) -> Value {
    let hello = client.ok("initialize", json!({"apiVersion":{"major":1,"minor":13},"client":{"name":"query-test","version":"1"},"locale":locale}));
    assert_eq!(hello["capabilities"]["queryEditing"]["schemaVersion"], 1);
    assert_eq!(hello["capabilities"]["queryEditing"]["writesOwner"], false);
    assert_eq!(hello["limits"]["maxRequestBytes"], 1_048_576);
    hello
}

/// Capture the protocol's session/member generation independently of Rust scope types.
fn key(opened: &Value, index: usize) -> Value {
    json!({"sessionId":opened["sessionId"],"projectId":opened["projects"][index]["projectId"],"generation":opened["projects"][index]["generation"]})
}

/// Merge only object fields, as an external client composing the documented request envelope would.
fn with(base: &Value, fields: Value) -> Value {
    let mut result = base.clone();
    let Value::Object(fields) = fields else {
        panic!("fixture fields are an object")
    };
    result.as_object_mut().unwrap().extend(fields);
    result
}

/// The source node comes from the actual root's module role, never from a guessed logical ID.
fn module(opened: &Value, index: usize) -> Value {
    json!({"kind":"module","owner":opened["projects"][index]["root"]["objectId"],"role":"managed-application"})
}

/// All subsequent requests echo both independently checked editor and application revisions.
fn binding(key: &Value, opened: &Value) -> Value {
    with(
        key,
        json!({"documentId":opened["documentId"],"version":opened["version"],"snapshot":opened["snapshot"],"contextFingerprint":opened["contextFingerprint"]}),
    )
}

/// Compute source UTF-16 coordinates independently from the backend's map helpers.
fn point(input: &str, byte: usize) -> Value {
    let prefix = &input[..byte];
    json!({"line":prefix.matches('\n').count(),"character":prefix.rsplit('\n').next().unwrap().encode_utf16().count()})
}

/// Applying returned bytes models an editor `WorkspaceEdit` without publishing a file.
fn apply(input: &str, plan: &Value) -> String {
    let mut output = input.to_owned();
    for replacement in plan["replacements"].as_array().unwrap().iter().rev() {
        let start = usize::try_from(replacement["range"]["start"].as_u64().unwrap()).unwrap();
        let end = usize::try_from(replacement["range"]["end"].as_u64().unwrap()).unwrap();
        output.replace_range(start..end, replacement["text"].as_str().unwrap());
    }
    output
}

/// Legacy metadata clients cannot allocate query state; RU/EN clients get identical query plans/maps.
#[test]
fn query_rpc_reads_and_plans_dirty_bsl_with_locale_independent_maps() {
    let directory = TestDir::new();
    fixture_query(&directory.0);
    let original = bytes(&directory.0);
    let mut legacy = Client::new("en-US");
    let hello = legacy.initialize("en-US");
    assert!(hello["capabilities"].get("queryEditing").is_none());
    assert_eq!(
        legacy.request("query/bufferBegin", json!({}))["error"]["code"],
        -32601
    );
    legacy.finish();
    let mut expected = None;
    for locale in ["ru-RU", "en-US"] {
        let mut client = Client::new(locale);
        initialize(&mut client, locale);
        let workspace = client.open(&directory.0);
        let key = key(&workspace, 0);
        let dirty = INPUT.replace("// outside", "// unsaved owner");
        let opened = client.ok("query/open", with(&key, json!({"node":module(&workspace,0),"source":{"value":SOURCE,"encoding":"utf-8"},"version":"7","text":dirty})));
        assert_eq!(opened["snapshot"], snapshot(&dirty));
        assert_eq!(opened["diskSnapshot"], snapshot(INPUT));
        assert_eq!(opened["writable"], true);
        let revision = binding(&key, &opened);
        let query = client.ok(
            "query/read",
            with(
                &revision,
                json!({"at":point(&dirty, dirty.find('"').unwrap()+1)}),
            ),
        );
        assert_eq!(query["query"]["text"], "ВЫБРАТЬ \"😀\"\nИЗ Таблица");
        assert_eq!(query["query"]["comments"][0]["text"], "\t// owner comment");
        let no_op = client.ok("query/plan", with(&revision, json!({"query":query["query"]["identity"],"text":query["query"]["text"],"comments":[]})));
        assert_eq!(no_op["noOp"], true);
        assert_eq!(no_op["replacements"], json!([]));
        let draft = "ВЫБРАТЬ \"😀\"\nИЗ ДругаяТаблица";
        let plan = client.ok(
            "query/plan",
            with(
                &revision,
                json!({"query":query["query"]["identity"],"text":draft,"comments":[]}),
            ),
        );
        assert_eq!(
            apply(&dirty, &plan),
            dirty.replacen("ИЗ Таблица", "ИЗ ДругаяТаблица", 1)
        );
        assert_eq!(plan["projectedQuery"]["text"], draft);
        let mapped = client.ok("query/map", with(&revision, json!({"query":query["query"]["identity"],"direction":"queryToSource","positions":[{"line":0,"character":9}]})));
        assert_eq!(
            mapped["positions"][0],
            point(&dirty, dirty.find("\"\"😀").unwrap() + 2)
        );
        let invalid = client.request("query/map", with(&revision, json!({"query":query["query"]["identity"],"direction":"queryToSource","positions":[{"line":0,"character":10}]})));
        assert_eq!(
            invalid["error"]["data"]["details"]["reason"],
            "invalid_position"
        );
        let snapshot = (opened, query, no_op, plan);
        if let Some(expected) = &expected {
            assert_eq!(&snapshot, expected);
        } else {
            expected = Some(snapshot);
        }
        client.ok(
            "query/close",
            with(&key, json!({"documentId":revision["documentId"]})),
        );
        client.finish();
    }
    assert_eq!(bytes(&directory.0), original);
}

/// Missing comment decisions cannot silently delete BSL trivia, including a missing nullable field.
#[test]
fn query_rpc_requires_explicit_source_comment_destinations() {
    let directory = TestDir::new();
    fixture_query(&directory.0);
    let mut client = Client::new("en-US");
    initialize(&mut client, "en-US");
    let workspace = client.open(&directory.0);
    let key = key(&workspace, 0);
    let opened = client.ok("query/open", with(&key, json!({"node":module(&workspace,0),"source":{"value":SOURCE,"encoding":"utf-8"},"version":"1"})));
    let revision = binding(&key, &opened);
    let query = client.ok(
        "query/read",
        with(
            &revision,
            json!({"at":point(INPUT, INPUT.find('"').unwrap())}),
        ),
    );
    let plan = with(
        &revision,
        json!({"query":query["query"]["identity"],"text":"NEW\nQUERY","comments":[]}),
    );
    assert_eq!(
        client.request("query/plan", plan.clone())["error"]["data"]["details"]["reason"],
        "comments_require_decision"
    );
    let invalid = with(&plan, json!({"comments":[{"comment":0}]}));
    assert_eq!(
        client.request("query/plan", invalid)["error"]["code"],
        -32602
    );
    let moved = client.ok(
        "query/plan",
        with(&plan, json!({"comments":[{"comment":0,"queryOffset":4}]})),
    );
    assert_eq!(
        moved["projectedQuery"]["comments"][0]["text"],
        "\t// owner comment"
    );
    let removed = client.ok(
        "query/plan",
        with(
            &plan,
            json!({"comments":[{"comment":0,"queryOffset":null}]}),
        ),
    );
    assert_eq!(removed["projectedQuery"]["comments"], json!([]));
    client.finish();
}
