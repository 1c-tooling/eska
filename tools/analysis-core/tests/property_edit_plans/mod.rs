//! Property plans share CST/policy and bind actual editor versions without writing sources.

use std::{fs, path::Path};

use eska::{
    analysis_core::InputRequest,
    project_core::source_format::{
        AcceptedBase, NativeUuid, PropertyName, PropertyValue, SourcePath,
    },
};
use eska_analysis::{AnalysisSession, PropertyEditError};
use serde_json::{Value, json};

use super::{Client, initialize, notification, request, snapshot, support};

mod controls;
mod fixture;
use fixture::{OWNER, UUID};
const METHOD: &str = "eska/metadata/preparePropertyEdit";

/// The client promises versioned documentChanges before the server can offer editor plans.
fn initialize_edits() -> Value {
    let mut value = initialize();
    value["params"]["capabilities"] =
        json!({"workspace":{"workspaceEdit":{"documentChanges":true}}});
    value
}

/// A reopened server uses current UUID/path bindings while overlay values and the support pin stay exact.
#[test]
fn post_import_property_plans_use_current_bindings_under_the_original_pin() {
    use eska::project_core::source_format::{
        ImportAncestor, capture_import_ancestor, export_designer, import_designer,
        prepare_source_import,
    };
    for mode in ["rename", "add"] {
        let (directory, root, reference) = fixture::project(false);
        let source = root.join("src-plans");
        let store = directory.0.join("ancestor");
        let pin = capture_import_ancestor(&source, &store).unwrap();
        let ancestor = ImportAncestor::open(&store, &pin).unwrap();
        let native = directory.0.join("incoming-native");
        export_designer(&source, &native).unwrap();
        let path = native.join("CommonModules/Tools.xml");
        let contents = fs::read_to_string(&path).unwrap();
        let uuid = if mode == "add" {
            "90000000-0000-4000-8000-000000000001"
        } else {
            UUID
        };
        fs::write(
            native.join("CommonModules/Helpers.xml"),
            contents.replace(">Tools<", ">Helpers<").replace(UUID, uuid),
        )
        .unwrap();
        if mode == "rename" {
            fs::remove_file(&path).unwrap();
        }
        let path = native.join("Configuration.xml");
        let contents = fs::read_to_string(&path).unwrap();
        let refs = if mode == "add" {
            "<CommonModule>Tools</CommonModule><CommonModule>Helpers</CommonModule>"
        } else {
            "<CommonModule>Helpers</CommonModule>"
        };
        fs::write(
            path,
            contents.replace("<CommonModule>Tools</CommonModule>", refs),
        )
        .unwrap();
        fs::rename(
            native.join("CommonModules/Tools"),
            native.join("CommonModules/Helpers"),
        )
        .unwrap();
        if mode == "add" {
            fs::create_dir_all(native.join("CommonModules/Tools/Ext")).unwrap();
            fs::write(
                native.join("CommonModules/Tools/Ext/Module.bsl"),
                "// Saved module.\n",
            )
            .unwrap();
        }
        let incoming_source = directory.0.join("incoming-source");
        import_designer(&native, &incoming_source).unwrap();
        let incoming_store = directory.0.join("incoming-store");
        let incoming_pin = capture_import_ancestor(&incoming_source, &incoming_store).unwrap();
        let incoming = ImportAncestor::open(&incoming_store, &incoming_pin).unwrap();
        let approved = AcceptedBase::open(&root.join(".eska/accepted"), &reference).unwrap();
        let plan = prepare_source_import(&source, &ancestor, &incoming, &approved).unwrap();
        assert!(plan.conflicts().is_empty(), "{:?}", plan.conflicts());
        let output = directory.0.join("output");
        plan.publish_source(&output).unwrap();
        fs::rename(&source, directory.0.join("previous-source")).unwrap();
        fs::rename(output, &source).unwrap();
        let owner = "CommonModules/Helpers/object.yaml";
        let path = source.join(owner);
        let saved = fs::read_to_string(&path).unwrap();
        let text = format!("# unsaved annotation\n{saved}");
        for locale in ["ru-RU", "en-US"] {
            let mut client = Client::with_options(
                &root,
                locale,
                &["--accepted-base-sha256", &reference.sha256],
            );
            client.ask(&initialize_edits());
            client.send(&notification("initialized"));
            let uri = url::Url::from_file_path(&path).unwrap();
            client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri.as_str(),"version":1,"languageId":"yaml","text":text}}}));
            let mut operation = prepare(&text, "1", 1, "Post-import overlay edit");
            operation["params"]["source"] = json!(owner);
            operation["params"]["uuid"] = json!(uuid);
            let response = client.ask(&operation);
            assert!(response.get("error").is_none(), "{mode}: {response}");
            let preview = &response["result"];
            assert_eq!(preview["accepted_base"]["sha256"], reference.sha256);
            assert_eq!(preview["source"], owner);
            assert!(apply(&text, preview).contains("Post-import overlay edit"));
            assert_eq!(fs::read_to_string(&path).unwrap(), saved);
            let guard = root.join(".eska-project-source-import.json");
            fs::write(&guard, "{}").unwrap();
            assert_eq!(
                client.ask(&operation)["error"]["data"]["kind"],
                "source_import_incomplete"
            );
            fs::remove_file(guard).unwrap();
            client.finish();
        }
    }
}

/// The current editor bytes are provided explicitly and must match the host's stored fingerprint.
fn prepare(text: &str, revision: &str, version: i32, value: &str) -> Value {
    request(
        json!("plan"),
        METHOD,
        json!({"snapshot":snapshot(revision),"source":OWNER,
        "document_version":version,"text":text,"uuid":UUID,"property":"Comment",
        "value":{"state":"text","value":value}}),
    )
}

/// A plan is applied only in test-owned memory; the process receives the resulting didChange.
fn apply(text: &str, plan: &Value) -> String {
    let mut result = text.to_owned();
    for edit in plan["workspace_edit"]["documentChanges"][0]["edits"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
    {
        let offset = |position: &Value| {
            let line = usize::try_from(position["line"].as_u64().unwrap()).unwrap();
            let character = usize::try_from(position["character"].as_u64().unwrap()).unwrap();
            let start = text
                .split_inclusive('\n')
                .take(line)
                .map(str::len)
                .sum::<usize>();
            let prefix = text[start..]
                .split('\n')
                .next()
                .unwrap()
                .encode_utf16()
                .take(character)
                .collect::<Vec<_>>();
            start + String::from_utf16(&prefix).unwrap().len()
        };
        result.replace_range(
            offset(&edit["range"]["start"])..offset(&edit["range"]["end"]),
            edit["newText"].as_str().unwrap(),
        );
    }
    result
}

/// Client setup does not verify missing baseline storage, and unsupported edit versioning stays off.
#[test]
fn plans_require_an_independent_pin_and_versioned_client_before_lazy_base_read() {
    let (_directory, root, reference) = fixture::project(false);
    fs::remove_dir_all(root.join(".eska/accepted")).unwrap();
    for lang in ["en-US", "ru-RU"] {
        for (pin, capabilities, enabled) in [
            (false, initialize_edits(), false),
            (true, initialize(), false),
            (true, initialize_edits(), true),
        ] {
            let args = if pin {
                vec!["--accepted-base-sha256", reference.sha256.as_str()]
            } else {
                Vec::new()
            };
            let mut client = Client::with_options(&root, lang, &args);
            let response = client.ask(&capabilities);
            assert_eq!(
                response["result"]["capabilities"]["experimental"]["eska"]["editPlans"],
                enabled
            );
            client.send(&notification("initialized"));
            let response = client.ask(&prepare("not current bytes", "0", 1, "Changed"));
            assert_eq!(
                response["error"]["data"]["kind"],
                if enabled {
                    "source_edit_input_unavailable"
                } else {
                    "source_edit_plans_disabled"
                }
            );
            client.finish();
        }
    }
}

/// The existing transport delegates new scalar domains to the shared editor in both locales.
#[test]
fn boolean_property_plans_preserve_cst_and_reject_text_coercion() {
    let (_directory, root, reference) = fixture::project(false);
    let path = root.join("src-plans").join(OWNER);
    let saved = fs::read_to_string(&path).unwrap();
    let text = format!("# Я😀\r\n{}", saved.replace('\n', "\r\n"))
        .replace("\"value\": true", "\"value\": TRUE # boolean");
    let uri = url::Url::from_file_path(&path).unwrap().to_string();
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client =
            Client::with_options(&root, lang, &["--accepted-base-sha256", &reference.sha256]);
        client.ask(&initialize_edits());
        client.send(&notification("initialized"));
        client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"version":1,"languageId":"yaml","text":text}}}));
        let mut operation = prepare(&text, "1", 1, "unused");
        operation["params"]["property"] = json!("Server");
        operation["params"]["value"] = json!({"state":"boolean","value":false});
        let response = client.ask(&operation);
        assert!(response.get("error").is_none(), "{response}");
        let plan = &response["result"];
        assert_eq!(
            apply(&text, plan),
            text.replace("TRUE # boolean", "false # boolean")
        );
        assert_eq!(plan["unchanged"], false);
        if let Some(previous) = &previous {
            assert_eq!(previous, plan);
        }
        previous = Some(plan.clone());
        operation["params"]["value"] = json!({"state":"boolean","value":true});
        let no_op = client.ask(&operation);
        assert_eq!(no_op["result"]["unchanged"], true);
        assert_eq!(
            no_op["result"]["workspace_edit"]["documentChanges"][0]["edits"],
            json!([])
        );
        operation["params"]["value"] = json!({"state":"text","value":"false"});
        assert_eq!(
            client.ask(&operation)["error"]["data"]["kind"],
            "source_property_type_invalid"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), saved);
        client.finish();
    }
}

/// Exact current versions precede baseline IO; a valid plan changes only the intended property.
#[test]
fn versioned_property_plan_matches_core_and_changes_neither_input_nor_disk_in_both_locales() {
    let (_directory, root, reference) = fixture::project(false);
    let path = root.join("src-plans").join(OWNER);
    let saved = fs::read_to_string(&path).unwrap();
    let text = format!("# Я😀\r\n{}", saved.replace('\n', "\r\n"))
        .replace("\"Saved comment\"", "'Current comment' # keep");
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client =
            Client::with_options(&root, lang, &["--accepted-base-sha256", &reference.sha256]);
        client.ask(&initialize_edits());
        client.send(&notification("initialized"));
        let uri = url::Url::from_file_path(&path).unwrap().to_string();
        let opened = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"version":1,"languageId":"yaml","text":text}}}));
        let base = root.join(".eska/accepted");
        let absent = root.join(".eska/temporarily-absent");
        fs::rename(&base, &absent).unwrap();
        check_preflight(&mut client, &text);
        assert_eq!(
            client.ask(&prepare(&text, "1", 1, "Changed comment"))["error"]["data"]["kind"],
            "source_baseline_missing"
        );
        fs::rename(&absent, &base).unwrap();
        let response = client.ask(&prepare(&text, "1", 1, "Changed comment"));
        let plan = &response["result"];
        assert_eq!(plan["snapshot"], snapshot("1"));
        assert_eq!(plan["document_version"], 1);
        assert_eq!(plan["accepted_base"]["sha256"], reference.sha256);
        assert_eq!(
            plan["workspace_edit"]["documentChanges"][0]["textDocument"]["version"],
            1
        );
        assert_eq!(
            plan["workspace_edit"]["documentChanges"][0]["textDocument"]["uri"],
            uri
        );
        let updated = apply(&text, plan);
        assert_eq!(
            updated,
            text.replace("'Current comment'", "'Changed comment'")
        );
        assert_eq!(
            updated,
            core_preview(root.join("src-plans").as_path(), &base, &reference, &text)
        );
        let owners = client.ask(&request(
            json!("plan"),
            "eska/metadata/owners",
            json!({"snapshot":snapshot("1"),"limit":1}),
        ));
        assert_eq!(owners["result"]["resources"], opened["params"]["resources"]);
        assert_eq!(fs::read_to_string(&path).unwrap(), saved);
        if let Some(previous) = &previous {
            assert_eq!(previous, plan);
        }
        previous = Some(plan.clone());
        let no_op = client.ask(&prepare(&text, "1", 1, "Current comment"));
        assert_eq!(no_op["result"]["unchanged"], true);
        assert_eq!(
            no_op["result"]["workspace_edit"]["documentChanges"][0]["edits"],
            json!([])
        );
        client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":2},"contentChanges":[{"text":updated}]}}));
        assert_eq!(
            client.ask(&prepare(&text, "1", 1, "Changed"))["error"]["data"]["kind"],
            "stale_revision"
        );
        client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri}}}));
        assert_eq!(
            client.ask(&prepare(&saved, "3", 2, "Changed"))["error"]["data"]["kind"],
            "source_edit_document_not_open"
        );
        client.finish();
    }
    assert_eq!(fs::read_to_string(path).unwrap(), saved);
}

/// Cheap failures retain input and do not open the deliberately absent accepted base.
fn check_preflight(client: &mut Client, text: &str) {
    for (request, expected) in [
        (prepare(text, "0", 1, "Changed"), "stale_revision"),
        (
            prepare(text, "1", 2, "Changed"),
            "source_edit_document_version",
        ),
        (
            prepare("other bytes", "1", 1, "Changed"),
            "source_edit_input_conflict",
        ),
        (
            prepare(&text.replace("Current", "Changed"), "1", 1, "Changed"),
            "source_edit_input_conflict",
        ),
    ] {
        assert_eq!(client.ask(&request)["error"]["data"]["kind"], expected);
    }
}

/// Library consumers use the same current input checks and CST preparation without IDE runtime.
fn core_preview(
    source: &Path,
    base: &Path,
    reference: &eska::project_core::source_format::AcceptedRef,
    text: &str,
) -> String {
    let mut core = AnalysisSession::open(source).unwrap();
    let path = SourcePath::parse(OWNER).unwrap();
    core.apply_overlay(&path, core.revision(), 1, text, &InputRequest::default())
        .unwrap();
    let input = core
        .property_edit_input(&path, core.revision(), 1, text)
        .unwrap();
    let base = AcceptedBase::open(base, reference).unwrap();
    input
        .prepare(
            &base,
            &NativeUuid::parse(UUID).unwrap(),
            PropertyName::Comment,
            PropertyValue::Text("Changed comment".into()),
        )
        .unwrap()
        .preview
        .updated()
        .to_owned()
}

/// The shared library rejects closed, wrong-version, wrong-hash and invalid current input.
#[test]
fn library_input_preconditions_do_not_change_session_revision_or_resources() {
    let (_directory, root, _) = fixture::project(false);
    let source = root.join("src-plans");
    let text = fs::read_to_string(source.join(OWNER)).unwrap();
    let mut core = AnalysisSession::open(&source).unwrap();
    let path = SourcePath::parse(OWNER).unwrap();
    assert_eq!(
        core.property_edit_input(&path, core.revision(), 1, &text)
            .unwrap_err(),
        PropertyEditError::InputUnavailable
    );
    core.load_saved(&path, core.revision(), &InputRequest::default())
        .unwrap();
    assert_eq!(
        core.property_edit_input(&path, core.revision(), 1, &text)
            .unwrap_err(),
        PropertyEditError::DocumentNotOpen
    );
    core.apply_overlay(&path, core.revision(), 1, &text, &InputRequest::default())
        .unwrap();
    let revision = core.revision();
    let resources = core.resources();
    assert_eq!(
        core.property_edit_input(&path, revision, 2, &text)
            .unwrap_err(),
        PropertyEditError::DocumentVersion {
            current: 1,
            received: 2
        }
    );
    assert_eq!(
        core.property_edit_input(&path, revision, 1, "different")
            .unwrap_err(),
        PropertyEditError::SourceConflict
    );
    assert_eq!(
        core.property_edit_input(&path, revision, 1, &text.replace("Saved", "Wrong"))
            .unwrap_err(),
        PropertyEditError::SourceConflict
    );
    assert_eq!(core.revision(), revision);
    assert_eq!(core.resources(), resources);
    core.apply_overlay(&path, revision, 2, "broken: [", &InputRequest::default())
        .unwrap();
    assert_eq!(
        core.property_edit_input(&path, core.revision(), 2, "broken: [")
            .unwrap_err(),
        PropertyEditError::InputUnavailable
    );
}

/// A verified accepted freeze and unsupported structural changes cannot be bypassed by a draft.
#[test]
fn accepted_policy_and_existing_property_scope_are_preserved() {
    for frozen in [false, true] {
        let (_directory, root, reference) = fixture::project(frozen);
        let path = root.join("src-plans").join(OWNER);
        let text = fs::read_to_string(&path).unwrap();
        let mut client = Client::with_options(
            &root,
            "en-US",
            &["--accepted-base-sha256", &reference.sha256],
        );
        client.ask(&initialize_edits());
        client.send(&notification("initialized"));
        let uri = url::Url::from_file_path(path).unwrap().to_string();
        client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"version":1,"languageId":"yaml","text":text}}}));
        let mut command = prepare(&text, "1", 1, "Changed");
        if !frozen {
            command["params"]["property"] = json!("Name");
        }
        assert_eq!(
            client.ask(&command)["error"]["data"]["kind"],
            if frozen {
                "source_edit_frozen"
            } else {
                "source_edit_operation_unsupported"
            }
        );
        command = prepare(&text, "1", 1, "Changed");
        command["params"]["property"] = json!("Version");
        assert_eq!(
            client.ask(&command)["error"]["data"]["kind"],
            if frozen {
                "source_edit_frozen"
            } else {
                "source_edit_property_missing"
            }
        );
        client.finish();
    }
}

/// Bad caller trust or corrupted accepted bytes return no edit and leave the metadata host usable.
#[test]
fn untrusted_and_corrupt_base_cannot_produce_a_property_edit() {
    for lang in ["en-US", "ru-RU"] {
        let (_directory, root, reference) = fixture::project(false);
        let path = root.join("src-plans").join(OWNER);
        let text = fs::read_to_string(&path).unwrap();
        for pin in ["0".repeat(64), reference.sha256.clone()] {
            if pin == reference.sha256 {
                let manifest = root.join(".eska/accepted/accepted.json");
                let mut bytes = fs::read(&manifest).unwrap();
                bytes.push(b' ');
                fs::write(manifest, bytes).unwrap();
            }
            let mut client = Client::with_options(&root, lang, &["--accepted-base-sha256", &pin]);
            client.ask(&initialize_edits());
            client.send(&notification("initialized"));
            let uri = url::Url::from_file_path(&path).unwrap().to_string();
            let opened = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"version":1,"languageId":"yaml","text":text}}}));
            let response = client.ask(&prepare(&text, "1", 1, "Changed"));
            assert_eq!(
                response["error"]["data"]["kind"],
                "source_baseline_untrusted"
            );
            assert!(response.get("result").is_none());
            let owners = client.ask(&request(
                json!(2),
                "eska/metadata/owners",
                json!({"snapshot":snapshot("1"),"limit":1}),
            ));
            assert_eq!(owners["result"]["resources"], opened["params"]["resources"]);
            assert_eq!(owners["result"]["snapshot"], snapshot("1"));
            client.finish();
        }
        assert_eq!(fs::read_to_string(path).unwrap(), text);
    }
}
