//! Guarded source replacement requires a fresh host while the client retains its exact dirty buffers.

use super::*;
use eska::project_core::{
    discovery::discover,
    metadata_model::ProjectScope,
    source_format::{
        AcceptedRef, ImportAncestor, PROJECT_SOURCE_IMPORT_RECEIPT, SourceImportRecovery,
        accept_source, capture_import_ancestor, import_designer, prepare_project_source_import,
        recover_project_source_import,
    },
};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

/// Hash exact editor bytes independently, including BOM, CRLF and non-BMP characters.
fn hash(text: &str) -> String {
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(text.as_bytes()) {
        write!(output, "{byte:02x}").unwrap();
    }
    output
}

/// Full-sync editor bytes belong to the client and receive an acknowledgement only after acceptance.
fn open(client: &mut Client, root: &Path, path: &str, version: i32, text: &str) -> Value {
    let uri = Url::from_file_path(root.join("src").join(path)).unwrap();
    let language = if Path::new(path)
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("bsl"))
    {
        "bsl"
    } else {
        "yaml"
    };
    client.ask(
        &json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri.as_str(),"languageId":language,
            "version":version,"text":text}}}),
    )
}

/// A new host acknowledges the retained editor version and its independently hashed exact bytes.
fn assert_overlay(event: &Value, text: &str, version: i32) {
    assert_eq!(event["method"], "eska/inputChanged");
    let state = &event["params"]["input"]["state"];
    assert_eq!(state["source_sha256"], hash(text));
    assert_eq!(
        state["origin"],
        json!({"kind":"overlay","document_version":version})
    );
}

/// New processes also refuse incomplete journals; cold start cannot undo old-host retirement.
fn cold_start_refused(root: &Path, lang: &str) {
    let (output, responses) = session(root, lang, &[], &[initialize()]);
    assert!(!output.status.success());
    assert_eq!(responses.len(), 1);
    assert_eq!(
        responses[0]["error"]["data"]["kind"],
        "source_import_incomplete"
    );
}

/// Source directory moves may deliver root or manifest deletion before the advertised guard event.
#[test]
fn active_guard_survives_root_and_manifest_only_watcher_batches_in_both_locales() {
    for controls in [
        vec![("src/configuration.yaml", "root_configuration")],
        vec![("src/source.json", "source_manifest")],
        vec![
            ("src/configuration.yaml", "root_configuration"),
            ("src/source.json", "source_manifest"),
        ],
    ] {
        for lang in ["ru-RU", "en-US"] {
            let (_directory, root) = support::project("// Saved body.\r\n", false);
            let (mut client, _) = start(&root, lang);
            let ready = read(&mut client, "0");
            let guard = root.join(PROJECT_SOURCE_IMPORT_RECEIPT);
            fs::write(&guard, "active import; content is not SDK authority").unwrap();
            fs::rename(root.join("src"), root.join("previous-source")).unwrap();
            let changes = controls
                .iter()
                .map(|(path, _)| {
                    json!({"uri":Url::from_file_path(root.join(path)).unwrap().as_str(),"type":3})
                })
                .collect::<Vec<_>>();
            let event = client.ask(&watch(json!(changes)));
            assert_eq!(event["method"], "eska/restartRequired");
            let state = &event["params"];
            assert_eq!(state["snapshot"], ready["result"]["snapshot"]);
            let changes = state["changes"].as_array().unwrap();
            assert_eq!(changes.len(), controls.len() + 1);
            assert!(changes.contains(&json!({"kind":"source_import",
                "uri":Url::from_file_path(&guard).unwrap().as_str()})));
            for (path, kind) in &controls {
                assert!(changes.contains(&json!({"kind":kind,
                    "uri":Url::from_file_path(root.join(path)).unwrap().as_str()})));
            }
            let _ = blocked(&mut client, state);
            client.finish();
        }
    }
}

/// An active guard adds its recovery hint without hiding settings that require explicit restart.
#[test]
fn active_guard_keeps_settings_changes_in_restart_batches_in_both_locales() {
    for (path, kind) in [
        ("eska.toml", "project_settings"),
        ("bsl-analyzer.toml", "analysis_settings"),
    ] {
        for lang in ["ru-RU", "en-US"] {
            let (_directory, root) = support::project("// Saved body.\r\n", false);
            let (mut client, _) = start(&root, lang);
            read(&mut client, "0");
            let guard = root.join(PROJECT_SOURCE_IMPORT_RECEIPT);
            fs::write(&guard, "active import; content is not SDK authority").unwrap();
            let event = client.ask(&watch(
                json!([{"uri":Url::from_file_path(root.join(path)).unwrap().as_str(),"type":2}]),
            ));
            assert_eq!(event["method"], "eska/restartRequired");
            let changes = event["params"]["changes"].as_array().unwrap();
            assert_eq!(changes.len(), 2);
            assert!(changes.contains(&json!({"kind":kind,
                "uri":Url::from_file_path(root.join(path)).unwrap().as_str()})));
            assert!(changes.contains(&json!({"kind":"source_import",
                "uri":Url::from_file_path(&guard).unwrap().as_str()})));
            client.finish();
        }
    }
}

/// Any visible marker type is a guard; requests, document inputs and valid file batches all observe it.
#[test]
fn live_guard_retires_without_waiting_for_its_file_event_in_both_locales() {
    for trigger in ["request", "document", "saved-event"] {
        for marker in ["file", "directory", "alias"] {
            #[cfg(not(unix))]
            if marker == "alias" {
                continue;
            }
            for lang in ["ru-RU", "en-US"] {
                let (_directory, root) = support::project("// Saved body.\r\n", false);
                let module = root.join("src").join(MODULE);
                let saved = fs::read(&module).unwrap();
                let dirty = "\u{feff}// Я😀 editor bytes.\r\n";
                let (mut client, _) = start(&root, lang);
                read(&mut client, "0");
                assert_eq!(
                    open(&mut client, &root, MODULE, 7, dirty)["method"],
                    "eska/inputChanged"
                );
                let guard = root.join(PROJECT_SOURCE_IMPORT_RECEIPT);
                match marker {
                    "file" => fs::write(&guard, "unparsed guarded operation").unwrap(),
                    "directory" => fs::create_dir(&guard).unwrap(),
                    _ => {
                        #[cfg(unix)]
                        std::os::unix::fs::symlink("absent-target", &guard).unwrap();
                    }
                }
                cold_start_refused(&root, lang);
                let observed = match trigger {
                    "request" => read(&mut client, "2"),
                    "document" => open(
                        &mut client,
                        &root,
                        MODULE,
                        8,
                        "// Rejected later draft.\r\n",
                    ),
                    _ => client.ask(&watch(
                        json!([{"uri":Url::from_file_path(&module).unwrap().as_str(),"type":2}]),
                    )),
                };
                let state = if trigger == "saved-event" {
                    assert_eq!(observed["method"], "eska/restartRequired");
                    &observed["params"]
                } else {
                    let error = if trigger == "request" {
                        &observed["error"]
                    } else {
                        &observed["params"]["error"]
                    };
                    assert_eq!(error["data"]["kind"], "project_restart_required");
                    &error["data"]["details"]
                };
                assert_eq!(state["snapshot"], snapshot("2"));
                assert_eq!(
                    state["changes"],
                    json!([{"kind":"source_import","uri":Url::from_file_path(&guard).unwrap().as_str()}])
                );
                if marker == "directory" {
                    fs::remove_dir(&guard).unwrap();
                } else {
                    fs::remove_file(&guard).unwrap();
                }
                blocked(&mut client, state);
                client.finish();
                let (mut reopened, _) = start(&root, lang);
                let accepted = open(&mut reopened, &root, MODULE, 7, dirty);
                assert_overlay(&accepted, dirty, 7);
                reopened.finish();
                assert_eq!(fs::read(&module).unwrap(), saved);
            }
        }
    }
}

/// Independent ancestry and approval permit a real renamed source journal with changed incoming BSL.
fn renamed_incoming(
    directory: &Path,
    root: &Path,
) -> (AcceptedRef, ImportAncestor, ImportAncestor) {
    fs::create_dir(root.join(".eska")).unwrap();
    let pin = accept_source(&root.join("src"), &root.join(".eska/accepted"), &[]).unwrap();
    let store = directory.join("ancestor");
    let reference = capture_import_ancestor(&root.join("src"), &store).unwrap();
    let ancestor = ImportAncestor::open(&store, &reference).unwrap();
    let native = directory.join("native");
    let path = native.join("CommonModules/Tools.xml");
    let contents = fs::read_to_string(&path).unwrap();
    fs::write(
        native.join("CommonModules/Helpers.xml"),
        contents.replace(">Tools<", ">Helpers<"),
    )
    .unwrap();
    fs::remove_file(path).unwrap();
    let path = native.join("Configuration.xml");
    let contents = fs::read_to_string(&path).unwrap();
    fs::write(path, contents.replace(">Tools<", ">Helpers<")).unwrap();
    fs::rename(
        native.join("CommonModules/Tools"),
        native.join("CommonModules/Helpers"),
    )
    .unwrap();
    fs::write(
        native.join("CommonModules/Helpers/Ext/Module.bsl"),
        "// Incoming saved body.\r\n",
    )
    .unwrap();
    let source = directory.join("incoming-source");
    import_designer(&native, &source).unwrap();
    let store = directory.join("incoming");
    let reference = capture_import_ancestor(&source, &store).unwrap();
    (
        pin,
        ancestor,
        ImportAncestor::open(&store, &reference).unwrap(),
    )
}

/// Completion and rollback both need a new host; renamed dirty tabs are retained as explicit orphan inputs.
#[test]
fn guarded_journal_recovery_preserves_dirty_buffers_and_reopens_the_current_composition() {
    for action in [
        SourceImportRecovery::Complete,
        SourceImportRecovery::RollBack,
    ] {
        for trigger in ["request", "watcher"] {
            for lang in ["ru-RU", "en-US"] {
                let (directory, root) = support::project("// Saved body.\r\n", false);
                let (pin, ancestor, incoming) = renamed_incoming(&directory.0, &root);
                let project = discover(&root).unwrap();
                let plan = prepare_project_source_import(
                    &project,
                    &ProjectScope::Standalone,
                    &pin,
                    &ancestor,
                    &incoming,
                )
                .unwrap();
                let saved_owner = fs::read_to_string(root.join("src").join(OWNER)).unwrap();
                let dirty_owner = format!("# Unpublished YAML annotation.\r\n{saved_owner}");
                let dirty_module = "\u{feff}// Unpublished Я😀 module.\r\n";
                let (mut client, _) = start(&root, lang);
                read(&mut client, "0");
                open(&mut client, &root, OWNER, 5, &dirty_owner);
                let last =
                    open(&mut client, &root, MODULE, 7, dirty_module)["params"]["snapshot"].clone();
                let guard = root.join(PROJECT_SOURCE_IMPORT_RECEIPT);
                assert_eq!(
                    plan.publish_cancellable(&|| guard.exists())
                        .unwrap_err()
                        .code,
                    "source_read_cancelled"
                );
                let observed = if trigger == "watcher" {
                    client.ask(&watch(
                        json!([{"uri":Url::from_file_path(&guard).unwrap().as_str(),"type":1}]),
                    ))
                } else {
                    read(&mut client, last["revision"].as_str().unwrap())
                };
                let state = if trigger == "watcher" {
                    &observed["params"]
                } else {
                    &observed["error"]["data"]["details"]
                };
                assert_eq!(state["snapshot"], last);
                assert_eq!(state["changes"][0]["kind"], "source_import");
                cold_start_refused(&root, lang);
                let receipt = plan.reference().clone();
                drop(plan);
                fs::remove_dir_all(directory.0.join("incoming")).unwrap();
                recover_project_source_import(&root, &receipt, action, &|| false).unwrap();
                assert!(!guard.exists());
                blocked(&mut client, state);
                client.finish();
                let name = if matches!(action, SourceImportRecovery::Complete) {
                    "Helpers"
                } else {
                    "Tools"
                };
                let current_owner = OWNER.replace("Tools", name);
                let current_module = MODULE.replace("Tools", name);
                let saved_owner_after =
                    fs::read_to_string(root.join("src").join(&current_owner)).unwrap();
                let saved_module_after =
                    fs::read_to_string(root.join("src").join(&current_module)).unwrap();
                assert!(!saved_owner_after.contains("Unpublished"));
                assert_eq!(
                    saved_module_after,
                    if name == "Helpers" {
                        "// Incoming saved body.\r\n"
                    } else {
                        "// Saved body.\r\n"
                    }
                );
                let (mut reopened, _) = start(&root, lang);
                let owner = reopened.ask(&request(
                    json!("current"),
                    "eska/metadata/read",
                    json!({"source":current_owner,"snapshot":snapshot("0")}),
                ));
                assert!(owner.get("error").is_none(), "{owner}");
                if name == "Helpers" {
                    let orphan = open(&mut reopened, &root, OWNER, 5, &dirty_owner);
                    assert_eq!(orphan["params"]["error"]["data"]["kind"], "unknown_source");
                }
                let accepted = open(&mut reopened, &root, &current_module, 7, dirty_module);
                assert_overlay(&accepted, dirty_module, 7);
                reopened.finish();
                assert_eq!(
                    fs::read_to_string(root.join("src").join(current_owner)).unwrap(),
                    saved_owner_after
                );
                assert_eq!(
                    fs::read_to_string(root.join("src").join(current_module)).unwrap(),
                    saved_module_after
                );
            }
        }
    }
}
