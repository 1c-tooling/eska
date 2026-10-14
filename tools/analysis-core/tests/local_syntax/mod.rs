//! Framed all-type syntax sessions retain existing editor, snapshot and import boundaries.

use eska::project_core::{
    ProjectType,
    source_format::{PROJECT_SOURCE_IMPORT_RECEIPT, SourcePath},
};
use serde_json::{Value, json};
use std::{fs, path::Path};

use super::support::TestDirectory;
use super::{Client, initialize, notification, request};
#[path = "../support/local_syntax_project.rs"]
mod fixture;

/// The real source URI comes from the declared physical module, never native filename guesses.
fn uri(root: &Path, module: &SourcePath) -> String {
    url::Url::from_file_path(root.join("src").join(module.as_path()))
        .unwrap()
        .to_string()
}

/// Open only after negotiating the same source profile and document lifecycle as the original server.
fn start(root: &Path, lang: &str, extra: &[&str]) -> (Client, Value) {
    let mut client = Client::with_options(root, lang, extra);
    let response = client.ask(&initialize());
    assert!(response.get("error").is_none(), "{response}");
    client.send(&notification("initialized"));
    (client, response)
}

/// Localized prose is the only part of syntax diagnostics omitted from machine parity comparison.
fn machine_items(response: &Value) -> Value {
    let mut items = response["result"]["items"].clone();
    for item in items.as_array_mut().unwrap() {
        item.as_object_mut().unwrap().remove("message");
    }
    items
}

/// Position-bearing semantic requests retain their actual typed payload validation.
fn semantic_request(method: &str, uri: &str) -> Value {
    let mut params = json!({"textDocument":{"uri":uri},"position":{"line":1,"character":0}});
    if method == "textDocument/references" {
        params["context"] = json!({"includeDeclaration":false});
    }
    if method == "textDocument/rename" {
        params["newName"] = json!("НовоеИмя");
    }
    request(json!(30), method, params)
}

/// Local-only capabilities and direct guards agree without reading or acknowledging dependencies.
#[test]
fn local_syntax_capabilities_and_direct_semantic_refusals_are_honest() {
    for kind in [
        ProjectType::Extension,
        ProjectType::Processing,
        ProjectType::Report,
    ] {
        let (_directory, root, module) = fixture::project(kind, fixture::GOOD, false);
        for lang in ["ru-RU", "en-US"] {
            let (mut client, response) = start(&root, lang, &[]);
            let capabilities = &response["result"]["capabilities"];
            let eska = &capabilities["experimental"]["eska"];
            assert_eq!(eska["analysisScope"], "local_syntax");
            assert_eq!(
                eska["languageMethods"],
                json!(["textDocument/diagnostic", "textDocument/documentSymbol"])
            );
            assert_eq!(eska["cancellableMethods"], eska["languageMethods"]);
            assert_eq!(eska["editPlans"], false);
            assert_eq!(eska["nestedFormModels"], false);
            assert_eq!(capabilities["documentSymbolProvider"], true);
            for name in [
                "completionProvider",
                "hoverProvider",
                "definitionProvider",
                "referencesProvider",
                "signatureHelpProvider",
                "renameProvider",
            ] {
                assert!(capabilities.get(name).is_none());
            }
            let owners = request(
                json!(10),
                "eska/metadata/owners",
                json!({"snapshot":eska["snapshot"],"limit":200}),
            );
            let before = client.ask(&owners);
            for method in [
                "textDocument/completion",
                "textDocument/definition",
                "textDocument/hover",
                "textDocument/signatureHelp",
                "textDocument/references",
                "textDocument/prepareRename",
                "textDocument/rename",
            ] {
                let refusal = client.ask(&semantic_request(method, &uri(&root, &module)));
                assert_eq!(
                    refusal["error"]["data"]["kind"], "analysis_context_required",
                    "{refusal}"
                );
            }
            assert_eq!(client.ask(&owners), before);
            client.finish();
        }
    }
}

/// Full sync, watched saved edits, stale versions and close all preserve the original authority contract.
#[test]
fn local_syntax_overlay_watch_close_and_symbols_follow_exact_current_bytes() {
    for kind in fixture::TYPES {
        let (_directory, root, module) = fixture::project(kind, fixture::GOOD, false);
        let file = root.join("src").join(module.as_path());
        let uri = uri(&root, &module);
        let mut machines = Vec::new();
        for lang in ["ru-RU", "en-US"] {
            fs::write(&file, fixture::GOOD).unwrap();
            let (mut client, _) = start(&root, lang, &[]);
            let opened = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"bsl","version":1,"text":fixture::GOOD}}}));
            assert_eq!(opened["method"], "eska/inputChanged");
            let snapshot = opened["params"]["snapshot"].clone();
            fs::write(&file, fixture::BAD).unwrap();
            let watched = client.ask(&json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":uri,"type":2}]}}));
            assert_eq!(watched["params"]["snapshot"], snapshot);
            assert_eq!(watched["params"]["inputs"][0]["invalidated"], false);
            let stale = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":1},"contentChanges":[{"text":fixture::BAD}]}}));
            assert_eq!(
                stale["params"]["error"]["data"]["kind"],
                "stale_document_version"
            );
            assert_eq!(stale["params"]["snapshot"], snapshot);
            let diagnostic = request(
                json!(20),
                "textDocument/diagnostic",
                json!({"textDocument":{"uri":uri}}),
            );
            assert_eq!(client.ask(&diagnostic)["result"]["items"], json!([]));
            let symbols = client.ask(&request(
                json!(21),
                "textDocument/documentSymbol",
                json!({"textDocument":{"uri":uri}}),
            ));
            assert_eq!(symbols["result"][0]["name"], "Проверка");
            let closed = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri}}}));
            assert_eq!(closed["method"], "eska/inputChanged");
            let checked = client.ask(&diagnostic);
            assert!(!checked["result"]["items"].as_array().unwrap().is_empty());
            assert_eq!(
                checked["result"]["eska"]["coverage"]["semantic_complete"],
                false
            );
            assert_eq!(
                checked["result"]["eska"]["analysis_scope"],
                if kind == ProjectType::Configuration {
                    "canonical_configuration"
                } else {
                    "local_syntax"
                }
            );
            machines.push(machine_items(&checked));
            client.finish();
        }
        assert_eq!(machines[0], machines[1]);
        assert_eq!(fs::read(file).unwrap(), fixture::BAD.as_bytes());
    }
}

/// Import retirement retains the last wire-confirmed input snapshot for every admitted source type.
#[test]
fn local_syntax_import_guard_rejects_later_drafts_without_advancing_snapshot() {
    for kind in fixture::TYPES {
        let (_directory, root, module) = fixture::project(kind, fixture::GOOD, false);
        for lang in ["ru-RU", "en-US"] {
            let (mut client, _) = start(&root, lang, &[]);
            let uri = uri(&root, &module);
            let opened = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"bsl","version":7,"text":fixture::BAD}}}));
            let snapshot = opened["params"]["snapshot"].clone();
            let marker = root.join(PROJECT_SOURCE_IMPORT_RECEIPT);
            fs::write(&marker, "test-owned import guard").unwrap();
            let rejected = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":8},"contentChanges":[{"text":fixture::GOOD}]}}));
            assert_eq!(rejected["method"], "eska/inputRejected");
            assert_eq!(rejected["params"]["snapshot"], snapshot);
            assert_eq!(
                rejected["params"]["error"]["data"]["kind"],
                "project_restart_required"
            );
            fs::remove_file(marker).unwrap();
            let error = client.ask(&request(
                json!(20),
                "textDocument/diagnostic",
                json!({"textDocument":{"uri":uri}}),
            ));
            assert_eq!(error["error"]["data"]["kind"], "project_restart_required");
            assert_eq!(error["error"]["data"]["details"]["snapshot"], snapshot);
            client.finish();
        }
    }
}

/// Deadline controls reject a local request before any lazy module or metadata read.
#[test]
fn local_syntax_deadline_prevents_input_reads() {
    for kind in [
        ProjectType::Extension,
        ProjectType::Processing,
        ProjectType::Report,
    ] {
        let (_directory, root, module) = fixture::project(kind, fixture::GOOD, false);
        let (mut client, response) = start(&root, "en-US", &["--timeout-seconds", "0"]);
        let owners = request(
            json!(10),
            "eska/metadata/owners",
            json!({"snapshot":response["result"]["capabilities"]["experimental"]["eska"]["snapshot"],"limit":200}),
        );
        let before = client.ask(&owners);
        let error = client.ask(&request(
            json!(20),
            "textDocument/diagnostic",
            json!({"textDocument":{"uri":uri(&root,&module)}}),
        ));
        assert_eq!(error["error"]["data"]["kind"], "analysis_deadline_exceeded");
        assert_eq!(client.ask(&owners), before);
        client.finish();
    }
}

/// Both binaries retain the same rule identities and exact byte boundaries for one saved source snapshot.
#[cfg(feature = "check-cli")]
#[test]
fn local_syntax_checker_and_lsp_findings_match_for_all_four_types() {
    for kind in fixture::TYPES {
        let (_directory, root, module) = fixture::project(kind, fixture::BAD, false);
        for lang in ["ru-RU", "en-US"] {
            let output = std::process::Command::new(env!("CARGO_BIN_EXE_eska-check"))
                .current_dir(&root)
                .env("PATH", "")
                .args(["--lang", lang, "--profile", "syntax", "--format", "json"])
                .output()
                .unwrap();
            assert_eq!(
                output.status.code(),
                Some(if kind == ProjectType::Configuration {
                    1
                } else {
                    2
                })
            );
            assert!(output.stderr.is_empty());
            let batch: Value = serde_json::from_slice(&output.stdout).unwrap();
            let batch_file = &batch["data"]["report"]["files"][0];
            let (mut client, _) = start(&root, lang, &[]);
            let pull = client.ask(&request(
                json!(20),
                "textDocument/diagnostic",
                json!({"textDocument":{"uri":uri(&root,&module)}}),
            ));
            assert_eq!(
                pull["result"]["eska"]["source_sha256"],
                batch_file["source_sha256"]
            );
            assert_eq!(
                pull["result"]["eska"]["config_sha256"],
                batch["data"]["settings_sha256"]
            );
            let items = pull["result"]["items"].as_array().unwrap();
            let findings = batch_file["findings"].as_array().unwrap();
            assert_eq!(items.len(), findings.len());
            for (item, finding) in items.iter().zip(findings) {
                assert_eq!(item["code"], finding["code"]);
                assert_eq!(item["data"]["start_byte"], finding["start_byte"]);
                assert_eq!(item["data"]["end_byte"], finding["end_byte"]);
                assert_eq!(item["data"]["original_severity"], finding["severity"]);
                for (endpoint, key) in [("start", "start_byte"), ("end", "end_byte")] {
                    let byte = usize::try_from(finding[key].as_u64().unwrap()).unwrap();
                    let prefix = &fixture::BAD[..byte];
                    let line = prefix.bytes().filter(|byte| *byte == b'\n').count();
                    let character = prefix
                        .rsplit('\n')
                        .next()
                        .unwrap()
                        .trim_end_matches('\r')
                        .encode_utf16()
                        .count();
                    assert_eq!(
                        item["range"][endpoint],
                        json!({"line":line,"character":character})
                    );
                }
            }
            client.finish();
        }
    }
}
