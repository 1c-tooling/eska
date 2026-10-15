//! Actual framed navigation preserves declared identity, current input and independent policy approval.

use std::{fs, path::Path};

use eska::project_core::ProjectType;
use eska::project_core::source_format::{
    NativeUuid, PROJECT_SOURCE_IMPORT_RECEIPT, SourcePath, SourceReader, accept_source,
};
use serde_json::{Value, json};

use super::{Client, initialize, local_syntax::fixture, request};

const METHOD: &str = "eska/metadata/permissions";

/// Readonly consumers negotiate source/policy readers independently of versioned edit-plan support.
fn start(root: &Path, lang: &str, options: &[&str]) -> (Client, Value) {
    let mut client = Client::with_options(root, lang, options);
    let opened = client.ask(&initialize());
    assert!(opened.get("result").is_some(), "{opened}");
    client.send(&super::notification("initialized"));
    (
        client,
        opened["result"]["capabilities"]["experimental"]["eska"].clone(),
    )
}

/// Policy reads use the exact last wire snapshot and never prepare source edits.
fn permissions(client: &mut Client, snapshot: &Value, sources: &[&str]) -> Value {
    client.ask(&request(
        json!(20),
        METHOD,
        json!({"snapshot":snapshot,"sources":sources}),
    ))
}

/// One complete document lifecycle addresses the real authoring file while leaving disk bytes intact.
fn document(
    client: &mut Client,
    root: &Path,
    source: &str,
    version: Option<i32>,
    text: &str,
) -> Value {
    let uri = url::Url::from_file_path(root.join("src").join(source)).unwrap();
    let (method, params) = version.map_or_else(|| (
        "textDocument/didClose", json!({"textDocument":{"uri":uri.as_str()}}),
    ), |version| (
        "textDocument/didOpen", json!({"textDocument":{"uri":uri.as_str(),"languageId":"eska-source","version":version,"text":text}}),
    ));
    client.ask(&json!({"jsonrpc":"2.0","method":method,"params":params}))
}

/// All four roots and modules are source declarations; pending or failed input never erases identity.
#[test]
fn all4_navigation_and_unknown_permissions_keep_local_scope_and_current_owner_protocol() {
    for kind in fixture::TYPES {
        let (_directory, root, module) = fixture::project(kind, fixture::GOOD, false);
        let reader = SourceReader::open(&root.join("src")).unwrap();
        let binding = reader.declared_module(&module).unwrap();
        let owner = binding.owner.clone();
        let original = fs::read(root.join("src").join(owner.as_path())).unwrap();
        // A generated candidate cannot approve itself, and its storage is irrelevant without a pin.
        fs::write(
            root.join(".eska/accepted/accepted.json"),
            "unapproved corrupt candidate",
        )
        .unwrap();
        for lang in ["ru-RU", "en-US"] {
            let (mut client, hello) = start(&root, lang, &["--profile", "syntax"]);
            let declared = assert_declarations(&mut client, &hello, &root, &reader, &module, kind);
            let snapshot = &hello["snapshot"];
            let pending = permissions(&mut client, snapshot, &[owner.as_str(), module.as_str()]);
            assert_eq!(pending["result"]["snapshot"], *snapshot);
            assert!(pending["result"]["files"].as_array().unwrap().iter().all(
                |file| file["read_only"] == true
                    && file["unknown"] == true
                    && file["objects"] == json!([])
            ));
            let loaded = client.ask(&request(
                json!(3),
                "eska/metadata/read",
                json!({"snapshot":snapshot,"source":owner}),
            ));
            assert_eq!(
                loaded["result"]["metadata"]["state"]["input"]["kind"],
                "ready"
            );
            assert_eq!(
                loaded["result"]["metadata"]["state"]["model"],
                if kind == fixture::TYPES[0] {
                    "ready"
                } else {
                    "source_only"
                }
            );
            assert_eq!(
                loaded["result"]["metadata"]["location_encoding"],
                "utf-8-bytes"
            );
            let current = &loaded["result"]["snapshot"];
            let unapproved = permissions(&mut client, current, &[owner.as_str(), module.as_str()]);
            assert!(
                unapproved["result"]["files"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|file| file["unknown"] == true)
            );
            let overlay = document(
                &mut client,
                &root,
                owner.as_str(),
                Some(7),
                "object: [unterminated\r\n",
            );
            assert_eq!(overlay["method"], "eska/inputChanged");
            assert_eq!(
                overlay["params"]["input"]["state"]["input"]["kind"],
                "unavailable"
            );
            let failed = client.ask(&request(
                json!(4),
                "eska/metadata/owners",
                json!({"snapshot":overlay["params"]["snapshot"],"limit":200}),
            ));
            let failed_owner = failed["result"]["owners"]
                .as_array()
                .unwrap()
                .iter()
                .find(|entry| entry["source"] == owner.as_str())
                .unwrap();
            assert_eq!(failed_owner["declaration"], declared["declaration"]);
            assert_eq!(failed_owner["modules"], declared["modules"]);
            assert_eq!(failed_owner["state"]["input"]["kind"], "unavailable");
            let closed = document(&mut client, &root, owner.as_str(), None, "");
            assert_eq!(
                closed["params"]["input"]["state"]["origin"]["kind"],
                "saved"
            );
            assert_eq!(
                fs::read(root.join("src").join(owner.as_path())).unwrap(),
                original
            );
            client.finish();
        }
    }
}

/// Header and immutable declarations are checked against the actual reader before any lazy payload read.
fn assert_declarations(
    client: &mut Client,
    hello: &Value,
    root: &Path,
    reader: &SourceReader,
    module: &SourcePath,
    kind: ProjectType,
) -> Value {
    assert_eq!(hello["project"]["project_type"], kind.as_str());
    assert_eq!(hello["project"]["scope"], json!({"kind":"standalone"}));
    assert_eq!(
        hello["project"]["root_source"],
        reader.configuration().path.as_str()
    );
    assert_eq!(
        hello["project"]["root_uri"],
        url::Url::from_directory_path(root).unwrap().as_str()
    );
    assert_eq!(
        hello["project"]["source_uri"],
        url::Url::from_directory_path(reader.root())
            .unwrap()
            .as_str()
    );
    assert_eq!(
        hello["analysisScope"],
        if kind == ProjectType::Configuration {
            "canonical_configuration"
        } else {
            "local_syntax"
        }
    );
    assert_eq!(hello["editPlans"], false);
    assert_eq!(
        hello["metadata"]["permissions"],
        json!({"method":METHOD,"scope":"declared-source","readOnly":true,"cancellable":true,"approval":"independent-accepted-base-pin","maxSources":200})
    );
    assert!(
        hello["cancellableMethods"]
            .as_array()
            .unwrap()
            .contains(&json!(METHOD))
    );
    let page = client.ask(&request(
        json!(2),
        "eska/metadata/owners",
        json!({"snapshot":hello["snapshot"],"limit":200}),
    ));
    let owners = page["result"]["owners"].as_array().unwrap();
    assert_eq!(owners.len(), reader.owner_paths().count());
    assert!(
        owners
            .iter()
            .all(|entry| entry["state"]["input"]["kind"] == "pending")
    );
    let binding = reader.declared_module(module).unwrap();
    let declared = owners
        .iter()
        .find(|entry| entry["source"] == binding.owner.as_str())
        .unwrap();
    assert!(!declared["declaration"]["uuid"].as_str().unwrap().is_empty());
    assert_eq!(declared["modules"][0]["source"], module.as_str());
    assert_eq!(
        declared["modules"][0]["native_path"],
        binding.native_path.as_str()
    );
    assert_eq!(
        declared["modules"][0]["native_descriptor"],
        binding.native_descriptor.as_str()
    );
    declared.clone()
}

/// An explicit independently produced pin protects current owners without enabling edit or semantic plans.
#[test]
fn independently_pinned_permissions_follow_pending_ready_and_invalid_current_inputs() {
    let (_directory, root) = super::support::project("// saved module\r\n", false);
    fs::create_dir(root.join(".eska")).unwrap();
    let freeze = NativeUuid::parse("20000000-0000-4000-8000-000000000001").unwrap();
    let pin = accept_source(&root.join("src"), &root.join(".eska/accepted"), &[freeze]).unwrap();
    let owner = "CommonModules/Tools/object.yaml";
    let module = "CommonModules/Tools/Ext/Module.bsl";
    let saved = fs::read(root.join("src").join(owner)).unwrap();
    for lang in ["ru-RU", "en-US"] {
        let (mut client, hello) = start(
            &root,
            lang,
            &["--profile", "syntax", "--accepted-base-sha256", &pin.sha256],
        );
        assert_eq!(hello["editPlans"], false);
        assert_eq!(hello["analysisScope"], "canonical_configuration");
        let pending = permissions(&mut client, &hello["snapshot"], &[owner, module]);
        assert!(
            pending["result"]["files"]
                .as_array()
                .unwrap()
                .iter()
                .all(|file| file["unknown"] == true)
        );
        let loaded = client.ask(&request(
            json!(4),
            "eska/metadata/read",
            json!({"snapshot":hello["snapshot"],"source":owner}),
        ));
        let ready = permissions(&mut client, &loaded["result"]["snapshot"], &[owner, module]);
        assert_eq!(ready["result"]["snapshot"], loaded["result"]["snapshot"]);
        for file in ready["result"]["files"].as_array().unwrap() {
            assert_eq!(file["unknown"], false, "{ready}");
            assert_eq!(file["read_only"], true);
            assert_eq!(file["objects"][0]["allowed"], false);
            assert_eq!(file["objects"][0]["reason"], "eska_frozen");
        }
        let overlay = document(&mut client, &root, owner, Some(8), "object: [invalid\n");
        let invalid = permissions(
            &mut client,
            &overlay["params"]["snapshot"],
            &[owner, module],
        );
        assert!(
            invalid["result"]["files"]
                .as_array()
                .unwrap()
                .iter()
                .all(|file| file["unknown"] == true)
        );
        let closed = document(&mut client, &root, owner, None, "");
        let restored = permissions(&mut client, &closed["params"]["snapshot"], &[owner, module]);
        assert!(
            restored["result"]["files"]
                .as_array()
                .unwrap()
                .iter()
                .all(|file| file["unknown"] == false && file["read_only"] == true)
        );
        assert_eq!(fs::read(root.join("src").join(owner)).unwrap(), saved);
        client.finish();
    }
}

/// Invalid batches, foreign/stale snapshots and undeclared paths cannot become partial policy success.
#[test]
fn permissions_parameters_and_snapshots_fail_closed_without_changing_inputs() {
    let (_directory, root) = super::support::project("// untouched\n", false);
    let (mut client, hello) = start(&root, "en-US", &["--profile", "syntax"]);
    let snapshot = &hello["snapshot"];
    let source = "CommonModules/Tools/object.yaml";
    for sources in [
        json!([]),
        json!([source, source]),
        json!(vec![source; 201]),
        json!(["../outside.yaml"]),
    ] {
        let refused = client.ask(&request(
            json!(21),
            METHOD,
            json!({"snapshot":snapshot,"sources":sources}),
        ));
        assert_eq!(refused["error"]["code"], -32602, "{refused}");
    }
    let extra = client.ask(&request(
        json!(22),
        METHOD,
        json!({"snapshot":snapshot,"sources":[source],"write":true}),
    ));
    assert_eq!(extra["error"]["data"]["kind"], "rpc_invalid_params");
    let unknown = permissions(&mut client, snapshot, &["Unknown/object.yaml"]);
    assert_eq!(unknown["error"]["data"]["kind"], "source_edit_undeclared");
    let stale = permissions(
        &mut client,
        &json!({"session_id":"session-1","revision":"999"}),
        &[source],
    );
    assert_eq!(stale["error"]["data"]["kind"], "stale_revision");
    let foreign = permissions(
        &mut client,
        &json!({"session_id":"foreign","revision":"0"}),
        &[source],
    );
    assert_eq!(foreign["error"]["data"]["kind"], "unknown_session");
    let current = permissions(&mut client, snapshot, &[source]);
    assert_eq!(current["result"]["snapshot"], *snapshot);
    client.finish();
}

/// The new cancellable request has a real transport control and zero expires before any policy read.
#[test]
fn permissions_deadline_is_structured_and_import_guard_retires_the_old_host() {
    let (_directory, root, module) = fixture::project(fixture::TYPES[0], fixture::GOOD, false);
    let (mut timed, hello) = start(
        &root,
        "en-US",
        &["--profile", "syntax", "--timeout-seconds", "0"],
    );
    let refused = permissions(&mut timed, &hello["snapshot"], &[module.as_str()]);
    assert_eq!(refused["error"]["code"], -32000);
    assert_eq!(
        refused["error"]["data"]["kind"],
        "analysis_deadline_exceeded"
    );
    assert_eq!(refused["error"]["data"]["details"], hello["snapshot"]);
    timed.finish();
    let (mut client, hello) = start(&root, "ru-RU", &["--profile", "syntax"]);
    fs::write(
        root.join(PROJECT_SOURCE_IMPORT_RECEIPT),
        "unresolved owned operation",
    )
    .unwrap();
    let retired = permissions(&mut client, &hello["snapshot"], &[module.as_str()]);
    assert_eq!(retired["error"]["data"]["kind"], "project_restart_required");
    assert_eq!(
        retired["error"]["data"]["details"]["snapshot"],
        hello["snapshot"]
    );
    fs::remove_file(root.join(PROJECT_SOURCE_IMPORT_RECEIPT)).unwrap();
    let still_retired = permissions(&mut client, &hello["snapshot"], &[module.as_str()]);
    assert_eq!(
        still_retired["error"]["data"]["kind"],
        "project_restart_required"
    );
    client.finish();
}

/// A supplied malformed pin is rejected even when semantic admission is intentionally disabled.
#[test]
fn invalid_independent_pin_does_not_silently_fall_back_to_unknown_permissions() {
    let (_directory, root, _) = fixture::project(fixture::TYPES[3], fixture::GOOD, false);
    let mut client = Client::with_options(
        &root,
        "en-US",
        &[
            "--profile",
            "syntax",
            "--accepted-base-sha256",
            "not-an-approved-hash",
        ],
    );
    let rejected = client.ask(&initialize());
    assert_eq!(rejected["error"]["code"], -32602);
    assert_eq!(
        rejected["error"]["data"]["kind"],
        "accepted_base_pin_invalid"
    );
}
