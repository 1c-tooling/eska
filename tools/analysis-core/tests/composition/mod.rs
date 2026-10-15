//! Framed composition retains exact current contributor authority, paths and import retirement.

use std::{fs, path::Path};

use serde_json::{Value, json};

use super::{Client, initialize, notification, support};
use support::composition_project as fixture;

/// These URIs address the verified physical root, never a relative-path alias in another member.
fn uri(root: &Path, path: &eska::project_core::source_format::SourcePath) -> String {
    url::Url::from_file_path(root.join("src").join(path.as_path()))
        .unwrap()
        .into()
}

/// The actual selected CFE receives a canonical CF/CFE context only from explicit source settings.
fn start(project: &fixture::CompositionProject, lang: &str) -> Client {
    let mut client = Client::with_options(&project.roots["b"], lang, &["--profile", "semantic"]);
    let result = client.ask(&initialize());
    assert!(result.get("error").is_none(), "{result}");
    let composition = &result["result"]["capabilities"]["experimental"]["eska"]["composition"];
    assert_eq!(composition["selected"], 2);
    assert_eq!(composition["current_complete"], false);
    assert_eq!(composition["roots"].as_array().unwrap().len(), 4);
    client.send(&notification("initialized"));
    client
}

/// The same selected caller exercises native inferred metadata fields after all current contributions load.
fn completion(client: &mut Client, project: &fixture::CompositionProject) -> Value {
    client.ask(&json!({"jsonrpc":"2.0","id":50,"method":"textDocument/completion","params":{
        "textDocument":{"uri":uri(&project.roots["b"],&project.module)},"position":{"line":2,"character":10}}}))
}

/// Standard completion shape stays unchanged while its native view excludes unrelated siblings.
fn labels(value: &Value) -> Vec<&str> {
    value["result"]["items"]
        .as_array()
        .unwrap_or_else(|| panic!("{value}"))
        .iter()
        .map(|item| item["label"].as_str().unwrap())
        .collect()
}

/// Editor authority in a dependency invalidates native inference without changing saved bytes.
#[test]
fn composition_framed_current_adoption_same_paths_and_late_sibling_import_are_isolated() {
    let directory = support::TestDirectory::new();
    let project = fixture::project(&directory.0);
    fixture::parse_only(&project);
    let catalog = project.roots["a"]
        .join("src")
        .join(project.catalog.as_path());
    let saved = fs::read_to_string(&catalog).unwrap();
    let draft = saved.replace("AField", "DirtyField");
    let receipt = project.roots["sibling"]
        .join(eska::project_core::source_format::PROJECT_SOURCE_IMPORT_RECEIPT);
    for lang in ["ru-RU", "en-US"] {
        let mut client = start(&project, lang);
        let first = completion(&mut client, &project);
        let names = labels(&first);
        assert!(
            names.contains(&"BaseField") && names.contains(&"AField"),
            "{first}"
        );
        assert!(!names.contains(&"SiblingField"));
        let opened = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{
            "uri":uri(&project.roots["a"],&project.catalog),"languageId":"eska-source","version":4,"text":draft}}}));
        assert_eq!(opened["method"], "eska/inputChanged");
        assert_eq!(opened["params"]["root"], 1);
        let current = completion(&mut client, &project);
        assert!(labels(&current).contains(&"DirtyField"), "{current}");
        assert!(!labels(&current).contains(&"AField"));
        let invalid = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
            "textDocument":{"uri":uri(&project.roots["a"],&project.catalog),"version":5},"contentChanges":[{"text":"broken: ["}]}}));
        assert_eq!(invalid["method"], "eska/inputChanged");
        let refused = completion(&mut client, &project);
        assert_eq!(
            refused["error"]["data"]["kind"],
            "analysis_composition_incomplete"
        );
        assert_eq!(refused["error"]["data"]["details"]["source"]["root"], 1);
        fs::write(&receipt, b"owned active import receipt").unwrap();
        let retired = completion(&mut client, &project);
        assert_eq!(retired["error"]["data"]["kind"], "project_restart_required");
        fs::remove_file(&receipt).unwrap();
        client.finish();
        assert_eq!(fs::read_to_string(&catalog).unwrap(), saved);
        let mut reopened = start(&project, lang);
        let restored = completion(&mut reopened, &project);
        assert!(labels(&restored).contains(&"AField"));
        assert!(!labels(&restored).contains(&"DirtyField"));
        reopened.finish();
    }
}

/// A saved dependency watch invalidates the exact root and reloads only its new current contribution.
#[test]
fn composition_framed_saved_dependency_and_metadata_definition_keep_root_identity() {
    let directory = support::TestDirectory::new();
    let project = fixture::project(&directory.0);
    fixture::parse_only(&project);
    let catalog = project.roots["a"]
        .join("src")
        .join(project.catalog.as_path());
    let saved = fs::read_to_string(&catalog).unwrap();
    let mut client = start(&project, "en-US");
    assert!(labels(&completion(&mut client, &project)).contains(&"AField"));
    fs::write(&catalog, saved.replace("AField", "SavedField")).unwrap();
    let changed = client.ask(
        &json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{
        "uri":uri(&project.roots["a"],&project.catalog),"type":2}]}}),
    );
    assert_eq!(changed["method"], "eska/inputsChanged");
    assert_eq!(changed["params"]["inputs"][0]["root"], 1);
    assert_eq!(changed["params"]["inputs"][0]["invalidated"], true);
    let current = completion(&mut client, &project);
    assert!(labels(&current).contains(&"SavedField"));
    assert!(!labels(&current).contains(&"AField"));
    let body = fixture::CALLER.replace("Value.\n", "Value.BaseField;\n");
    let opened = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{
        "uri":uri(&project.roots["b"],&project.module),"languageId":"bsl","version":1,"text":body}}}));
    assert_eq!(opened["params"]["root"], 2);
    let definition = client.ask(&json!({"jsonrpc":"2.0","id":72,"method":"textDocument/definition","params":{
        "textDocument":{"uri":uri(&project.roots["b"],&project.module)},"position":{"line":2,"character":12}}}));
    assert_eq!(
        definition["result"]["eska"]["target"]["root"], 0,
        "{definition}"
    );
    assert_eq!(
        definition["result"]["uri"],
        uri(&project.roots["base"], &project.catalog)
    );
    client.finish();
}

/// A foreign member's workspace settings are actual selected controls, not inferred source ancestry.
#[test]
fn composition_foreign_workspace_control_retires_the_host_without_discarding_drafts() {
    let directory = support::TestDirectory::new();
    let mut project = fixture::project(&directory.0);
    fixture::parse_only(&project);
    let foreign = directory.0.join("foreign");
    fs::create_dir(&foreign).unwrap();
    let base = foreign.join("base");
    fs::rename(&project.roots["base"], &base).unwrap();
    project.roots.insert("base", base);
    let control = foreign.join("eska.toml");
    fs::write(&control, "[workspace]\nmembers=['base']\n").unwrap();
    let settings = fs::read_to_string(&project.settings)
        .unwrap()
        .replace("../base/src", "../foreign/base/src");
    fs::write(&project.settings, settings).unwrap();
    for lang in ["ru-RU", "en-US"] {
        let mut client =
            Client::with_options(&project.roots["b"], lang, &["--profile", "semantic"]);
        let initialized = client.ask(&initialize());
        assert!(initialized.get("error").is_none(), "{initialized}");
        let control_uri = url::Url::from_file_path(&control).unwrap();
        assert!(initialized["result"]["capabilities"]["experimental"]["eska"]["projectChanges"]["inputs"].as_array().unwrap().iter().any(|input| input["uri"] == control_uri.as_str()), "{initialized}");
        client.send(&notification("initialized"));
        let opened = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{
            "uri":uri(&project.roots["b"],&project.module),"languageId":"bsl","version":1,"text":fixture::CALLER}}}));
        assert_eq!(opened["method"], "eska/inputChanged");
        let retired = client.ask(&json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{
            "uri":control_uri.as_str(),"type":2}]}}));
        assert_eq!(retired["method"], "eska/restartRequired");
        assert_eq!(retired["params"]["snapshot"], opened["params"]["snapshot"]);
        let refused = completion(&mut client, &project);
        assert_eq!(refused["error"]["data"]["kind"], "project_restart_required");
        client.finish();
    }
}

/// A first semantic navigation request prepares visible current inputs without an unrelated warm-up.
#[test]
fn composition_cold_definition_and_references_prepare_visible_contributions_after_invalidation() {
    let directory = support::TestDirectory::new();
    let project = fixture::project(&directory.0);
    fixture::parse_only(&project);
    let caller = "Procedure Run() Export\n    Tools.BaseMethod();\nEndProcedure\n";
    for name in ["a", "b", "sibling"] {
        fs::write(
            project.roots[name]
                .join("src")
                .join(project.module.as_path()),
            caller,
        )
        .unwrap();
    }
    for method in ["textDocument/definition", "textDocument/references"] {
        for lang in ["ru-RU", "en-US"] {
            let mut client = start(&project, lang);
            for invalidated in [false, true] {
                if invalidated {
                    let changed = client.ask(&json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{
                        "uri":uri(&project.roots["a"],&project.catalog),"type":2}]}}));
                    assert_eq!(changed["method"], "eska/inputsChanged");
                    assert_eq!(changed["params"]["inputs"][0]["root"], 1);
                }
                let mut params = json!({"textDocument":{"uri":uri(&project.roots["b"],&project.module)},"position":{"line":1,"character":15}});
                if method == "textDocument/references" {
                    params["context"] = json!({"includeDeclaration":true});
                }
                let result =
                    client.ask(&json!({"jsonrpc":"2.0","id":80,"method":method,"params":params}));
                assert!(result.get("error").is_none(), "{method}: {result}");
                if method == "textDocument/definition" {
                    assert_eq!(
                        result["result"]["uri"],
                        uri(&project.roots["base"], &project.module),
                        "{result}"
                    );
                    assert_eq!(result["result"]["eska"]["target"]["root"], 0);
                } else {
                    let hits = result["result"].as_array().unwrap();
                    let roots = hits
                        .iter()
                        .map(|hit| hit["eska"]["target"]["root"].as_u64().unwrap())
                        .collect::<std::collections::BTreeSet<_>>();
                    assert_eq!(
                        roots,
                        std::collections::BTreeSet::from([0, 1, 2]),
                        "{result}"
                    );
                    assert_eq!(hits.len(), 3, "{result}");
                    assert!(hits.iter().all(
                        |hit| hit["eska"]["coverage"]["metadata_models_complete"] == true
                            && hit["eska"]["coverage"]["module_inputs_complete"] == true
                    ));
                }
            }
            client.finish();
        }
    }
}

/// A versioned local plan can be the first semantic operation in a configured selected member.
#[test]
fn composition_cold_local_rename_prepares_context_under_the_same_host_request() {
    let directory = support::TestDirectory::new();
    let project = fixture::project(&directory.0);
    fixture::parse_only(&project);
    let text = "Procedure Run(Argument) Export\n    Message(Argument);\nEndProcedure\n";
    for lang in ["ru-RU", "en-US"] {
        let mut client = Client::with_options(
            &project.roots["b"],
            lang,
            &[
                "--profile",
                "semantic",
                "--accepted-base-sha256",
                &project.selected_accepted_sha256,
            ],
        );
        let mut init = initialize();
        init["params"]["capabilities"] = json!({"workspace":{"workspaceEdit":{"documentChanges":true}},"textDocument":{"rename":{"prepareSupport":true}}});
        let result = client.ask(&init);
        assert!(result.get("error").is_none(), "{result}");
        client.send(&notification("initialized"));
        let opened = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{
            "uri":uri(&project.roots["b"],&project.module),"languageId":"bsl","version":4,"text":text}}}));
        assert_eq!(opened["method"], "eska/inputChanged");
        let plan = client.ask(&json!({"jsonrpc":"2.0","id":95,"method":"textDocument/rename","params":{
            "textDocument":{"uri":uri(&project.roots["b"],&project.module)},"position":{"line":0,"character":15},"newName":"Current"}}));
        assert_eq!(
            plan["result"]["documentChanges"][0]["textDocument"]["version"], 4,
            "{plan}"
        );
        assert_eq!(
            plan["result"]["documentChanges"][0]["edits"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            plan["result"]["eska"]["coverage"]["metadata_models_complete"],
            true
        );
        client.finish();
    }
}
