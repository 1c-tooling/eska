//! Real framed saves retain editor authority, exact disk bytes and independently approved policy.

use std::{
    fmt::Write as _,
    fs,
    path::Path,
    thread,
    time::{Duration, Instant},
};

use eska::project_core::source_format::{
    AcceptedBase, AcceptedRef, NativeUuid, PropertyName, PropertyValue, SourcePath, SourceReader,
    accept_source, import_designer_queries, prepare_property_edit_text,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::local_syntax::fixture;
use super::{Client, initialize, notification, request, snapshot, support};

const METHOD: &str = "eska/source/save";
const CANDIDATE: &str = "\u{feff}// Я😀 draft\r\nProcedure Draft(\r\nEndProcedure\r\n";

/// A fresh accepted store changes only this test's policy, never the independently supplied pin.
fn accept(root: &Path, frozen: &[NativeUuid]) -> AcceptedRef {
    let storage = root.join(".eska/accepted");
    fs::remove_dir_all(&storage).unwrap();
    accept_source(&root.join("src"), &storage, frozen).unwrap()
}

/// Fingerprints preserve BOM and CRLF; journal callers pass logical text explicitly.
fn hash(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .fold(String::with_capacity(64), |mut hash, byte| {
            write!(hash, "{byte:02x}").unwrap();
            hash
        })
}

/// Save uses the same exact snapshot and open-document version as the input journal.
fn operation(
    source: &SourcePath,
    current: &Value,
    version: i32,
    original: &str,
    text: &str,
) -> Value {
    request(
        json!("save"),
        METHOD,
        json!({"snapshot":current,"source":source,
        "document_version":version,"draft_sha256":hash(text.strip_prefix('\u{feff}').unwrap_or(text)),
        "expected_disk_sha256":hash(original),"text":text}),
    )
}

/// The ordinary client offers no preview documentChanges capability and needs only the explicit pin.
fn start(root: &Path, pin: Option<&str>, lang: &str) -> (Client, Value) {
    let args = pin.map_or_else(Vec::new, |pin| vec!["--accepted-base-sha256", pin]);
    let mut client = Client::with_options(root, lang, &args);
    let mut initialize = initialize();
    initialize["params"]["initializationOptions"]["eska"]["querySyntax"] = json!(true);
    let response = client.ask(&initialize);
    assert!(response.get("error").is_none(), "{response}");
    client.send(&notification("initialized"));
    (client, response)
}

/// VS Code's journal supplies logical text; the FSP save candidate retains its raw encoding marker.
fn open(
    client: &mut Client,
    root: &Path,
    source: &SourcePath,
    language: &str,
    text: &str,
) -> Value {
    let logical = if language == "yaml" {
        text
    } else {
        text.strip_prefix('\u{feff}').unwrap_or(text)
    };
    let uri = url::Url::from_file_path(root.join("src").join(source.as_path())).unwrap();
    let ack = client.ask(
        &json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri.as_str(),"version":1,"languageId":language,"text":logical}}}),
    );
    assert_eq!(ack["method"], "eska/inputChanged", "{ack}");
    ack
}

/// A later real write preserves BOM/CRLF and keeps didSave from taking the second editor version away.
fn second_save(client: &mut Client, path: &Path, module: &SourcePath) {
    let raw = CANDIDATE.replace("Draft(", "SecondDraft(");
    let logical = raw.strip_prefix('\u{feff}').unwrap();
    let uri = url::Url::from_file_path(path).unwrap();
    let changed = client.ask(
        &json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
        "textDocument":{"uri":uri.as_str(),"version":2},"contentChanges":[{"text":logical}]}}),
    );
    assert_eq!(changed["method"], "eska/inputChanged", "{changed}");
    let response = client.ask(&operation(
        module,
        &changed["params"]["snapshot"],
        2,
        CANDIDATE,
        &raw,
    ));
    assert_eq!(response["result"]["outcome"], "published", "{response}");
    assert_eq!(
        response["result"]["snapshot"],
        changed["params"]["snapshot"]
    );
    assert_eq!(response["result"]["document_version"], 2);
    assert_eq!(response["result"]["disk_sha256"], hash(&raw));
    assert_eq!(fs::read_to_string(path).unwrap(), raw);
    let saved = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didSave","params":{"textDocument":{"uri":uri.as_str()}}}));
    assert_eq!(saved["params"]["snapshot"], changed["params"]["snapshot"]);
    assert_eq!(
        saved["params"]["input"]["state"]["origin"],
        json!({"kind":"overlay","document_version":2})
    );
    assert_eq!(
        saved["params"]["input"]["state"]["source_sha256"],
        hash(logical)
    );
}

/// All four storage profiles save malformed BSL and retain the editor version through repeat and didSave.
#[test]
fn all_four_storage_profiles_save_exact_bsl_without_preview_or_semantic_admission() {
    for kind in fixture::TYPES {
        let (_directory, root, module) = fixture::project(kind, fixture::GOOD, false);
        let pin = accept(&root, &[]);
        let path = root.join("src").join(module.as_path());
        for locale in ["ru-RU", "en-US"] {
            fs::write(&path, fixture::GOOD).unwrap();
            let (mut client, initialized) = start(&root, Some(&pin.sha256), locale);
            let eska = &initialized["result"]["capabilities"]["experimental"]["eska"];
            assert_eq!(
                eska["sourceSave"],
                json!({"method":METHOD,"scope":"selected-open-document",
                "sourceKinds":["yaml","bsl","sdbl"],"maxRequestBytes":4_194_304,"cancellation":"before-commit"})
            );
            assert_eq!(eska["editPlans"], false);
            let ack = open(&mut client, &root, &module, "bsl", CANDIDATE);
            let save = operation(
                &module,
                &ack["params"]["snapshot"],
                1,
                fixture::GOOD,
                CANDIDATE,
            );
            let first = client.ask(&save);
            assert_eq!(first["result"]["outcome"], "published", "{first}");
            assert_eq!(first["result"]["snapshot"], ack["params"]["snapshot"]);
            assert_eq!(first["result"]["document_version"], 1);
            assert_eq!(first["result"]["disk_sha256"], hash(CANDIDATE));
            assert_eq!(fs::read_to_string(&path).unwrap(), CANDIDATE);
            let repeat = client.ask(&save);
            assert_eq!(repeat["result"]["outcome"], "already_current", "{repeat}");
            second_save(&mut client, &path, &module);
            client.finish();
        }
    }
}

/// Query parse errors remain storable text under every supported project profile.
#[test]
fn all_four_storage_profiles_save_malformed_sdbl_with_bom_and_crlf() {
    for kind in support::query_project::TYPES {
        let project = support::query_project::project(kind);
        let pin = accept(&project.root, &[]);
        let path = project.root.join("src").join(project.source.as_path());
        let original = fs::read_to_string(&path).unwrap();
        let (mut client, _) = start(&project.root, Some(&pin.sha256), "en-US");
        let ack = open(
            &mut client,
            &project.root,
            &project.source,
            "sdbl",
            support::query_project::BAD,
        );
        let save = operation(
            &project.source,
            &ack["params"]["snapshot"],
            1,
            &original,
            support::query_project::BAD,
        );
        let response = client.ask(&save);
        assert_eq!(response["result"]["outcome"], "published", "{response}");
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            support::query_project::BAD
        );
        assert_eq!(client.ask(&save)["result"]["outcome"], "already_current");
        client.finish();
    }
}

/// Editor-authored literal Unicode and escaped text publish exactly; frozen changes remain atomic.
#[test]
fn yaml_save_uses_exact_property_policy_and_refuses_denied_or_extra_trivia() {
    for kind in fixture::TYPES {
        for frozen in [false, true] {
            let (directory, root, _module) = fixture::project(kind, fixture::GOOD, false);
            let native = directory.0.join("native");
            let descriptor = if matches!(
                kind,
                eska::project_core::ProjectType::Configuration
                    | eska::project_core::ProjectType::Extension
            ) {
                native.join("Configuration.xml")
            } else {
                native.join("ScopeFixture.xml")
            };
            let xml = fs::read_to_string(&descriptor).unwrap();
            fs::write(
                &descriptor,
                xml.replacen("</Name>", "</Name><Comment>Saved comment</Comment>", 1),
            )
            .unwrap();
            fs::remove_dir_all(root.join("src")).unwrap();
            import_designer_queries(&native, &root.join("src")).unwrap();
            let reader = SourceReader::open(&root.join("src")).unwrap();
            let owner = reader.configuration().path.clone();
            let uuid = reader.declared_owner(&owner).unwrap().uuid.clone();
            let initial = accept(&root, &[]);
            let original = fs::read_to_string(root.join("src").join(owner.as_path())).unwrap();
            let saved_comment = r#""Comment": "Saved comment""#;
            assert_eq!(original.matches(saved_comment).count(), 1);
            let candidate = original.replacen(
                saved_comment,
                r#""Comment": "Current Я😀 \"quoted\" C:\\sample""#,
                1,
            );
            let pin = if frozen {
                accept(&root, &[uuid])
            } else {
                initial
            };
            let (mut client, _) = start(&root, Some(&pin.sha256), "en-US");
            let ack = open(&mut client, &root, &owner, "yaml", &candidate);
            let save = operation(&owner, &ack["params"]["snapshot"], 1, &original, &candidate);
            let response = client.ask(&save);
            if frozen {
                assert_eq!(
                    response["error"]["data"]["kind"], "source_edit_frozen",
                    "{response}"
                );
                assert_eq!(
                    fs::read_to_string(root.join("src").join(owner.as_path())).unwrap(),
                    original
                );
            } else {
                assert_eq!(response["result"]["outcome"], "published", "{response}");
                assert_eq!(
                    fs::read_to_string(root.join("src").join(owner.as_path())).unwrap(),
                    candidate
                );
            }
            client.finish();
        }
    }
}

/// Input guards run before expensive storage reads and never replace the current open draft.
#[test]
fn save_refuses_stale_snapshot_version_hash_closed_and_undeclared_inputs() {
    let (_directory, root, module) = fixture::project(
        eska::project_core::ProjectType::Configuration,
        fixture::GOOD,
        false,
    );
    let pin = accept(&root, &[]);
    let (mut client, _) = start(&root, Some(&pin.sha256), "en-US");
    let closed = operation(&module, &snapshot("0"), 1, fixture::GOOD, CANDIDATE);
    assert_eq!(
        client.ask(&closed)["error"]["data"]["kind"],
        "source_edit_document_not_open"
    );
    let ack = open(&mut client, &root, &module, "bsl", CANDIDATE);
    let valid = operation(
        &module,
        &ack["params"]["snapshot"],
        1,
        fixture::GOOD,
        CANDIDATE,
    );
    for (field, value, kind) in [
        ("snapshot", snapshot("0"), "stale_revision"),
        ("document_version", json!(2), "source_edit_document_version"),
        (
            "draft_sha256",
            json!("0".repeat(64)),
            "source_edit_input_conflict",
        ),
        ("draft_sha256", json!("broken"), "source_save_hash_invalid"),
        (
            "source",
            json!("../foreign/Module.bsl"),
            "source_path_invalid",
        ),
        (
            "text",
            json!("different current text"),
            "source_edit_input_conflict",
        ),
    ] {
        let mut invalid = valid.clone();
        invalid["params"][field] = value;
        let response = client.ask(&invalid);
        assert!(response.get("error").is_some(), "{kind}: {response}");
        if field != "source" {
            assert_eq!(response["error"]["data"]["kind"], kind, "{response}");
        }
    }
    assert_eq!(
        fs::read_to_string(root.join("src").join(module.as_path())).unwrap(),
        fixture::GOOD
    );
    assert_eq!(client.ask(&valid)["result"]["outcome"], "published");
    client.finish();
}

/// Concurrent disk bytes are neither overwritten nor adopted as a new original by the save planner.
#[test]
fn disk_conflict_preserves_the_editor_draft_and_all_four_profiles_enforce_freeze() {
    for kind in fixture::TYPES {
        let (_directory, root, module) = fixture::project(kind, fixture::GOOD, false);
        let reader = SourceReader::open(&root.join("src")).unwrap();
        let owner = &reader.declared_module(&module).unwrap().owner;
        let uuid = reader.declared_owner(owner).unwrap().uuid.clone();
        let pin = accept(&root, &[uuid]);
        let (mut client, _) = start(&root, Some(&pin.sha256), "en-US");
        let ack = open(&mut client, &root, &module, "bsl", CANDIDATE);
        let save = operation(
            &module,
            &ack["params"]["snapshot"],
            1,
            fixture::GOOD,
            CANDIDATE,
        );
        let denied = client.ask(&save);
        assert_eq!(
            denied["error"]["data"]["kind"], "source_edit_frozen",
            "{denied}"
        );
        let path = root.join("src").join(module.as_path());
        fs::write(&path, "// concurrent disk edit\n").unwrap();
        let conflict = client.ask(&save);
        assert_eq!(
            conflict["error"]["data"]["kind"], "source_edit_conflict",
            "{conflict}"
        );
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            "// concurrent disk edit\n"
        );
        client.finish();
    }
}

/// No implicit pin is inferred from adjacent accepted storage, and a wrong explicit pin cannot publish.
#[test]
fn save_requires_a_fresh_independent_pin() {
    let (_directory, root, module) = fixture::project(
        eska::project_core::ProjectType::Configuration,
        fixture::GOOD,
        false,
    );
    let _pin = accept(&root, &[]);
    for pin in [None, Some("0".repeat(64))] {
        let (mut client, initialized) = start(&root, pin.as_deref(), "en-US");
        assert_eq!(
            initialized["result"]["capabilities"]["experimental"]["eska"]
                .get("sourceSave")
                .is_some(),
            pin.is_some()
        );
        let ack = open(&mut client, &root, &module, "bsl", CANDIDATE);
        let response = client.ask(&operation(
            &module,
            &ack["params"]["snapshot"],
            1,
            fixture::GOOD,
            CANDIDATE,
        ));
        assert!(response.get("error").is_some(), "{response}");
        assert_eq!(
            fs::read_to_string(root.join("src").join(module.as_path())).unwrap(),
            fixture::GOOD
        );
        client.finish();
    }
}

/// A client that never consumes the first result can reconnect and reconcile the same hash pair safely.
#[test]
fn lost_save_acknowledgement_reconnects_as_already_current_without_a_receipt_cache() {
    let (_directory, root, module) = fixture::project(
        eska::project_core::ProjectType::Configuration,
        fixture::GOOD,
        false,
    );
    let pin = accept(&root, &[]);
    let path = root.join("src").join(module.as_path());
    {
        let (mut client, _) = start(&root, Some(&pin.sha256), "en-US");
        let ack = open(&mut client, &root, &module, "bsl", CANDIDATE);
        client.send(&operation(
            &module,
            &ack["params"]["snapshot"],
            1,
            fixture::GOOD,
            CANDIDATE,
        ));
        let end = Instant::now() + Duration::from_secs(5);
        while fs::read_to_string(&path).unwrap() != CANDIDATE {
            assert!(Instant::now() < end, "save did not publish");
            thread::sleep(Duration::from_millis(2));
        }
    }
    let (mut client, _) = start(&root, Some(&pin.sha256), "en-US");
    let ack = open(&mut client, &root, &module, "bsl", CANDIDATE);
    let response = client.ask(&operation(
        &module,
        &ack["params"]["snapshot"],
        1,
        fixture::GOOD,
        CANDIDATE,
    ));
    assert_eq!(
        response["result"]["outcome"], "already_current",
        "{response}"
    );
    assert_eq!(fs::read_to_string(path).unwrap(), CANDIDATE);
    client.finish();
}

/// Deadline cancellation is deterministic before any publication for every storage profile.
#[test]
fn zero_deadline_refuses_save_before_commit_for_all_four_profiles() {
    for kind in fixture::TYPES {
        let (_directory, root, module) = fixture::project(kind, fixture::GOOD, false);
        let pin = accept(&root, &[]);
        let mut client = Client::with_options(
            &root,
            "en-US",
            &[
                "--accepted-base-sha256",
                &pin.sha256,
                "--timeout-seconds",
                "0",
            ],
        );
        client.ask(&initialize());
        client.send(&notification("initialized"));
        let ack = open(&mut client, &root, &module, "bsl", CANDIDATE);
        let response = client.ask(&operation(
            &module,
            &ack["params"]["snapshot"],
            1,
            fixture::GOOD,
            CANDIDATE,
        ));
        assert_eq!(
            response["error"]["data"]["kind"], "analysis_deadline_exceeded",
            "{response}"
        );
        assert_eq!(
            fs::read_to_string(root.join("src").join(module.as_path())).unwrap(),
            fixture::GOOD
        );
        client.finish();
    }
}

/// Foreign editor ownership does not grant write authority to an equal selected relative path.
#[test]
fn foreign_composition_input_cannot_save_an_equal_selected_path() {
    let directory = support::TestDirectory::new();
    let project = support::composition_project::project(&directory.0);
    let root = &project.roots["b"];
    let foreign = project.roots["base"]
        .join("src")
        .join(project.module.as_path());
    let foreign_saved = fs::read_to_string(&foreign).unwrap();
    let selected = root.join("src").join(project.module.as_path());
    let selected_saved = fs::read_to_string(&selected).unwrap();
    let (mut client, _) = start(root, Some(&project.selected_accepted_sha256), "en-US");
    let uri = url::Url::from_file_path(&foreign).unwrap();
    let ack = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri.as_str(),"version":1,"languageId":"bsl","text":CANDIDATE.strip_prefix('\u{feff}').unwrap()}}}));
    assert_eq!(ack["method"], "eska/inputChanged", "{ack}");
    let response = client.ask(&operation(
        &project.module,
        &ack["params"]["snapshot"],
        1,
        &selected_saved,
        CANDIDATE,
    ));
    assert_eq!(
        response["error"]["data"]["kind"], "source_edit_document_not_open",
        "{response}"
    );
    assert_eq!(fs::read_to_string(foreign).unwrap(), foreign_saved);
    assert_eq!(fs::read_to_string(selected).unwrap(), selected_saved);
    client.finish();
}

/// A true no-op does not rewrite disk or increment the version; YAML lexical bypasses keep dirty drafts.
#[test]
fn unchanged_and_refused_yaml_saves_preserve_disk_and_document_authority() {
    let (_directory, root, module) = fixture::project(
        eska::project_core::ProjectType::Configuration,
        fixture::GOOD,
        false,
    );
    let pin = accept(&root, &[]);
    let reader = SourceReader::open(&root.join("src")).unwrap();
    let owner = reader.declared_module(&module).unwrap().owner.clone();
    let path = root.join("src").join(owner.as_path());
    let original = fs::read_to_string(&path).unwrap();
    let before = fs::metadata(&path).unwrap().modified().unwrap();
    let (mut client, _) = start(&root, Some(&pin.sha256), "en-US");
    let ack = open(&mut client, &root, &owner, "yaml", &original);
    let save = operation(&owner, &ack["params"]["snapshot"], 1, &original, &original);
    assert_eq!(client.ask(&save)["result"]["outcome"], "already_current");
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
    let uri = url::Url::from_file_path(&path).unwrap();
    let comment = format!("# unrelated annotation\n{original}");
    let changed = client.ask(
        &json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
        "textDocument":{"uri":uri.as_str(),"version":2},"contentChanges":[{"text":comment}]}}),
    );
    let response = client.ask(&operation(
        &owner,
        &changed["params"]["snapshot"],
        2,
        &original,
        &comment,
    ));
    assert_eq!(
        response["error"]["data"]["kind"], "source_edit_value_unsupported",
        "{response}"
    );
    let mut bom = operation(
        &owner,
        &changed["params"]["snapshot"],
        2,
        &original,
        &comment,
    );
    bom["params"]["text"] = json!(format!("\u{feff}{comment}"));
    assert_eq!(
        client.ask(&bom)["error"]["data"]["kind"],
        "source_save_yaml_bom_unsupported"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    let repaired = client.ask(
        &json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
        "textDocument":{"uri":uri.as_str(),"version":3},"contentChanges":[{"text":original}]}}),
    );
    assert_eq!(
        client.ask(&operation(
            &owner,
            &repaired["params"]["snapshot"],
            3,
            &original,
            &original
        ))["result"]["outcome"],
        "already_current"
    );
    client.finish();
}

/// A missed control-file notification cannot authorize writes under stale selected project settings or maps.
#[test]
fn save_rechecks_current_project_settings_and_manifest_without_replacing_the_open_draft() {
    for control in ["settings", "manifest"] {
        let (_directory, root, module) = fixture::project(
            eska::project_core::ProjectType::Configuration,
            fixture::GOOD,
            false,
        );
        let pin = accept(&root, &[]);
        let (mut client, _) = start(&root, Some(&pin.sha256), "en-US");
        let ack = open(&mut client, &root, &module, "bsl", CANDIDATE);
        let save = operation(
            &module,
            &ack["params"]["snapshot"],
            1,
            fixture::GOOD,
            CANDIDATE,
        );
        let path = if control == "settings" {
            root.join("eska.toml")
        } else {
            root.join("src/source.json")
        };
        let original = fs::read_to_string(&path).unwrap();
        let changed = if control == "settings" {
            original.replace(".eska/accepted", ".eska/different-accepted")
        } else {
            format!("{original}\n ")
        };
        assert_ne!(changed, original);
        fs::write(&path, changed).unwrap();
        let response = client.ask(&save);
        assert_eq!(
            response["error"]["data"]["kind"],
            if control == "settings" {
                "source_input_changed"
            } else {
                "source_edit_structure_changed"
            },
            "{response}"
        );
        assert_eq!(
            fs::read_to_string(root.join("src").join(module.as_path())).unwrap(),
            fixture::GOOD
        );
        fs::write(path, original).unwrap();
        assert_eq!(client.ask(&save)["result"]["outcome"], "published");
        client.finish();
    }
}

/// A root property's successful guarded publication must behave as an ordinary overlay-aware file event.
fn root_watch(client: &mut Client, root: &Path, source: &SourcePath, kind: u8) -> Value {
    let uri = url::Url::from_file_path(root.join("src").join(source.as_path())).unwrap();
    client.ask(
        &json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{
        "changes":[{"uri":uri.as_str(),"type":kind}]}}),
    )
}

/// A root-only proof keeps dirty BSL/SDBL siblings available to ordinary sequential Save All requests.
#[test]
fn own_root_property_watch_keeps_dirty_siblings_and_the_original_session_for_all_four_profiles() {
    for kind in support::query_project::TYPES {
        let project = support::query_project::project(kind);
        let pin = accept(&project.root, &[]);
        let reader = SourceReader::open(&project.root.join("src")).unwrap();
        let source = reader.configuration().path.clone();
        let uuid = reader.configuration().declarations[0].uuid.clone();
        let module = reader.module_paths().next().unwrap().clone();
        let root_path = project.root.join("src").join(source.as_path());
        let saved = fs::read_to_string(&root_path).unwrap();
        let module_saved =
            fs::read_to_string(project.root.join("src").join(module.as_path())).unwrap();
        let query_saved =
            fs::read_to_string(project.root.join("src").join(project.source.as_path())).unwrap();
        let base = AcceptedBase::open(&project.root.join(".eska/accepted"), &pin).unwrap();
        let candidate = prepare_property_edit_text(
            &base,
            &uuid,
            PropertyName::Comment,
            PropertyValue::Text("Current root Я😀".into()),
            &saved,
        )
        .unwrap()
        .updated()
        .to_owned();
        let (mut client, _) = start(&project.root, Some(&pin.sha256), "en-US");
        open(&mut client, &project.root, &source, "yaml", &candidate);
        open(&mut client, &project.root, &module, "bsl", CANDIDATE);
        let ack = open(
            &mut client,
            &project.root,
            &project.source,
            "sdbl",
            support::query_project::BAD,
        );
        let current = ack["params"]["snapshot"].clone();
        let save = operation(&source, &current, 1, &saved, &candidate);
        assert_eq!(client.ask(&save)["result"]["outcome"], "published");
        assert_eq!(client.ask(&save)["result"]["outcome"], "already_current");
        let watched = root_watch(&mut client, &project.root, &source, 2);
        assert_eq!(watched["method"], "eska/inputsChanged", "{watched}");
        assert_eq!(watched["params"]["snapshot"], current);
        assert_eq!(watched["params"]["inputs"][0]["invalidated"], false);
        // Atomic replacement can coalesce to delete/create events while the confirmed bytes already exist.
        for kind in [3, 1] {
            let watched = root_watch(&mut client, &project.root, &source, kind);
            assert_eq!(watched["method"], "eska/inputsChanged", "{watched}");
            assert_eq!(watched["params"]["snapshot"], current);
        }
        let permissions = client.ask(&request(
            json!("permissions"),
            "eska/metadata/permissions",
            json!({"snapshot":current,"sources":[source]}),
        ));
        assert!(permissions.get("error").is_none(), "{permissions}");
        assert_eq!(permissions["result"]["files"][0]["read_only"], false);
        for (path, original, text) in [
            (&module, module_saved.as_str(), CANDIDATE),
            (
                &project.source,
                query_saved.as_str(),
                support::query_project::BAD,
            ),
        ] {
            let response = client.ask(&operation(path, &current, 1, original, text));
            assert_eq!(response["result"]["outcome"], "published", "{response}");
            assert_eq!(response["result"]["snapshot"], current);
            assert_eq!(
                fs::read_to_string(project.root.join("src").join(path.as_path())).unwrap(),
                text
            );
        }
        fs::write(&root_path, format!("{candidate}\n# external change\n")).unwrap();
        assert_eq!(
            root_watch(&mut client, &project.root, &source, 2)["method"],
            "eska/restartRequired"
        );
        client.finish();
    }
}

/// Observing an already-current root does not fabricate proof that this host published that root.
#[test]
fn already_current_root_without_prior_publication_retains_the_normal_control_restart() {
    let project = support::query_project::project("configuration");
    let pin = accept(&project.root, &[]);
    let reader = SourceReader::open(&project.root.join("src")).unwrap();
    let source = reader.configuration().path.clone();
    let text = fs::read_to_string(project.root.join("src").join(source.as_path())).unwrap();
    let (mut client, _) = start(&project.root, Some(&pin.sha256), "en-US");
    let ack = open(&mut client, &project.root, &source, "yaml", &text);
    let response = client.ask(&operation(
        &source,
        &ack["params"]["snapshot"],
        1,
        &text,
        &text,
    ));
    assert_eq!(
        response["result"]["outcome"], "already_current",
        "{response}"
    );
    assert_eq!(
        root_watch(&mut client, &project.root, &source, 2)["method"],
        "eska/restartRequired"
    );
    client.finish();
}

/// Deletion and aliases always retire the original source, even when their target has once-published bytes.
#[cfg(unix)]
#[test]
fn own_root_publication_never_exempts_deletion_or_aliased_replacement() {
    for alias in [false, true] {
        let project = support::query_project::project("configuration");
        let pin = accept(&project.root, &[]);
        let reader = SourceReader::open(&project.root.join("src")).unwrap();
        let source = reader.configuration().path.clone();
        let uuid = reader.configuration().declarations[0].uuid.clone();
        let path = project.root.join("src").join(source.as_path());
        let original = fs::read_to_string(&path).unwrap();
        let base = AcceptedBase::open(&project.root.join(".eska/accepted"), &pin).unwrap();
        let candidate = prepare_property_edit_text(
            &base,
            &uuid,
            PropertyName::Comment,
            PropertyValue::Text("Changed root".into()),
            &original,
        )
        .unwrap()
        .updated()
        .to_owned();
        let (mut client, _) = start(&project.root, Some(&pin.sha256), "en-US");
        let ack = open(&mut client, &project.root, &source, "yaml", &candidate);
        assert_eq!(
            client.ask(&operation(
                &source,
                &ack["params"]["snapshot"],
                1,
                &original,
                &candidate
            ))["result"]["outcome"],
            "published"
        );
        fs::remove_file(&path).unwrap();
        if alias {
            let target = project.root.join("root-copy.yaml");
            fs::write(&target, &candidate).unwrap();
            std::os::unix::fs::symlink(target, &path).unwrap();
        }
        assert_eq!(
            root_watch(
                &mut client,
                &project.root,
                &source,
                if alias { 2 } else { 3 }
            )["method"],
            "eska/restartRequired"
        );
        client.finish();
    }
}
