//! Actual framed notifications exercise editor ownership against the shared YAML/BSL session.

use std::{fmt::Write as _, fs, path::Path};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use url::Url;

use super::{Client, initialize, notification, request, snapshot, support};

const OWNER: &str = "CommonModules/Tools/object.yaml";
const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";

/// URI encoding must preserve spaces and Unicode without guessing how a file URI is constructed.
fn uri(root: &Path, source: &str) -> String {
    Url::from_file_path(root.join("src").join(source))
        .unwrap()
        .to_string()
}

/// Initialization advertises only the capabilities implemented by this transport.
fn start(root: &Path, lang: &str, extra: &[&str]) -> Client {
    let mut client = Client::with_options(root, lang, extra);
    let response = client.ask(&initialize());
    let capabilities = &response["result"]["capabilities"];
    assert_eq!(capabilities["textDocumentSync"]["change"], 1);
    assert_eq!(capabilities["experimental"]["eska"]["documentSync"], true);
    assert_eq!(
        capabilities["experimental"]["eska"]["languageQueries"],
        false
    );
    assert_eq!(
        capabilities["experimental"]["eska"]["maxRequestBytes"],
        4 * 1024 * 1024
    );
    client.send(&notification("initialized"));
    client
}

/// A notification has no correlation ID; its acknowledgement is an independent server event.
fn notify(method: &str, params: Value) -> Value {
    let mut notification = json!({"jsonrpc":"2.0", "method":method});
    notification["params"] = params;
    notification
}

/// Text is transmitted verbatim; metadata identity still comes from the declared source.
fn open(uri: &str, version: i32, text: &str) -> Value {
    notify(
        "textDocument/didOpen",
        json!({"textDocument":{
        "uri":uri,"languageId":"eska-source","version":version,"text":text}}),
    )
}

/// Full replacement is the only supported document-change mode.
fn change(uri: &str, version: i32, text: &str) -> Value {
    notify(
        "textDocument/didChange",
        json!({"textDocument":{"uri":uri,"version":version},
        "contentChanges":[{"text":text}]}),
    )
}

/// Closing a document relinquishes editor authority over its saved bytes.
fn close(uri: &str) -> Value {
    notify("textDocument/didClose", json!({"textDocument":{"uri":uri}}))
}

/// Each acknowledged update exposes the exact shared revision and current input summary.
fn changed(event: &Value, revision: &str) {
    assert_eq!(event["method"], "eska/inputChanged");
    assert!(event.get("id").is_none());
    assert_eq!(event["params"]["snapshot"], snapshot(revision));
}

/// Rejected lifecycle, version and URI operations leave the shared revision intact.
fn rejected(event: &Value, kind: &str, revision: &str) {
    assert_eq!(event["method"], "eska/inputRejected");
    assert!(event.get("id").is_none());
    assert_eq!(event["params"]["error"]["data"]["kind"], kind);
    assert_eq!(event["params"]["snapshot"], snapshot(revision));
}

/// The current descriptor is read without reloading disk while an overlay owns the input.
fn metadata(client: &mut Client, revision: &str) -> Value {
    let response = client.ask(&request(
        json!(10),
        "eska/metadata/read",
        json!({"snapshot":snapshot(revision),"source":OWNER}),
    ));
    assert!(response.get("error").is_none(), "{response}");
    response["result"]["metadata"].clone()
}

/// The typed property must originate from the current YAML bytes, never a previous saved view.
fn server_property(metadata: &Value) -> &Value {
    let properties = metadata["documents"][0]["objects"][0]["properties"]
        .as_array()
        .unwrap();
    &properties
        .iter()
        .find(|property| property["property"]["key"]["name"] == "Server")
        .unwrap()["property"]["value"]["Text"]
}

/// Hash comparisons include BOM and CRLF and do not depend on server-generated hash values.
fn hash(text: &str) -> String {
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(text.as_bytes()) {
        write!(output, "{byte:02x}").unwrap();
    }
    output
}

/// Saved refresh and save notifications must not overwrite bytes owned by an open editor.
#[test]
fn yaml_and_bsl_share_revision_and_save_keeps_editor_authority() {
    let saved_body = "// Saved body.\r\n";
    let (_directory, root) = support::project(saved_body, false);
    let saved = fs::read_to_string(root.join("src").join(OWNER)).unwrap();
    let draft = saved.replace("<Server>true</Server>", "<Server>false</Server>");
    assert_ne!(saved, draft);
    let owner_uri = uri(&root, OWNER);
    let module_uri = uri(&root, MODULE);
    let mut client = start(&root, "en-US", &[]);
    let saved_metadata = metadata(&mut client, "0");
    assert_eq!(server_property(&saved_metadata), "true");
    let opened = client.ask(&open(&owner_uri, 7, &draft));
    changed(&opened, "2");
    assert_eq!(
        opened["params"]["input"]["state"]["source_sha256"],
        hash(&draft)
    );
    let refresh = client.ask(&request(
        json!(11),
        "eska/metadata/refresh",
        json!({"snapshot":snapshot("2"),"source":OWNER}),
    ));
    assert_eq!(refresh["error"]["data"]["kind"], "overlay_active");
    assert_eq!(refresh["error"]["data"]["details"]["document_version"], 7);
    let body = "\u{feff}// Черновик\r\nПроцедура Тест()\r\nКонецПроцедуры\r\n";
    let module = client.ask(&open(&module_uri, -1, body));
    changed(&module, "3");
    let input = &module["params"]["input"];
    assert_eq!(input["kind"], "module");
    assert_eq!(input["state"]["source_sha256"], hash(body));
    assert_eq!(input["state"]["input"]["bytes"], body.len());
    assert_eq!(
        module["params"]["resources"]["total_source_bytes"],
        draft.len() + body.len()
    );
    assert_eq!(server_property(&metadata(&mut client, "3")), "false");
    fs::write(root.join("src").join(OWNER), "schema_version: broken\n").unwrap();
    let save = client.ask(&notify(
        "textDocument/didSave",
        json!({"textDocument":{"uri":owner_uri},
        "text":"This optional save text does not replace the editor document."}),
    ));
    changed(&save, "3");
    assert_eq!(server_property(&metadata(&mut client, "3")), "false");
    let closed = client.ask(&close(&owner_uri));
    changed(&closed, "4");
    assert_eq!(
        closed["params"]["input"]["state"]["origin"]["kind"],
        "saved"
    );
    assert_eq!(metadata(&mut client, "4")["documents"], json!([]));
    assert_eq!(
        closed["params"]["resources"]["total_source_bytes"],
        body.len()
    );
    let module_closed = client.ask(&close(&module_uri));
    changed(&module_closed, "5");
    assert_eq!(
        module_closed["params"]["input"]["state"]["source_sha256"],
        hash(saved_body)
    );
    assert_eq!(
        module_closed["params"]["input"]["state"]["origin"]["kind"],
        "saved"
    );
    client.finish();
    assert_eq!(
        fs::read_to_string(root.join("src").join(MODULE)).unwrap(),
        saved_body
    );
}

/// Failed accepted text clears the old properties while newer text can recover the same open file.
#[test]
fn invalid_overlay_stale_versions_and_recovery_are_locale_independent() {
    let (_directory, root) = support::project("// Body.\n", false);
    let saved = fs::read_to_string(root.join("src").join(OWNER)).unwrap();
    let owner_uri = uri(&root, OWNER);
    let mut transcripts = Vec::new();
    for lang in ["ru-RU", "en-US"] {
        let mut client = start(&root, lang, &[]);
        let first = client.ask(&open(&owner_uri, 1, &saved));
        changed(&first, "1");
        let invalid = client.ask(&change(&owner_uri, 2, "broken: [\n"));
        changed(&invalid, "2");
        assert_eq!(
            invalid["params"]["input"]["state"]["input"]["kind"],
            "unavailable"
        );
        let invalid_metadata = metadata(&mut client, "2");
        assert_eq!(invalid_metadata["documents"], json!([]));
        let stale = client.ask(&change(&owner_uri, 2, &saved));
        rejected(&stale, "stale_document_version", "2");
        let old_token = client.ask(&request(
            json!(11),
            "eska/metadata/read",
            json!({"snapshot":snapshot("1"),"source":OWNER}),
        ));
        assert_eq!(old_token["error"]["data"]["kind"], "stale_revision");
        let recovered = client.ask(&change(&owner_uri, 3, &saved));
        changed(&recovered, "3");
        assert_eq!(server_property(&metadata(&mut client, "3")), "true");
        let closed = client.ask(&close(&owner_uri));
        changed(&closed, "4");
        transcripts.push(vec![
            first,
            invalid,
            invalid_metadata,
            stale,
            old_token,
            recovered,
            closed,
        ]);
        client.finish();
    }
    assert_eq!(transcripts[0], transcripts[1]);
    assert_eq!(
        fs::read_to_string(root.join("src").join(OWNER)).unwrap(),
        saved
    );
}

/// Unbalanced or unsupported changes are rejected before consuming a document version.
#[test]
fn balanced_lifecycle_and_full_sync_are_required() {
    let (_directory, root) = support::project("// Saved.\n", false);
    let module_uri = uri(&root, MODULE);
    let mut client = start(&root, "en-US", &[]);
    rejected(
        &client.ask(&change(&module_uri, 1, "")),
        "document_not_open",
        "0",
    );
    changed(&client.ask(&open(&module_uri, 1, "// Editor.\n")), "1");
    rejected(
        &client.ask(&open(&module_uri, 2, "// Duplicate.\n")),
        "document_already_open",
        "1",
    );
    let incremental = notify(
        "textDocument/didChange",
        json!({"textDocument":{"uri":module_uri,"version":2},
        "contentChanges":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},"text":""}]}),
    );
    rejected(
        &client.ask(&incremental),
        "full_document_change_required",
        "1",
    );
    changed(&client.ask(&change(&module_uri, 2, "// Full.\n")), "2");
    fs::remove_file(root.join("src").join(MODULE)).unwrap();
    let closed = client.ask(&close(&module_uri));
    changed(&closed, "3");
    assert_eq!(
        closed["params"]["input"]["state"]["input"]["kind"],
        "unavailable"
    );
    assert_eq!(closed["params"]["resources"]["total_source_bytes"], 0);
    rejected(&client.ask(&close(&module_uri)), "document_not_open", "3");
    client.finish();
}

/// URI decoding permits declared missing files but rejects outside paths, resources and binary bodies.
#[test]
fn only_declared_local_text_inputs_can_be_opened() {
    let (directory, original) = support::project("// Body.\n", false);
    let root = directory.0.join("Проект с пробелом");
    fs::rename(original, &root).unwrap();
    fs::remove_file(root.join("src").join(MODULE)).unwrap();
    let module_uri = uri(&root, MODULE);
    assert!(module_uri.contains("%20"));
    let mut client = start(&root, "en-US", &[]);
    for (uri, kind) in [
        (
            String::from("https://example.invalid/Module.bsl"),
            "document_uri_invalid",
        ),
        (
            Url::from_file_path(root.join("outside.bsl"))
                .unwrap()
                .to_string(),
            "document_outside_source",
        ),
        (format!("{module_uri}?changed=true"), "document_uri_invalid"),
        (format!("{module_uri}#line"), "document_uri_invalid"),
        (uri(&root, "unknown.bsl"), "unknown_source"),
        (uri(&root, "source.json"), "unknown_source"),
    ] {
        rejected(&client.ask(&open(&uri, 1, "// Editor.\n")), kind, "0");
    }
    let opened = client.ask(&open(&module_uri, 1, "// Missing saved file is allowed.\n"));
    changed(&opened, "1");
    assert_eq!(opened["params"]["source"], MODULE);
    client.finish();
    let (_binary_directory, binary_root) = support::project("binary", true);
    let mut binary_client = start(&binary_root, "en-US", &[]);
    let binary_uri = uri(&binary_root, "CommonModules/Tools/Ext/Module.bin");
    rejected(
        &binary_client.ask(&open(&binary_uri, 1, "// Not editable.\n")),
        "binary_module",
        "0",
    );
    binary_client.finish();
}

/// An accepted quota failure still owns the document lifecycle and may recover with smaller text.
#[test]
fn source_quota_failure_clears_old_text_and_keeps_editor_version() {
    let (_directory, root) = support::project("// Saved.\n", false);
    let module_uri = uri(&root, MODULE);
    let mut client = start(&root, "en-US", &["--max-retained-source-bytes", "12"]);
    changed(&client.ask(&open(&module_uri, 1, "// Small.\n")), "1");
    let refused = client.ask(&change(
        &module_uri,
        2,
        "// This body exceeds the byte quota.\n",
    ));
    changed(&refused, "2");
    assert_eq!(
        refused["params"]["input"]["state"]["input"]["failure"]["code"],
        "analysis_source_budget_exceeded"
    );
    assert_eq!(
        refused["params"]["input"]["state"]["source_sha256"],
        Value::Null
    );
    assert_eq!(
        refused["params"]["input"]["state"]["origin"]["document_version"],
        2
    );
    assert_eq!(refused["params"]["resources"]["total_source_bytes"], 0);
    rejected(
        &client.ask(&change(&module_uri, 1, "")),
        "stale_document_version",
        "2",
    );
    let recovered = client.ask(&change(&module_uri, 3, "// New.\n"));
    changed(&recovered, "3");
    assert_eq!(
        recovered["params"]["input"]["state"]["source_sha256"],
        hash("// New.\n")
    );
    changed(&client.ask(&close(&module_uri)), "4");
    client.finish();
}

/// Documents larger than the old metadata-only frame limit reach the host without truncation.
#[test]
fn full_document_frames_can_exceed_one_mebibyte() {
    let (_directory, root) = support::project("// Saved.\n", false);
    let module_uri = uri(&root, MODULE);
    let text = format!("// {}\r\n", "x".repeat(1_100_000));
    let mut client = start(&root, "en-US", &[]);
    let opened = client.ask(&open(&module_uri, 1, &text));
    changed(&opened, "1");
    assert_eq!(
        opened["params"]["input"]["state"]["input"]["bytes"],
        text.len()
    );
    assert_eq!(
        opened["params"]["input"]["state"]["source_sha256"],
        hash(&text)
    );
    client.finish();
    assert_eq!(
        fs::read_to_string(root.join("src").join(MODULE)).unwrap(),
        "// Saved.\n"
    );
}
