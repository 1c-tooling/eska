//! Standard syntax pulls are tested against real process input and the standalone batch profile.

use std::{fs, path::Path};

use serde_json::{Value, json};
use url::Url;

use super::{Client, initialize, notification, request, snapshot, support};

const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";

/// Standard URI and explicit ESKA context refer to one declared current module.
fn pull(root: &Path, revision: &str) -> Value {
    request(
        json!(20),
        "textDocument/diagnostic",
        json!({"textDocument":{
        "uri":Url::from_file_path(root.join("src").join(MODULE)).unwrap().to_string()},
        "identifier":"eska-syntax", "eska":{"snapshot":snapshot(revision)}}),
    )
}

/// Metadata-only initialization remains possible even if language settings are unavailable.
fn start(root: &Path, lang: &str, extra: &[&str]) -> (Client, Value) {
    let mut client = Client::with_options(root, lang, extra);
    let init = client.ask(&initialize());
    assert!(init.get("error").is_none(), "{init}");
    client.send(&notification("initialized"));
    (client, init)
}

/// Opened text uses the same document lifecycle as the metadata reader.
fn open(root: &Path, version: i32, text: &str) -> Value {
    json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{
        "uri":Url::from_file_path(root.join("src").join(MODULE)).unwrap().to_string(),
        "version":version,"languageId":"bsl","text":text}}})
}

/// Full document close restores only the actual current saved bytes.
fn close(root: &Path) -> Value {
    json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{
        "uri":Url::from_file_path(root.join("src").join(MODULE)).unwrap().to_string()}}})
}

/// Opaque report IDs and localized presentation are excluded; all machine semantics are compared.
fn machine(mut value: Value) -> Value {
    if let Some(fields) = value.get_mut("result").and_then(Value::as_object_mut) {
        fields.remove("resultId");
        if let Some(items) = fields.get_mut("items").and_then(Value::as_array_mut) {
            for item in items {
                item.as_object_mut().unwrap().remove("message");
            }
        }
    }
    value
}

/// UTF-16 ranges must independently match the canonical byte boundaries, including Unicode and CRLF.
fn position(text: &str, byte: usize) -> Value {
    assert!(text.is_char_boundary(byte));
    let prefix = &text[..byte];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count();
    let last = prefix.rsplit('\n').next().unwrap().trim_end_matches('\r');
    json!({"line":line,"character":last.encode_utf16().count()})
}

/// Saved/editor/close findings retain exact fingerprints and standard ranges in both locales.
#[test]
fn saved_editor_and_close_diagnostics_use_the_same_current_snapshot() {
    let saved = "\u{feff}// Saved 😀\r\nПроцедура Тест()\r\nКонецПроцедуры\r\n";
    let (_directory, root) = support::project(saved, false);
    let draft = "\u{feff}// Черновик 😀\r\nПроцедура Ошибка(\r\n";
    let mut reports = Vec::new();
    for lang in ["en-US", "ru-RU"] {
        let (mut client, init) = start(&root, lang, &[]);
        assert_eq!(init["result"]["capabilities"]["positionEncoding"], "utf-16");
        assert_eq!(
            init["result"]["capabilities"]["diagnosticProvider"]["identifier"],
            "eska-syntax"
        );
        let first = client.ask(&pull(&root, "0"));
        assert_eq!(first["result"]["items"], json!([]));
        assert_eq!(first["result"]["eska"]["snapshot"], snapshot("1"));
        assert_eq!(
            first["result"]["eska"]["coverage"]["module_syntax_complete"],
            true
        );
        assert_eq!(
            first["result"]["eska"]["coverage"]["metadata_models_complete"],
            false
        );
        assert_eq!(
            first["result"]["eska"]["coverage"]["semantic_complete"],
            false
        );
        assert_eq!(
            client.ask(&open(&root, 1, draft))["params"]["snapshot"],
            snapshot("2")
        );
        let editor = client.ask(&pull(&root, "2"));
        assert_ne!(
            editor["result"]["eska"]["source_sha256"],
            first["result"]["eska"]["source_sha256"]
        );
        let items = editor["result"]["items"].as_array().unwrap();
        assert!(!items.is_empty(), "{editor}");
        for item in items {
            assert_eq!(item["code"], "ParseError");
            assert_eq!(
                item["range"]["start"],
                position(
                    draft,
                    usize::try_from(item["data"]["start_byte"].as_u64().unwrap()).unwrap()
                )
            );
            assert_eq!(
                item["range"]["end"],
                position(
                    draft,
                    usize::try_from(item["data"]["end_byte"].as_u64().unwrap()).unwrap()
                )
            );
            let message = item["message"].as_str().unwrap();
            assert!(!message.is_empty());
            if lang == "en-US" {
                assert!(message.is_ascii(), "{message}");
            } else {
                assert!(!message.is_ascii(), "{message}");
            }
        }
        assert_eq!(
            client.ask(&pull(&root, "2"))["result"]["resultId"],
            editor["result"]["resultId"]
        );
        assert_eq!(
            client.ask(&pull(&root, "1"))["error"]["data"]["kind"],
            "stale_revision"
        );
        assert_eq!(
            client.ask(&close(&root))["params"]["snapshot"],
            snapshot("3")
        );
        let closed = client.ask(&pull(&root, "3"));
        assert_eq!(closed["result"]["items"], first["result"]["items"]);
        assert_eq!(
            closed["result"]["eska"]["source_sha256"],
            first["result"]["eska"]["source_sha256"]
        );
        reports.push(vec![machine(first), machine(editor), machine(closed)]);
        client.finish();
    }
    assert_eq!(reports[0], reports[1]);
    assert_eq!(
        fs::read_to_string(root.join("src").join(MODULE)).unwrap(),
        saved
    );
}

/// Untyped parser reasons use a generic English message and preserve detailed Russian output.
#[test]
fn custom_syntax_reason_is_localized_without_translating_rendered_phrases() {
    let (_directory, root) = support::project("Procedure Test()\nValue;\nEndProcedure\n", false);
    for (lang, expected) in [
        ("en-US", "Syntax error"),
        ("ru-RU", "Ожидался вызов или присваивание"),
    ] {
        let (mut client, _) = start(&root, lang, &[]);
        let report = client.ask(&pull(&root, "0"));
        let items = report["result"]["items"].as_array().unwrap();
        assert_eq!(items.len(), 1, "{report}");
        assert_eq!(items[0]["code"], "ParseError");
        assert_eq!(items[0]["message"], expected);
        client.finish();
    }
}

/// Missing, protected and refused input never produces a successful empty full report.
#[test]
fn unavailable_modules_have_current_coded_state() {
    let (_directory, root) = support::project("// Saved.\n", false);
    fs::remove_file(root.join("src").join(MODULE)).unwrap();
    let (mut client, _) = start(&root, "en-US", &[]);
    let missing = client.ask(&pull(&root, "0"));
    assert_eq!(missing["error"]["data"]["kind"], "module_unavailable");
    assert_eq!(
        missing["error"]["data"]["details"]["snapshot"],
        snapshot("1")
    );
    assert!(missing.get("result").is_none());
    assert_eq!(
        client.ask(&open(&root, 1, "// Current.\n"))["params"]["snapshot"],
        snapshot("2")
    );
    assert_eq!(client.ask(&pull(&root, "2"))["result"]["items"], json!([]));
    client.finish();
    let (_binary_directory, binary_root) = support::project("binary", true);
    let (mut binary, _) = start(&binary_root, "en-US", &[]);
    let mut binary_pull = pull(&binary_root, "0");
    binary_pull["params"]["textDocument"]["uri"] = json!(
        Url::from_file_path(binary_root.join("src/CommonModules/Tools/Ext/Module.bin"))
            .unwrap()
            .to_string()
    );
    assert_eq!(
        binary.ask(&binary_pull)["error"]["data"]["details"]["input"]["input"]["kind"],
        "binary"
    );
    binary.finish();
    let (_quota_directory, quota_root) = support::project("// Body.\n", false);
    let (mut quota, _) = start(&quota_root, "en-US", &["--max-retained-source-bytes", "0"]);
    let refused = quota.ask(&pull(&quota_root, "0"));
    assert_eq!(
        refused["error"]["data"]["details"]["input"]["input"]["failure"]["code"],
        "analysis_source_budget_exceeded"
    );
    quota.finish();
}

/// Invalid or unsupported analyzer settings are explicit, while independent metadata remains usable.
#[test]
fn settings_failure_and_disabled_profile_do_not_claim_clean_analysis() {
    let (_directory, root) = support::project("Процедура Ошибка(\n", false);
    for (config, code) in [
        (
            "[diagnostics.parameters]\nParseError = false\n",
            "analysis_profile_disabled",
        ),
        ("invalid = [\n", "analysis_config_invalid"),
        (
            "[source]\nexclude=['CommonModules']\n",
            "analysis_scope_unsupported",
        ),
    ] {
        fs::write(root.join("bsl-analyzer.toml"), config).unwrap();
        let (mut client, init) = start(&root, "en-US", &[]);
        assert_eq!(
            init["result"]["capabilities"]["diagnosticProvider"]["identifier"],
            "eska-syntax"
        );
        assert_eq!(
            init["result"]["capabilities"]["experimental"]["eska"]["analysis"]["metadataEnabled"],
            true
        );
        assert_eq!(client.ask(&pull(&root, "0"))["error"]["data"]["kind"], code);
        let tree = client.ask(&request(
            json!(21),
            "eska/metadata/owners",
            json!({"snapshot":snapshot("0"),"limit":200}),
        ));
        assert_eq!(tree["result"]["resources"]["total_source_bytes"], 0);
        assert_eq!(tree["result"]["snapshot"], snapshot("0"));
        client.finish();
    }
}

/// Unsupported profiles and stale ESKA tokens reject before the declared pending module is read.
#[test]
fn standard_pull_and_explicit_snapshot_guards_are_both_supported() {
    let (_directory, root) = support::project("Процедура Ошибка(\n", false);
    let (mut client, _) = start(&root, "en-US", &[]);
    let mut other = pull(&root, "0");
    other["params"]["identifier"] = json!("all");
    assert_eq!(
        client.ask(&other)["error"]["data"]["kind"],
        "analysis_profile_unsupported"
    );
    assert_eq!(
        client.ask(&pull(&root, "999"))["error"]["data"]["kind"],
        "stale_revision"
    );
    let mut standard = pull(&root, "0");
    standard["params"].as_object_mut().unwrap().remove("eska");
    standard["params"]
        .as_object_mut()
        .unwrap()
        .remove("identifier");
    let response = client.ask(&standard);
    assert_eq!(response["result"]["kind"], "full");
    assert_eq!(response["result"]["eska"]["snapshot"], snapshot("1"));
    assert!(!response["result"]["items"].as_array().unwrap().is_empty());
    client.finish();
}

/// The core batch checker and IDE serialize identical codes, severities and byte ranges for saved input.
#[test]
fn saved_server_and_shared_batch_syntax_findings_match() {
    use eska_analysis::{
        DiagnosticsConfig, Locale,
        batch::{BatchOptions, check_syntax},
    };
    let body = "\u{feff}Процедура Ошибка(\r\n";
    let (_directory, root) = support::project(body, false);
    let (mut client, _) = start(&root, "en-US", &[]);
    let response = client.ask(&pull(&root, "0"));
    let source = eska::project_core::source_format::SourceReader::open(&root.join("src")).unwrap();
    let batch = check_syntax(
        source,
        &DiagnosticsConfig {
            locale: Locale::En,
            ..DiagnosticsConfig::default()
        },
        &BatchOptions::default(),
    )
    .unwrap();
    let findings = &batch
        .files
        .iter()
        .find(|file| file.source.as_str() == MODULE)
        .unwrap()
        .findings;
    let server = response["result"]["items"].as_array().unwrap();
    assert_eq!(server.len(), findings.len());
    for (item, finding) in server.iter().zip(findings) {
        assert_eq!(item["code"], finding.code);
        assert_eq!(item["data"]["original_severity"], finding.severity);
        assert_eq!(item["data"]["start_byte"], finding.start_byte);
        assert_eq!(item["data"]["end_byte"], finding.end_byte);
        assert_eq!(item["message"], finding.message);
    }
    client.finish();
}

/// One server holds its negotiated settings snapshot; a restart intentionally picks up new settings.
#[test]
fn settings_changes_do_not_silently_change_a_running_profile() {
    let (_directory, root) = support::project("Процедура Ошибка(\n", false);
    let (mut client, _) = start(&root, "en-US", &[]);
    let first = client.ask(&pull(&root, "0"));
    assert!(!first["result"]["items"].as_array().unwrap().is_empty());
    fs::write(
        root.join("bsl-analyzer.toml"),
        "[diagnostics.parameters]\nParseError=false\n",
    )
    .unwrap();
    let same = client.ask(&pull(&root, "1"));
    assert_eq!(same["result"], first["result"]);
    client.finish();
    let (mut next, init) = start(&root, "en-US", &[]);
    assert_eq!(
        init["result"]["capabilities"]["diagnosticProvider"]["identifier"],
        "eska-syntax"
    );
    assert_eq!(
        next.ask(&pull(&root, "0"))["error"]["data"]["kind"],
        "analysis_profile_disabled"
    );
    next.finish();
}
