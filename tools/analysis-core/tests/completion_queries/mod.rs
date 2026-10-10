//! Completion observes typed YAML changes and UTF-16 positions through the actual server process.

use std::{collections::BTreeSet, fs, path::Path};

use eska::{
    analysis_core::InputRequest,
    project_core::source_format::{SourcePath, SourceReader},
};
use eska_analysis::{AnalysisSession, Locale, QueryCancellation};
use serde_json::{Value, json};

use super::{Client, initialize, notification, request, snapshot, support};

mod fixture;

const MODULE: &str = "Catalogs/Sample/Ext/ObjectModule.bsl";
const OWNER: &str = "Catalogs/Sample/object.yaml";
const BODY: &str = "\u{feff}// Я😀\r\nПроцедура Проверка()\r\n\tОбъект = Справочники.Sample.СоздатьЭлемент();\r\n\tОбъект.\r\nКонецПроцедуры\r\n";

/// The prefix ends at the cursor, retaining BOM/CRLF and non-BMP content before it.
fn pull(source: &Path, module: &str, revision: &str, prefix: &str) -> Value {
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count();
    let character = prefix.rsplit('\n').next().unwrap().encode_utf16().count();
    request(
        json!("completion"),
        "textDocument/completion",
        json!({"textDocument":{"uri":url::Url::from_file_path(source.join(module)).unwrap().to_string()},
        "position":{"line":line,"character":character},"eska":{"snapshot":snapshot(revision)}}),
    )
}

/// Existing metadata reads prepare only explicitly selected owners, not the whole project.
fn read(client: &mut Client, owner: &str, revision: &str) -> Value {
    client.ask(&request(
        json!(5),
        "eska/metadata/read",
        json!({"source":owner,"snapshot":snapshot(revision)}),
    ))
}

/// Inspect semantic candidates without asserting unrelated presentation labels or sort weights.
fn labels(report: &Value) -> BTreeSet<String> {
    report["result"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["label"].as_str().unwrap().to_owned())
        .collect()
}

/// Compare actual shared-core candidates over the same explicit saved inputs and locale.
fn core_labels(source: &Path, cursor: usize, locale: Locale) -> BTreeSet<String> {
    let mut core = AnalysisSession::open(source).unwrap();
    let owner = SourcePath::parse(OWNER).unwrap();
    core.load_saved(&owner, core.revision(), &InputRequest::default())
        .unwrap();
    let module = SourcePath::parse(MODULE).unwrap();
    core.load_module_saved(&module, core.revision(), &InputRequest::default())
        .unwrap();
    core.module_completions(
        &module,
        core.revision(),
        u32::try_from(cursor).unwrap(),
        locale,
        &QueryCancellation::default(),
    )
    .unwrap()
    .value
    .into_iter()
    .map(|item| item.label)
    .collect()
}

/// A current YAML field replaces the saved field in real inference; invalid input loses the draft.
#[test]
fn completion_uses_current_yaml_and_bsl_with_core_parity_in_both_locales() {
    let (_directory, root) = fixture::project(BODY);
    let source = root.join("src-completion");
    let cursor = BODY.find("Объект.\r\n").unwrap() + "Объект.".len();
    let prefix = &BODY[..cursor];
    let owner = SourcePath::parse(OWNER).unwrap();
    let reader = SourceReader::open(&source).unwrap();
    let descriptor = reader.read_owner(&owner).unwrap();
    let range = descriptor
        .declarations
        .iter()
        .find(|node| node.name == "Description")
        .unwrap()
        .name_range
        .clone();
    let mut draft = fs::read_to_string(source.join(OWNER)).unwrap();
    draft.replace_range(range, "LiveField");
    fs::write(
        source.join("Catalogs/Sample.xml"),
        "<untrusted-native-fallback><Name>LiveField</Name></untrusted-native-fallback>",
    )
    .unwrap();
    for (lang, locale) in [("en-US", Locale::En), ("ru-RU", Locale::Ru)] {
        let mut client = Client::with_options(&root, lang, &[]);
        let init = client.ask(&initialize());
        assert_eq!(
            init["result"]["capabilities"]["completionProvider"]["resolveProvider"],
            false
        );
        assert_eq!(
            init["result"]["capabilities"]["completionProvider"]["triggerCharacters"],
            json!(["."])
        );
        client.send(&notification("initialized"));
        assert_eq!(
            read(&mut client, OWNER, "0")["result"]["snapshot"],
            snapshot("1")
        );
        let saved = client.ask(&pull(&source, MODULE, "1", prefix));
        assert!(labels(&saved).contains("Description"), "{saved}");
        assert!(!labels(&saved).contains("LiveField"));
        assert_eq!(saved["result"]["eska"]["snapshot"], snapshot("2"));
        assert_eq!(saved["result"]["eska"]["position_byte"], cursor);
        assert_eq!(saved["result"]["isIncomplete"], true);
        assert_eq!(
            saved["result"]["eska"]["coverage"]["semantic_complete"],
            false
        );
        assert_eq!(labels(&saved), core_labels(&source, cursor, locale));
        let uri = url::Url::from_file_path(source.join(OWNER))
            .unwrap()
            .to_string();
        let opened=client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"yaml","version":1,"text":draft}}}));
        assert_eq!(opened["params"]["snapshot"], snapshot("3"));
        let current = client.ask(&pull(&source, MODULE, "3", prefix));
        assert!(labels(&current).contains("LiveField"));
        assert!(!labels(&current).contains("Description"));
        assert_eq!(
            current["result"]["eska"]["source_sha256"],
            saved["result"]["eska"]["source_sha256"]
        );
        let changed=client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":2},"contentChanges":[{"text":"broken: ["}]}}));
        assert_eq!(changed["params"]["snapshot"], snapshot("4"));
        let invalid = client.ask(&pull(&source, MODULE, "4", prefix));
        assert!(!labels(&invalid).contains("LiveField"));
        assert_eq!(invalid["result"]["isIncomplete"], true);
        assert_eq!(
            client.ask(&pull(&source, MODULE, "3", prefix))["error"]["data"]["kind"],
            "stale_revision"
        );
        let closed=client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri}}}));
        assert_eq!(closed["params"]["snapshot"], snapshot("5"));
        assert!(labels(&client.ask(&pull(&source, MODULE, "5", prefix))).contains("Description"));
        let module_uri = url::Url::from_file_path(source.join(MODULE))
            .unwrap()
            .to_string();
        let editor = BODY.replace("Справочники.Sample.СоздатьЭлемент()", "Новый Структура");
        client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":module_uri,"languageId":"bsl","version":1,"text":editor}}}));
        let editor_cursor = editor.find("Объект.\r\n").unwrap() + "Объект.".len();
        let edited = client.ask(&pull(&source, MODULE, "6", &editor[..editor_cursor]));
        assert!(!labels(&edited).contains("Description"));
        assert_ne!(
            edited["result"]["eska"]["source_sha256"],
            saved["result"]["eska"]["source_sha256"]
        );
        client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":module_uri}}}));
        assert_eq!(
            labels(&client.ask(&pull(&source, MODULE, "7", prefix))),
            labels(&saved)
        );
        client.finish();
    }
    assert_eq!(fs::read_to_string(source.join(MODULE)).unwrap(), BODY);
}

/// Cursor errors retain the new lazy-read snapshot; LSP columns past EOL clamp to current EOL.
#[test]
fn cursor_and_preflight_failures_never_return_successful_empty_completion() {
    let body = "\u{feff}// Я😀\r\nProcedure Test()\r\nEndProcedure\r\n";
    let (_directory, root) = support::project(body, false);
    let source = root.join("src");
    let module = "CommonModules/Tools/Ext/Module.bsl";
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        client.ask(&initialize());
        client.send(&notification("initialized"));
        let mut bad = pull(&source, module, "0", "");
        bad["params"]["position"] = json!({"line":0,"character":6});
        let error = client.ask(&bad);
        assert_eq!(error["error"]["data"]["kind"], "position_invalid");
        assert_eq!(error["error"]["data"]["details"], snapshot("1"));
        bad["params"]["eska"]["snapshot"] = snapshot("1");
        bad["params"]["position"] = json!({"line":0,"character":999});
        let clamped = client.ask(&bad);
        assert_eq!(
            clamped["result"]["eska"]["position_byte"],
            body.find('\r').unwrap()
        );
        for position in [
            json!({"line":9,"character":0}),
            json!({"line":0,"character":-1}),
            json!({"line":0,"character":4_294_967_295_u64}),
        ] {
            bad["params"]["position"] = position;
            assert!(client.ask(&bad).get("error").is_some());
        }
        bad = pull(&source, module, "1", "\u{feff}// Я😀");
        bad["params"]["context"] = json!({"triggerKind":2});
        assert_eq!(
            client.ask(&bad)["error"]["data"]["kind"],
            "rpc_invalid_params"
        );
        client.finish();
    }
}

/// Completion remains independent of the syntax rule and zero timeout keeps current inputs intact.
#[test]
fn completion_settings_and_deadline_have_distinct_current_failures() {
    let (_directory, root) = support::project("Procedure Test()\nEndProcedure\n", false);
    let source = root.join("src");
    let module = "CommonModules/Tools/Ext/Module.bsl";
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &["--timeout-seconds", "0"]);
        client.ask(&initialize());
        client.send(&notification("initialized"));
        let error = client.ask(&pull(&source, module, "0", ""));
        assert_eq!(error["error"]["code"], -32000);
        assert_eq!(error["error"]["data"]["kind"], "analysis_deadline_exceeded");
        assert!(error["error"]["data"].get("retriggerRequest").is_none());
        assert_eq!(error["error"]["data"]["details"], snapshot("0"));
        assert_eq!(
            read(&mut client, "CommonModules/Tools/object.yaml", "0")["result"]["snapshot"],
            snapshot("1")
        );
        client.finish();
        fs::write(
            root.join("bsl-analyzer.toml"),
            "[diagnostics.parameters]\nParseError=false\n",
        )
        .unwrap();
        let mut client = Client::with_options(&root, lang, &[]);
        let init = client.ask(&initialize());
        assert_eq!(
            init["result"]["capabilities"]["diagnosticProvider"]["identifier"],
            "eska-syntax"
        );
        assert!(
            init["result"]["capabilities"]
                .get("completionProvider")
                .is_some()
        );
        client.send(&notification("initialized"));
        let completed = client.ask(&pull(&source, module, "0", ""));
        assert!(completed.get("result").is_some());
        client.finish();
        fs::write(root.join("bsl-analyzer.toml"), "invalid=[\n").unwrap();
        let mut client = Client::with_options(&root, lang, &[]);
        let init = client.ask(&initialize());
        assert!(
            init["result"]["capabilities"]
                .get("completionProvider")
                .is_none()
        );
        client.send(&notification("initialized"));
        assert_eq!(
            client.ask(&pull(&source, module, "0", ""))["error"]["data"]["kind"],
            "analysis_config_invalid"
        );
        client.finish();
        fs::remove_file(root.join("bsl-analyzer.toml")).unwrap();
    }
}

/// Negotiated snippets never arrive as literal placeholders in a client that cannot interpret them.
#[test]
fn completion_snippets_follow_client_capabilities() {
    let body = "Процедура Тест()\n    Есл\nКонецПроцедуры\n";
    let (_directory, root) = support::project(body, false);
    let source = root.join("src");
    let prefix = &body[..body.find("Есл\n").unwrap() + "Есл".len()];
    for snippets in [false, true] {
        let mut client = Client::new(&root);
        let mut init = initialize();
        init["params"]["capabilities"] =
            json!({"textDocument":{"completion":{"completionItem":{"snippetSupport":snippets}}}});
        client.ask(&init);
        client.send(&notification("initialized"));
        let response = client.ask(&pull(
            &source,
            "CommonModules/Tools/Ext/Module.bsl",
            "0",
            prefix,
        ));
        let items = response["result"]["items"].as_array().unwrap();
        assert_eq!(
            items.iter().any(|item| item["insertTextFormat"] == 2),
            snippets,
            "{response}"
        );
        if !snippets {
            assert!(
                items
                    .iter()
                    .all(|item| !item["insertText"].as_str().unwrap().contains('$'))
            );
        }
        client.finish();
    }
}
