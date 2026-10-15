//! Contextual query diagnostics are an independent current-root channel with IDE/batch parity.

use super::{
    Client, initialize, notification, query_analysis_project as fixture, request,
    support::TestDirectory,
};
use serde_json::{Value, json};
use std::fs;

mod consumers;

/// Full analysis is explicitly independent from the existing parse-only initialization option.
fn start(
    project: &fixture::Project,
    lang: &str,
    syntax: bool,
    analysis: bool,
    extra: &[&str],
) -> (Client, Value) {
    let mut client = Client::with_options(&project.root, lang, extra);
    let mut init = initialize();
    init["params"]["initializationOptions"]["eska"] =
        json!({"protocolVersion":"0.1","querySyntax":syntax,"queryAnalysis":analysis});
    let result = client.ask(&init);
    assert!(result.get("error").is_none(), "{result}");
    client.send(&notification("initialized"));
    (client, result)
}

/// Physical URIs retain the actual native owner and do not alias an equal path in another root.
fn uri(project: &fixture::Project, path: &eska::project_core::source_format::SourcePath) -> String {
    url::Url::from_file_path(project.root.join("src").join(path.as_path()))
        .unwrap()
        .into()
}

/// One current full replacement is acknowledged before any query result can cite it.
fn open(client: &mut Client, uri: &str, version: i64, text: &str) -> Value {
    client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"sdbl","version":version,"text":text}}}))
}

/// Explicit identifiers never silently switch the rules selected by an older client.
fn pull(client: &mut Client, uri: &str, identifier: Option<&str>) -> Value {
    let mut params = json!({"textDocument":{"uri":uri}});
    if let Some(identifier) = identifier {
        params["identifier"] = json!(identifier);
    }
    client.ask(&request(json!(20), "textDocument/diagnostic", params))
}

/// Localized prose is the only diagnostic difference allowed between these equivalent snapshots.
fn machine(value: &Value) -> Value {
    let mut items = value["result"]["items"].clone();
    for item in items.as_array_mut().unwrap() {
        item.as_object_mut().unwrap().remove("message");
    }
    items
}

/// Native metadata rules are opt-in; both old default and explicit syntax identifiers remain parse-only.
#[test]
fn query_analysis_cf_opt_in_keeps_existing_syntax_and_locale_independent_native_ranges() {
    let directory = TestDirectory::new();
    let project = fixture::project(&directory.0, "configuration");
    assert!(
        project
            .root
            .join("src")
            .join(project.catalog.as_path())
            .is_file()
    );
    let before = fixture::snapshot(&project);
    let query_uri = uri(&project, &project.source);
    let text = "// current\r\nSELECT \"Я😀\" AS Caption, Data.Absent AS Value FROM Catalog.Items AS Data\r\n";
    let mut variants = vec![];
    let mut messages = vec![];
    for lang in ["ru-RU", "en-US"] {
        let (mut client, initialized) = start(&project, lang, true, true, &[]);
        let capability =
            &initialized["result"]["capabilities"]["experimental"]["eska"]["queryAnalysis"];
        assert_eq!(capability["enabled"], true);
        assert!(
            capability["rules"]
                .as_array()
                .unwrap()
                .contains(&json!("UnknownFieldInQuery"))
        );
        let ack = open(&mut client, &query_uri, 4, text);
        assert_eq!(ack["method"], "eska/inputChanged");
        for identifier in [None, Some("eska-query-syntax")] {
            let syntax = pull(&mut client, &query_uri, identifier);
            assert_eq!(syntax["result"]["items"], json!([]), "{syntax}");
            assert_eq!(syntax["result"]["eska"]["scope"], "syntax_only");
        }
        let full = pull(&mut client, &query_uri, Some("eska-query-analysis"));
        assert!(full.get("error").is_none(), "{full}");
        let unknown = full["result"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["code"] == "UnknownFieldInQuery")
            .unwrap();
        assert_ne!(unknown["message"], "UnknownFieldInQuery (Blocker)");
        messages.push(unknown["message"].clone());
        assert_eq!(
            full["result"]["eska"]["coverage"]["metadata_complete"],
            true
        );
        assert_eq!(
            full["result"]["eska"]["coverage"]["parameters_complete"],
            false
        );
        assert_eq!(
            full["result"]["eska"]["coverage"]["consumer_complete"],
            false
        );
        assert_eq!(
            full["result"]["eska"]["coverage"]["semantic_complete"],
            false
        );
        assert_eq!(
            full["result"]["eska"]["input_revision"],
            ack["params"]["snapshot"]["revision"]
        );
        assert_eq!(full["result"]["eska"]["resources"]["module_text_bytes"], 0);
        assert!(full["result"]["eska"]["config_sha256"].as_str().is_some());
        variants.push(machine(&full));
        client.finish();
    }
    assert_ne!(messages[0], messages[1]);
    assert_eq!(variants[0], variants[1]);
    assert_eq!(fixture::snapshot(&project), before);
    let (mut old, _) = start(&project, "en-US", true, false, &[]);
    open(&mut old, &query_uri, 1, text);
    let refused = pull(&mut old, &query_uri, Some("eska-query-analysis"));
    assert_eq!(
        refused["error"]["data"]["kind"],
        "query_analysis_not_negotiated"
    );
    old.finish();
}

/// A foreign adopted input is authoritative; invalid/pending and repair never resurrect the base view.
#[test]
fn query_analysis_cfe_current_foreign_watch_close_and_root_scope_are_explicit() {
    let directory = TestDirectory::new();
    let (project, foreign, catalog) = fixture::composition(&directory.0);
    let path = foreign.join("src").join(catalog.as_path());
    let saved = fs::read_to_string(&path).unwrap();
    let foreign_uri = url::Url::from_file_path(&path).unwrap().to_string();
    let query_uri = uri(&project, &project.source);
    let text = "SELECT Data.BaseField AS Base,Data.AField AS Extension,Data.SiblingField AS Missing FROM Catalog.Items AS Data";
    for lang in ["ru-RU", "en-US"] {
        let (mut client, _) = start(&project, lang, false, true, &["--profile", "semantic"]);
        assert_eq!(
            open(&mut client, &query_uri, 1, text)["method"],
            "eska/inputChanged"
        );
        let first = pull(&mut client, &query_uri, None);
        assert!(first.get("error").is_none(), "{first}");
        let missing = first["result"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| d["code"] == "UnknownFieldInQuery")
            .count();
        assert_eq!(missing, 1, "{first}");
        assert_eq!(first["result"]["eska"]["root"], 2);
        assert!(
            first["result"]["eska"]["composition_sha256"]
                .as_str()
                .is_some()
        );
        assert_eq!(first["result"]["eska"]["resources"]["module_text_bytes"], 0);
        assert_eq!(
            open(&mut client, &foreign_uri, 1, "broken: [")["method"],
            "eska/inputChanged"
        );
        let invalid = pull(&mut client, &query_uri, None);
        assert_eq!(
            invalid["error"]["data"]["kind"],
            "analysis_composition_incomplete"
        );
        let close=client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":foreign_uri}}}));
        assert_eq!(close["method"], "eska/inputChanged");
        assert!(pull(&mut client, &query_uri, None).get("error").is_none());
        fs::write(&path, saved.replace("AField", "SavedField")).unwrap();
        let watch=client.ask(&json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":foreign_uri,"type":2}]}}));
        assert_eq!(watch["method"], "eska/inputsChanged");
        assert_eq!(watch["params"]["inputs"][0]["root"], 1);
        let changed = pull(&mut client, &query_uri, None);
        assert!(changed.get("error").is_none(), "{changed}");
        assert_eq!(
            changed["result"]["items"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|d| d["code"] == "UnknownFieldInQuery")
                .count(),
            2,
            "{changed}"
        );
        client.finish();
        fs::write(&path, &saved).unwrap();
    }
}

/// External and standalone syntax profiles refuse contextual validation while retaining query syntax.
#[test]
fn query_analysis_without_base_and_zero_deadline_never_publish_clean_findings() {
    for kind in fixture::TYPES
        .into_iter()
        .filter(|kind| *kind != "configuration")
    {
        let directory = TestDirectory::new();
        let project = fixture::project(&directory.0, kind);
        for lang in ["ru-RU", "en-US"] {
            let options = if kind == "extension" {
                vec![]
            } else {
                vec!["--profile", "semantic"]
            };
            let (mut client, initialized) = start(&project, lang, true, true, &options);
            assert_eq!(
                initialized["result"]["capabilities"]["experimental"]["eska"]["queryAnalysis"]["enabled"],
                false
            );
            let uri = uri(&project, &project.source);
            open(&mut client, &uri, 1, "SELECT 1 AS Value");
            assert_eq!(
                pull(&mut client, &uri, Some("eska-query-analysis"))["error"]["data"]["kind"],
                if kind == "extension" {
                    "analysis_context_required"
                } else {
                    "analysis_configuration_context_required"
                }
            );
            assert_eq!(
                pull(&mut client, &uri, Some("eska-query-syntax"))["result"]["items"],
                json!([])
            );
            client.finish();
        }
    }
    let directory = TestDirectory::new();
    let project = fixture::project(&directory.0, "configuration");
    let (mut client, _) = start(&project, "en-US", false, true, &["--timeout-seconds", "0"]);
    let result = pull(&mut client, &uri(&project, &project.source), None);
    assert_eq!(
        result["error"]["data"]["kind"],
        "analysis_deadline_exceeded"
    );
    client.finish();
}

/// Malformed negotiation and disabled rules refuse before query reads in either locale.
#[test]
fn query_analysis_negotiation_and_disabled_rule_preflight_do_not_read_inputs() {
    let directory = TestDirectory::new();
    let project = fixture::project(&directory.0, "configuration");
    let config = ide::sdbl_query_codes().into_iter().fold(
        "[diagnostics.parameters]\n".to_owned(),
        |mut text, code| {
            text.push_str(code.as_str());
            text.push_str("=false\n");
            text
        },
    );
    fs::write(project.root.join("bsl-analyzer.toml"), config).unwrap();
    let mut errors = vec![];
    for lang in ["ru-RU", "en-US"] {
        let mut client = Client::with_options(&project.root, lang, &[]);
        let mut invalid = initialize();
        invalid["params"]["initializationOptions"]["eska"]["queryAnalysis"] = json!("true");
        let refused = client.ask(&invalid);
        assert_eq!(refused["error"]["data"]["kind"], "rpc_invalid_params");
        let mut initialize = initialize();
        initialize["params"]["initializationOptions"]["eska"]["queryAnalysis"] = json!(true);
        let admitted = client.ask(&initialize);
        assert_eq!(
            admitted["result"]["capabilities"]["experimental"]["eska"]["queryAnalysis"]["enabled"],
            false
        );
        client.send(&notification("initialized"));
        let disabled = pull(&mut client, &uri(&project, &project.source), None);
        assert_eq!(
            disabled["error"]["data"]["kind"],
            "analysis_profile_disabled"
        );
        errors.push(disabled["error"].clone());
        let state = client.ask(&request(
            json!(21),
            "eska/metadata/owners",
            json!({"snapshot":{"session_id":"session-1","revision":"0"},"limit":200}),
        ));
        assert_eq!(state["result"]["snapshot"]["revision"], "0", "{state}");
        assert_eq!(state["result"]["resources"]["total_source_bytes"], 0);
        client.finish();
    }
    assert_eq!(errors[0], errors[1]);
}

/// Context preflight and native cancellation retain the current draft and allow request ID reuse.
#[test]
fn query_analysis_stale_snapshot_and_cancelled_current_draft_never_publish_findings() {
    let directory = TestDirectory::new();
    let project = fixture::project(&directory.0, "configuration");
    let query_uri = uri(&project, &project.source);
    for lang in ["ru-RU", "en-US"] {
        let (mut client, _) = start(&project, lang, false, true, &[]);
        let opened = open(&mut client, &query_uri, 1, "SELECT 1 AS Value");
        let stale = client.ask(&request(
            json!(80),
            "textDocument/diagnostic",
            json!({"textDocument":{"uri":query_uri},"eska":{"snapshot":{"session_id":"session-1","revision":"0"}}}),
        ));
        assert_eq!(stale["error"]["data"]["kind"], "stale_revision");
        assert_eq!(
            stale["error"]["data"]["details"],
            opened["params"]["snapshot"]
        );
        assert_eq!(
            pull(&mut client, &query_uri, None)["result"]["items"],
            json!([])
        );
        let large = format!(
            "{}SELECT 1 AS Value",
            "// Я😀 current query\r\n".repeat(80_000)
        );
        let opened = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":query_uri,"version":2},"contentChanges":[{"text":large}]}}));
        assert_eq!(opened["method"], "eska/inputChanged");
        assert_eq!(
            opened["params"]["resources"]["query_text_bytes"],
            large.len()
        );
        client.send(&request(
            json!(80),
            "textDocument/diagnostic",
            json!({"textDocument":{"uri":query_uri}}),
        ));
        client.send(&json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":80}}));
        let cancelled = client.receive();
        assert_eq!(cancelled["error"]["code"], -32800, "{cancelled}");
        assert_eq!(
            cancelled["error"]["data"]["details"],
            opened["params"]["snapshot"]
        );
        client.send(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":query_uri,"version":3},"contentChanges":[{"text":"SELECT 2 AS Value"}]}}));
        let changed = client.receive();
        let recovered = client.ask(&request(
            json!(80),
            "textDocument/diagnostic",
            json!({"textDocument":{"uri":query_uri}}),
        ));
        assert_eq!(recovered["result"]["items"], json!([]), "{recovered}");
        assert_eq!(
            recovered["result"]["eska"]["source_sha256"],
            changed["params"]["input"]["state"]["source_sha256"]
        );
        assert_eq!(
            recovered["result"]["eska"]["resources"]["module_text_bytes"],
            0
        );
        client.finish();
    }
}

/// Saved IDE and semantic checker select the same native contextual rules and exact query bytes.
#[cfg(feature = "check-cli")]
#[test]
fn query_analysis_saved_checker_matches_ide_for_cf_cfe_and_both_locales() {
    for composed in [false, true] {
        let directory = TestDirectory::new();
        let project = if composed {
            fixture::composition(&directory.0).0
        } else {
            fixture::project(&directory.0, "configuration")
        };
        let text = "SELECT Data.Missing AS Value FROM Catalog.Items AS Data";
        fs::write(
            project.root.join("src").join(project.source.as_path()),
            text,
        )
        .unwrap();
        let before = fixture::snapshot(&project);
        let mut machine_values = vec![];
        for lang in ["ru-RU", "en-US"] {
            let (mut client, _) = start(&project, lang, false, true, &["--profile", "semantic"]);
            let ide = pull(&mut client, &uri(&project, &project.source), None);
            assert!(ide.get("error").is_none(), "{ide}");
            client.finish();
            let output = std::process::Command::new(env!("CARGO_BIN_EXE_eska-check"))
                .current_dir(&project.root)
                .env("PATH", "")
                .args(["--lang", lang, "--profile", "semantic", "--format", "json"])
                .output()
                .unwrap();
            assert_eq!(
                output.status.code(),
                Some(2),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(output.stderr.is_empty());
            let report: Value = serde_json::from_slice(&output.stdout).unwrap();
            let report = &report["data"]["report"];
            let file = report["query_files"]
                .as_array()
                .unwrap()
                .iter()
                .find(|file| file["source"] == project.source.as_str())
                .unwrap();
            assert_eq!(file["scope"], "current_canonical_metadata");
            assert_eq!(file["context"]["metadata_complete"], true, "{report}");
            assert_eq!(report["query_coverage"]["consumer_complete"], false);
            assert_eq!(
                file["source_sha256"],
                ide["result"]["eska"]["source_sha256"]
            );
            let findings = file["findings"].as_array().unwrap();
            let items = ide["result"]["items"].as_array().unwrap();
            assert_eq!(findings.len(), items.len(), "{report}\n{ide}");
            for (finding, item) in findings.iter().zip(items) {
                assert_eq!(finding["code"], item["code"]);
                assert_eq!(finding["range"], item["range"]);
            }
            machine_values.push(report.clone());
        }
        assert_eq!(machine_values[0], machine_values[1]);
        assert_eq!(fixture::snapshot(&project), before);
    }
}
