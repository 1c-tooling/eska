//! Framed external own-context requests retain current editor authority and absent-base coverage.

mod metadata;

use eska::project_core::{
    ProjectType,
    source_format::{PROJECT_SOURCE_IMPORT_RECEIPT, SourcePath},
};
use serde_json::{Value, json};
use std::{fs, path::Path};

use super::{Client, initialize, notification, request, support::TestDirectory};
#[path = "../support/external_project.rs"]
mod fixture;

/// Every request addresses the declared physical authoring file, never a guessed native descriptor.
fn uri(root: &Path, source: &SourcePath) -> String {
    url::Url::from_file_path(root.join("src").join(source.as_path()))
        .unwrap()
        .to_string()
}

/// The explicit semantic profile admits own external metadata but never a fabricated base configuration.
fn start(root: &Path, lang: &str) -> (Client, Value) {
    let mut client = Client::with_options(root, lang, &["--profile", "semantic"]);
    let opened = client.ask(&initialize());
    assert!(opened.get("error").is_none(), "{opened}");
    client.send(&notification("initialized"));
    (client, opened)
}

/// Exact completion anchors follow BOM/CRLF source lines and LSP's UTF-16 position contract.
fn completion(
    root: &Path,
    source: &SourcePath,
    snapshot: &Value,
    body: &str,
    anchor: &str,
) -> Value {
    let offset = body.find(anchor).unwrap() + anchor.len();
    positioned(
        "textDocument/completion",
        root,
        source,
        snapshot,
        body,
        offset,
    )
}

/// Typed language requests share exact UTF-16 positions and the caller's current published generation.
fn positioned(
    method: &str,
    root: &Path,
    source: &SourcePath,
    snapshot: &Value,
    body: &str,
    offset: usize,
) -> Value {
    let before = &body[..offset];
    let line = before.bytes().filter(|byte| *byte == b'\n').count();
    let character = before.rsplit('\n').next().unwrap().encode_utf16().count();
    request(
        json!(10),
        method,
        json!({"textDocument":{"uri":uri(root,source)},"position":{"line":line,"character":character},"eska":{"snapshot":snapshot}}),
    )
}

/// Own object/form field cards and YAML locations carry truthful absent-base coverage in actual frames.
#[test]
fn external_self_and_form_field_hover_and_definition_follow_canonical_current_sources() {
    for kind in [ProjectType::Processing, ProjectType::Report] {
        let directory = TestDirectory::new();
        let root = fixture::project(&directory.0, kind);
        let paths = fixture::paths(&root);
        for lang in ["ru-RU", "en-US"] {
            let (mut client, _) = start(&root, lang);
            for (source, receiver) in [
                (&paths.object_module, "ЭтотОбъект"),
                (&paths.form_module, "Объект"),
            ] {
                let text = format!(
                    "\u{feff}// Я😀\r\nПроцедура Проба()\r\n    Значение = {receiver}.Сохраненный;\r\nКонецПроцедуры\r\n"
                );
                let opened = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri(&root,source),"languageId":"bsl","version":1,"text":text}}}));
                let snapshot = opened["params"]["snapshot"].clone();
                let at = text.find("Сохраненный").unwrap();
                let methods = if lang == "ru-RU" {
                    ["textDocument/hover", "textDocument/definition"]
                } else {
                    ["textDocument/definition", "textDocument/hover"]
                };
                let mut snapshot = snapshot;
                let mut results = serde_json::Map::new();
                for method in methods {
                    let response =
                        client.ask(&positioned(method, &root, source, &snapshot, &text, at));
                    assert!(response.get("error").is_none(), "{response}");
                    snapshot = response["result"]["eska"]["snapshot"].clone();
                    results.insert(method.to_owned(), response);
                }
                let hovered = &results["textDocument/hover"];
                assert_eq!(hovered["result"]["eska"]["kind"], "field");
                assert_eq!(
                    hovered["result"]["eska"]["coverage"]["metadata_context_complete"],
                    false
                );
                assert!(
                    hovered["result"]["contents"]["value"]
                        .as_str()
                        .unwrap()
                        .contains(if lang == "ru-RU" {
                            "Булево"
                        } else {
                            "Boolean"
                        })
                );
                let target = &results["textDocument/definition"];
                assert_eq!(target["result"]["uri"], uri(&root, &paths.owner));
                assert_eq!(
                    target["result"]["eska"]["target"]["uuid"],
                    "20000000-0000-4000-8000-000000000001"
                );
                assert_eq!(
                    target["result"]["eska"]["coverage"]["metadata_context_complete"],
                    false
                );
            }
            client.finish();
        }
        assert!(!root.join(".eska").exists());
    }
}

/// Both external profiles expose actual self/form completion with truthful incomplete owning-base coverage.
#[test]
fn external_own_types_and_absent_configuration_context_are_explicit_in_both_locales() {
    for kind in [ProjectType::Processing, ProjectType::Report] {
        let directory = TestDirectory::new();
        let root = fixture::project(&directory.0, kind);
        let paths = fixture::paths(&root);
        assert!(root.join("src").join(paths.form_owner.as_path()).is_file());
        for lang in ["ru-RU", "en-US"] {
            let (mut client, opened) = start(&root, lang);
            let caps = &opened["result"]["capabilities"];
            let eska = &caps["experimental"]["eska"];
            assert_eq!(eska["analysisScope"], "canonical_external");
            assert_eq!(eska["metadataContextComplete"], false);
            assert_eq!(eska["editPlans"], false);
            assert!(caps.get("completionProvider").is_some());
            assert!(caps.get("diagnosticProvider").is_none());
            let mut snapshot = eska["snapshot"].clone();
            for (source, body, anchor) in [
                (&paths.object_module, fixture::OBJECT_BODY, "ЭтотОбъект."),
                (&paths.form_module, fixture::FORM_BODY, "Объект."),
            ] {
                let result = client.ask(&completion(&root, source, &snapshot, body, anchor));
                assert!(result.get("error").is_none(), "{result}");
                assert!(
                    result["result"]["items"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|item| item["label"] == "Сохраненный")
                );
                assert_eq!(result["result"]["isIncomplete"], true);
                assert_eq!(
                    result["result"]["eska"]["coverage"]["metadata_context_complete"],
                    false
                );
                snapshot = result["result"]["eska"]["snapshot"].clone();
            }
            let refusal = client.ask(&request(json!(30), "textDocument/diagnostic", json!({"textDocument":{"uri":uri(&root,&paths.object_module)},"identifier":"eska-semantic","eska":{"snapshot":snapshot}})));
            assert_eq!(
                refusal["error"]["data"]["kind"],
                "analysis_configuration_context_required"
            );
            assert!(!root.join(".eska").exists());
            client.finish();
        }
    }
}

/// Dirty YAML changes current self/form fields, while saved files and import-retired editor authority survive.
#[test]
fn external_own_metadata_dirty_saved_watch_and_import_guard_keep_exact_current_authority() {
    for kind in [ProjectType::Processing, ProjectType::Report] {
        let directory = TestDirectory::new();
        let root = fixture::project(&directory.0, kind);
        let paths = fixture::paths(&root);
        let physical = root.join("src").join(paths.owner.as_path());
        let saved = fs::read_to_string(&physical).unwrap();
        let dirty = saved.replace("Сохраненный", "Текущий");
        for lang in ["ru-RU", "en-US"] {
            let (mut client, _) = start(&root, lang);
            let opened = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri(&root,&paths.owner),"languageId":"yaml","version":1,"text":dirty}}}));
            assert_eq!(opened["method"], "eska/inputChanged");
            let snapshot = opened["params"]["snapshot"].clone();
            let current = client.ask(&completion(
                &root,
                &paths.object_module,
                &snapshot,
                fixture::OBJECT_BODY,
                "ЭтотОбъект.",
            ));
            assert!(
                current["result"]["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|item| item["label"] == "Текущий")
            );
            assert_eq!(fs::read_to_string(&physical).unwrap(), saved);
            let dialog = fs::read_to_string(root.join("src").join(paths.dialog.as_path())).unwrap();
            let dialog_opened = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri(&root,&paths.dialog),"languageId":"xml","version":1,"text":dialog}}}));
            assert_eq!(dialog_opened["method"], "eska/inputChanged");
            let watched = client.ask(&json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":uri(&root,&paths.dialog),"type":2}]}}));
            assert_eq!(watched["params"]["inputs"][0]["invalidated"], false);
            let marker = root.join(PROJECT_SOURCE_IMPORT_RECEIPT);
            fs::create_dir_all(marker.parent().unwrap()).unwrap();
            fs::write(&marker, b"active import").unwrap();
            let rejected = client.ask(&completion(
                &root,
                &paths.object_module,
                &watched["params"]["snapshot"],
                fixture::OBJECT_BODY,
                "ЭтотОбъект.",
            ));
            assert_eq!(
                rejected["error"]["data"]["kind"],
                "project_restart_required"
            );
            fs::remove_file(marker).unwrap();
            client.finish();
        }
        assert_eq!(fs::read_to_string(&physical).unwrap(), saved);
    }
}
