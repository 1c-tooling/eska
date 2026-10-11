//! Semantic pulls preserve editor authority, dependency inputs and the checker rule identity.

mod controls;

use super::{Client, initialize, notification, request, snapshot, support};
use eska_analysis::DiagnosticCode;
use serde_json::{Value, json};
use std::{fs, path::Path};

#[path = "../support/semantic_project.rs"]
mod fixture;

const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";
const DEPENDENCY: &str = "CommonModules/ZDependency/Ext/Module.bsl";
const OWNER: &str = "CommonModules/Tools/object.yaml";
const BODY: &str = "\u{feff}// Current 😀\r\nProcedure Run()\r\n Value = \"😀\"; Value = Value;\r\nEndProcedure\r\n";

/// Select only actual kernel codes rather than relying on changing default rule activation.
fn settings(root: &Path, enabled: &[DiagnosticCode]) {
    use std::fmt::Write as _;
    let text = ide::all_diagnostic_codes().fold(
        "[diagnostics.parameters]\n".into(),
        |mut text: String, code| {
            writeln!(text, "{}={}", code.as_str(), enabled.contains(&code)).unwrap();
            text
        },
    );
    fs::write(root.join("bsl-analyzer.toml"), text).unwrap();
}

/// Freeze a process profile before lazy input reads and initialize the existing protocol normally.
fn start(root: &Path, lang: &str, extra: &[&str]) -> (Client, Value) {
    let mut args = vec!["--profile", "semantic"];
    args.extend_from_slice(extra);
    let mut client = Client::with_options(root, lang, &args);
    let value = client.ask(&initialize());
    assert!(value.get("error").is_none(), "{value}");
    client.send(&notification("initialized"));
    (client, value)
}

/// Resolve only the owned fixture's exact selected source path.
fn uri(source: &Path, path: &str) -> String {
    url::Url::from_file_path(source.join(path))
        .unwrap()
        .to_string()
}

/// Standard clients can pull the selected profile without requiring an ESKA snapshot extension.
fn pull(source: &Path, path: &str) -> Value {
    request(
        json!(20),
        "textDocument/diagnostic",
        json!({"textDocument":{"uri":uri(source,path)},"identifier":"eska-semantic"}),
    )
}

/// Editor-owned YAML and BSL use the same existing full-sync lifecycle.
fn open(source: &Path, path: &str, version: i32, text: &str) -> Value {
    let bsl = Path::new(path)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("bsl"));
    json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{
        "uri":uri(source,path),"version":version,"languageId":if bsl {"bsl"} else {"yaml"},"text":text}}})
}

/// Incremental document versions carry one complete replacement, as negotiated by the server.
fn change(source: &Path, path: &str, version: i32, text: &str) -> Value {
    json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri(source,path),"version":version},"contentChanges":[{"text":text}]}})
}

/// Closing restores the current saved body, not the last editor snapshot.
fn close(source: &Path, path: &str) -> Value {
    json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri(source,path)}}})
}

/// Remove only opaque IDs so both locale machine semantics can be compared without hiding findings.
fn machine(mut value: Value) -> Value {
    value["result"].as_object_mut().unwrap().remove("resultId");
    value
}

/// Independently derive UTF-16 positions from raw bytes, including non-BMP characters before a finding.
#[cfg(feature = "check-cli")]
fn position(text: &str, byte: u64) -> Value {
    let prefix = &text[..usize::try_from(byte).unwrap()];
    json!({"line":prefix.bytes().filter(|byte| *byte==b'\n').count(),
        "character":prefix.rsplit('\n').next().unwrap().trim_end_matches('\r').encode_utf16().count()})
}

/// The two real processes preserve selected settings, source hash, native byte spans and severities.
#[cfg(feature = "check-cli")]
#[test]
fn semantic_saved_pulls_match_checker_findings_and_profile_hash() {
    let (_directory, root) = support::project(BODY, false);
    let source = root.join("src");
    settings(&root, &[DiagnosticCode::SelfAssign]);
    for lang in ["ru-RU", "en-US"] {
        let (mut client, _) = start(&root, lang, &[]);
        let ide = client.ask(&pull(&source, MODULE));
        let run = std::process::Command::new(env!("CARGO_BIN_EXE_eska-check"))
            .current_dir(&root)
            .env("PATH", "")
            .args(["--profile", "semantic", "--format", "json", "--lang", lang])
            .output()
            .unwrap();
        assert_eq!(run.status.code(), Some(1));
        assert!(run.stderr.is_empty());
        let batch: Value = serde_json::from_slice(&run.stdout).unwrap();
        let file = &batch["data"]["report"]["files"][0];
        assert_eq!(
            ide["result"]["eska"]["config_sha256"],
            batch["data"]["settings_sha256"]
        );
        assert_eq!(
            ide["result"]["eska"]["source_sha256"],
            file["source_sha256"]
        );
        assert_eq!(
            ide["result"]["eska"]["coverage"]["semantic_profile_complete"],
            batch["data"]["report"]["coverage"]["semantic_profile_complete"]
        );
        let items = ide["result"]["items"].as_array().unwrap();
        let findings = file["findings"].as_array().unwrap();
        assert_eq!(items.len(), findings.len());
        assert_eq!(items.len(), 1);
        for (item, finding) in items.iter().zip(findings) {
            assert_eq!(item["code"], finding["code"]);
            assert_eq!(item["data"]["original_severity"], finding["severity"]);
            for side in ["start", "end"] {
                let byte = finding[format!("{side}_byte")].as_u64().unwrap();
                assert_eq!(item["data"][format!("{side}_byte")], byte);
                assert_eq!(item["range"][side], position(BODY, byte));
            }
        }
        client.finish();
    }
}

/// Unsaved BSL and YAML affect the current report; pulls never replace their editor versions.
#[test]
fn semantic_editor_and_metadata_inputs_remain_current_until_close() {
    let (_directory, root) = support::project(BODY, false);
    let source = root.join("src");
    settings(&root, &[DiagnosticCode::SelfAssign]);
    let saved = fs::read(source.join(MODULE)).unwrap();
    let yaml = fs::read_to_string(source.join(OWNER)).unwrap();
    let mut values = vec![];
    for lang in ["ru-RU", "en-US"] {
        let (mut client, init) = start(&root, lang, &[]);
        let provider = &init["result"]["capabilities"]["diagnosticProvider"];
        assert_eq!(provider["identifier"], "eska-semantic");
        assert_eq!(provider["interFileDependencies"], true);
        let analysis = &init["result"]["capabilities"]["experimental"]["eska"]["analysis"];
        assert_eq!(analysis["semanticEnabled"], true);
        assert_eq!(analysis["syntaxEnabled"], false);
        let first = client.ask(&pull(&source, MODULE));
        let data = &first["result"];
        assert_eq!(data["items"].as_array().unwrap().len(), 1);
        assert_eq!(data["items"][0]["code"], "SelfAssign");
        assert_eq!(data["eska"]["coverage"]["semantic_profile_complete"], true);
        assert_eq!(data["eska"]["coverage"]["module_syntax_complete"], false);
        assert_eq!(data["eska"]["coverage"]["semantic_complete"], false);
        let clean = "// Current editor.\nProcedure Run()\n Value = 2;\nEndProcedure\n";
        assert_eq!(
            client.ask(&open(&source, MODULE, 7, clean))["method"],
            "eska/inputChanged"
        );
        let editor = client.ask(&pull(&source, MODULE));
        assert_eq!(editor["result"]["items"], json!([]));
        assert_ne!(
            editor["result"]["eska"]["source_sha256"],
            data["eska"]["source_sha256"]
        );
        assert_eq!(client.ask(&pull(&source, MODULE)), editor);
        let invalid = support::property::server_value(&yaml, "Future");
        assert_ne!(invalid, yaml);
        client.ask(&open(&source, OWNER, 1, &invalid));
        let partial = client.ask(&pull(&source, MODULE));
        assert_eq!(
            partial["result"]["eska"]["coverage"]["semantic_profile_complete"],
            false
        );
        assert_eq!(
            partial["result"]["eska"]["coverage"]["metadata_models_complete"],
            false
        );
        assert_eq!(partial["result"]["items"], json!([]));
        client.ask(&change(&source, OWNER, 2, &yaml));
        assert_eq!(
            client.ask(&pull(&source, MODULE))["result"]["eska"]["coverage"]["semantic_profile_complete"],
            true
        );
        client.ask(&close(&source, MODULE));
        let restored = client.ask(&pull(&source, MODULE));
        assert_eq!(restored["result"]["items"], data["items"]);
        assert_eq!(
            restored["result"]["eska"]["source_sha256"],
            data["eska"]["source_sha256"]
        );
        client.ask(&close(&source, OWNER));
        values.push(vec![
            machine(first),
            machine(editor),
            machine(partial),
            machine(restored),
        ]);
        client.finish();
    }
    assert_eq!(values[0], values[1]);
    assert_eq!(fs::read(source.join(MODULE)).unwrap(), saved);
    assert_eq!(fs::read_to_string(source.join(OWNER)).unwrap(), yaml);
}

/// Export availability changes with a neighbor editor; a removed saved body yields explicit partial coverage.
#[test]
fn semantic_neighbors_and_saved_invalidations_recompute_the_same_caller() {
    let (directory, root) = support::project(
        "Procedure Run()\n ZDependency.Available();\n ZDependency.Missing();\nEndProcedure\n",
        false,
    );
    fixture::add_dependency(
        &root,
        &directory.0.join("native"),
        "Procedure Available() Export\nEndProcedure\n",
    );
    let source = root.join("src-dependent");
    settings(&root, &[DiagnosticCode::UnresolvedMethodCall]);
    let saved = fs::read(source.join(DEPENDENCY)).unwrap();
    for lang in ["ru-RU", "en-US"] {
        let (mut client, _) = start(&root, lang, &[]);
        let first = client.ask(&pull(&source, MODULE));
        assert_eq!(first["result"]["items"].as_array().unwrap().len(), 1);
        client.ask(&open(&source,DEPENDENCY,1,"Procedure Available() Export\nEndProcedure\nProcedure Missing() Export\nEndProcedure\n"));
        let editor = client.ask(&pull(&source, MODULE));
        assert_eq!(editor["result"]["items"], json!([]));
        client.ask(&close(&source, DEPENDENCY));
        assert_eq!(
            client.ask(&pull(&source, MODULE))["result"]["items"],
            first["result"]["items"]
        );
        fs::remove_file(source.join(DEPENDENCY)).unwrap();
        let event = json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":uri(&source,DEPENDENCY),"type":3}]}});
        assert_eq!(client.ask(&event)["method"], "eska/inputsChanged");
        let partial = client.ask(&pull(&source, MODULE));
        assert_eq!(partial["result"]["items"], json!([]));
        assert_eq!(
            partial["result"]["eska"]["coverage"]["module_inputs_complete"],
            false
        );
        assert_eq!(
            partial["result"]["eska"]["coverage"]["semantic_profile_complete"],
            false
        );
        fs::write(source.join(DEPENDENCY), &saved).unwrap();
        client.ask(&json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":uri(&source,DEPENDENCY),"type":1}]}}));
        assert_eq!(
            client.ask(&pull(&source, MODULE))["result"]["items"],
            first["result"]["items"]
        );
        client.finish();
    }
    assert_eq!(fs::read(source.join(DEPENDENCY)).unwrap(), saved);
}
