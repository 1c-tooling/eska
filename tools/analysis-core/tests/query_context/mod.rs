//! Current form authority and separate DCS document synchronization use the existing framed lifecycle.

use std::{fs, path::PathBuf};

use eska::project_core::{
    ProjectName,
    creation::import_query_project,
    source_format::{SourcePath, SourceReader},
};
use serde_json::{Value, json};

use super::{
    Client, initialize, notification, query_analysis_project as fixture, query_creation_fixture,
    request, support::TestDirectory,
};

const BODY: &str = "&НаКлиенте\nПроцедура SavedHandler()\nКонецПроцедуры\n&НаКлиенте\nПроцедура CurrentHandler()\nКонецПроцедуры\n";

/// Both query channels remain explicit; resource synchronization does not select a diagnostic profile.
fn start(root: &std::path::Path, lang: &str, semantic: bool, watchers: bool) -> (Client, Value) {
    let extra = if semantic {
        vec!["--profile", "semantic"]
    } else {
        vec![]
    };
    let mut client = Client::with_options(root, lang, &extra);
    let mut params = initialize();
    params["params"]["initializationOptions"]["eska"]["querySyntax"] = json!(true);
    params["params"]["initializationOptions"]["eska"]["queryAnalysis"] = json!(true);
    if watchers {
        params["params"]["capabilities"]["workspace"]["didChangeWatchedFiles"] =
            json!({"dynamicRegistration":true,"relativePatternSupport":true});
    }
    let response = client.ask(&params);
    assert!(response.get("error").is_none(), "{response}");
    client.send(&notification("initialized"));
    (client, response)
}

/// Declaration identity selects the DCS authoring file even when native and physical paths differ.
fn resource(project: &fixture::Project) -> PathBuf {
    let reader = SourceReader::open(&project.root.join("src")).unwrap();
    let native = reader
        .declared_query(&project.source)
        .unwrap()
        .native_resource;
    let source = reader
        .manifest()
        .files
        .iter()
        .find(|file| &file.path == native)
        .unwrap()
        .source_path();
    project.root.join("src").join(source.as_path())
}

/// Every acknowledged resource is the exact newer editor input, including readable malformed XML.
fn open(
    client: &mut Client,
    file: &std::path::Path,
    text: &str,
    version: i32,
    language: &str,
) -> Value {
    client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":url::Url::from_file_path(file).unwrap().as_str(),"languageId":language,"version":version,"text":text}}}))
}

/// All four project types accept only declared DCS files and preserve overlays across saved events.
#[test]
fn composition_xml_sync_is_current_versioned_and_independent_from_syntax_for_all_profiles() {
    for kind in fixture::TYPES {
        let directory = TestDirectory::new();
        let project = fixture::project(&directory.0, kind);
        let file = resource(&project);
        let saved = fs::read(&file).unwrap();
        let mut machine = vec![];
        for lang in ["ru-RU", "en-US"] {
            let (mut client, initialized) = start(&project.root, lang, false, false);
            assert_eq!(
                initialized["result"]["capabilities"]["experimental"]["eska"]["metadata"]["queryResources"],
                json!({"sync":true,"kind":"query_composition"})
            );
            let opened = open(&mut client, &file, "invalid XML", 7, "xml");
            assert_eq!(opened["method"], "eska/inputChanged", "{opened}");
            assert_eq!(opened["params"]["input"]["kind"], "query_composition");
            assert_eq!(opened["params"]["input"]["state"]["input"]["kind"], "ready");
            let uri = url::Url::from_file_path(&file).unwrap().to_string();
            let stale = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":7},"contentChanges":[{"text":"older"}]}}));
            assert_eq!(stale["method"], "eska/inputRejected");
            assert_eq!(stale["params"]["snapshot"], opened["params"]["snapshot"]);
            fs::remove_file(&file).unwrap();
            let watched = client.ask(&json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":uri,"type":3}]}}));
            assert_eq!(watched["params"]["inputs"][0]["invalidated"], false);
            let syntax = client.ask(&request(json!(30), "textDocument/diagnostic", json!({"textDocument":{"uri":url::Url::from_file_path(project.root.join("src").join(project.source.as_path())).unwrap().as_str()},"identifier":"eska-query-syntax"})));
            assert_eq!(syntax["result"]["items"], json!([]), "{syntax}");
            let closed = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri}}}));
            assert_eq!(
                closed["params"]["input"]["state"]["origin"]["kind"],
                "saved"
            );
            assert_eq!(
                closed["params"]["input"]["state"]["input"]["kind"],
                "unavailable"
            );
            assert_eq!(
                closed["params"]["input"]["state"]["input"]["failure"]["code"],
                "source_file_missing"
            );
            machine.push(json!({"opened":opened["params"],"closed":closed["params"]}));
            client.finish();
            fs::write(&file, &saved).unwrap();
        }
        assert_eq!(machine[0], machine[1]);
    }
}

/// The compact saved-input registration covers the exact DCS authoring basename.
#[test]
fn composition_resource_watch_registration_includes_declared_xml_in_both_locales() {
    let directory = TestDirectory::new();
    let project = fixture::project(&directory.0, "configuration");
    let name = resource(&project)
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    for lang in ["ru-RU", "en-US"] {
        let (mut client, _) = start(&project.root, lang, false, true);
        let registration = client.receive();
        assert!(
            registration["params"]["registrations"][1]["registerOptions"]["watchers"]
                .as_array()
                .unwrap()
                .iter()
                .any(|watcher| watcher["globPattern"]["pattern"] == format!("**/{name}"))
        );
        client.ask(&json!({"jsonrpc":"2.0","id":"eska/project-watch/1","result":null}));
        client.finish();
    }
}

/// A real declared form module lets native `UnusedLocalMethod` observe current event bindings.
fn form_project(directory: &std::path::Path) -> (PathBuf, PathBuf, PathBuf) {
    let native = query_creation_fixture::native(directory, "configuration");
    let form_module = native
        .input
        .join(native.dialog.parent().unwrap())
        .join("Form/Module.bsl");
    fs::create_dir_all(form_module.parent().unwrap()).unwrap();
    fs::write(form_module, BODY).unwrap();
    let dialog = native.input.join(&native.dialog);
    let xml = fs::read_to_string(&dialog).unwrap().replace(
        "<ReadOnly>",
        "<Events><Event name=\"OnOpen\">SavedHandler</Event></Events><ReadOnly>",
    );
    fs::write(dialog, xml).unwrap();
    let root = directory.join("project");
    import_query_project(
        &native.input,
        &root,
        ProjectName::parse("current-form".into()).unwrap(),
        None,
        &[],
    )
    .unwrap();
    fs::write(
        root.join("bsl-analyzer.toml"),
        "[diagnostics.parameters]\nUnusedLocalMethod=true\n",
    )
    .unwrap();
    let source = root.join("src");
    let module = SourcePath::parse("CommonForms/Main/Module.bsl").unwrap();
    assert!(
        SourceReader::open(&source)
            .unwrap()
            .declared_module(&module)
            .is_some()
    );
    (
        root,
        source.join(native.form_source),
        source.join(module.as_path()),
    )
}

/// Native ranges address the unchanged BSL document while a YAML body changes its referenced handler.
fn unused(client: &mut Client, module: &std::path::Path) -> Vec<String> {
    let value = client.ask(&request(json!(31), "textDocument/diagnostic", json!({"textDocument":{"uri":url::Url::from_file_path(module).unwrap().as_str()},"identifier":"eska-semantic"})));
    assert!(value.get("error").is_none(), "{value}");
    value["result"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["code"] == "UnusedLocalMethod")
        .map(|item| {
            let start = &item["range"]["start"];
            let end = &item["range"]["end"];
            let line = usize::try_from(start["line"].as_u64().unwrap()).unwrap();
            let first = usize::try_from(start["character"].as_u64().unwrap()).unwrap();
            let last = usize::try_from(end["character"].as_u64().unwrap()).unwrap();
            BODY.lines()
                .nth(line)
                .unwrap()
                .chars()
                .skip(first)
                .take(last - first)
                .collect()
        })
        .collect()
}

/// Unified YAML is the only event-binding authority; invalid overlays and close retain that contract.
#[test]
fn unified_form_owner_overlay_rebinds_native_handlers_and_close_restores_saved_body() {
    let directory = TestDirectory::new();
    let (root, form, module) = form_project(&directory.0);
    let saved = fs::read_to_string(&form).unwrap();
    let draft = saved.replace("SavedHandler", "CurrentHandler");
    assert_ne!(draft, saved);
    for lang in ["ru-RU", "en-US"] {
        let (mut client, _) = start(&root, lang, true, false);
        let ack = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":url::Url::from_file_path(&module).unwrap().as_str(),"languageId":"bsl","version":1,"text":BODY}}}));
        assert_eq!(ack["method"], "eska/inputChanged");
        assert_eq!(unused(&mut client, &module), ["CurrentHandler"]);
        let changed = open(&mut client, &form, &draft, 4, "yaml");
        assert_eq!(changed["params"]["input"]["kind"], "metadata");
        assert_eq!(
            changed["params"]["input"]["state"]["origin"]["document_version"],
            4
        );
        assert_eq!(unused(&mut client, &module), ["SavedHandler"]);
        let uri = url::Url::from_file_path(&form).unwrap().to_string();
        let invalid = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":5},"contentChanges":[{"text":"bad: ["}]}}));
        assert_eq!(invalid["method"], "eska/inputChanged");
        assert_eq!(
            invalid["params"]["input"]["state"]["input"]["kind"],
            "unavailable"
        );
        let watched = client.ask(&json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":uri,"type":2}]}}));
        assert_eq!(watched["params"]["inputs"][0]["invalidated"], false);
        let closed = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri}}}));
        assert_eq!(closed["params"]["input"]["state"]["model"], "ready");
        assert_eq!(unused(&mut client, &module), ["CurrentHandler"]);
        client.finish();
    }
    assert_eq!(fs::read_to_string(form).unwrap(), saved);
}
