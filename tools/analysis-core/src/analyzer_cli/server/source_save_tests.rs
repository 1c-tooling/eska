//! Deterministic transport interposition proves cancellation cannot retract a confirmed disk publication.

use std::{
    fs,
    io::{BufReader, Cursor},
    path::PathBuf,
};

use clap::{CommandFactory, FromArgMatches};
use eska::{
    project_core::source_format::{PROJECT_SOURCE_IMPORT_RECEIPT, accept_source},
    tool_cli::{
        localization::{Locale, Localizer},
        rpc_framing,
    },
};

use super::*;
use crate::{session::tests::support::TestDirectory, source_saves::sha256};

const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";
const ORIGINAL: &str = "// saved\r\nProcedure Saved()\r\nEndProcedure\r\n";
const DRAFT: &str = "// Я😀 editor\r\nProcedure Draft(\r\nEndProcedure\r\n";

/// A small source and independently captured base belong only to this test.
fn project() -> (TestDirectory, PathBuf, String) {
    let directory = TestDirectory::new();
    let root = crate::test_project::project(&directory.0, ORIGINAL, false);
    fs::create_dir(root.join(".eska")).unwrap();
    let pin = accept_source(&root.join("src"), &root.join(".eska/accepted"), &[]).unwrap();
    (directory, root, pin.sha256)
}

/// The real process argument decoder keeps pin and project selection independent of the editor request.
fn arguments(root: &std::path::Path, pin: &str) -> Cli {
    let matches = Cli::command()
        .version(env!("CARGO_PKG_VERSION"))
        .try_get_matches_from([
            "eska-analyzer",
            "--project-dir",
            root.to_str().unwrap(),
            "--accepted-base-sha256",
            pin,
        ])
        .unwrap();
    Cli::from_arg_matches(&matches).unwrap()
}

/// New worker instances use the same state as the production serial transport.
fn server<'a>(cli: &'a Cli, localizer: &'a Localizer) -> Server<'a> {
    Server {
        cli,
        localizer,
        phase: Phase::New,
        session: None,
        session_id: String::new(),
        documents: documents::Documents::default(),
        settings: None,
        completion: completion::Capabilities::default(),
        hierarchical_symbols: false,
        hover_format: hover::Format::Plaintext,
        signature: signature_help::Capabilities::negotiate(&Value::Null),
        edit_plans: None,
        save_authority: None,
        root_publication: None,
        navigation: None,
        read_permissions: None,
        project_inputs: None,
        restart: None,
        watcher: None,
        published_snapshot: None,
        pending_operation: None,
        resource_budget: None,
    }
}

/// Parsing and dispatch retain the real notification and request paths.
fn message(value: &Value) -> protocol::Message {
    let protocol::Incoming::Message(message) =
        protocol::parse(&serde_json::to_vec(value).unwrap()).unwrap()
    else {
        panic!("expected request");
    };
    message
}

/// Initialize and accept the current logical editor text before constructing its save request.
fn initialized(server: &mut Server<'_>, root: &std::path::Path) -> Value {
    server
        .initialize(
            &json!({"capabilities":{},"initializationOptions":{"eska":{"protocolVersion":"0.1"}}}),
        )
        .unwrap();
    server.phase = Phase::Ready;
    let uri = url::Url::from_file_path(root.join("src").join(MODULE)).unwrap();
    let step = server.process(
        &message(
            &json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri.as_str(),"version":1,"languageId":"bsl","text":DRAFT}}}),
        ),
        None,
    );
    let ack: Value =
        serde_json::from_slice(&server.encode_step_response(step, None).unwrap().unwrap()).unwrap();
    server.acknowledge_response();
    json!({"jsonrpc":"2.0","id":"save","method":source_saves::METHOD,"params":{
        "snapshot":ack["params"]["snapshot"],"source":MODULE,"document_version":1,
        "draft_sha256":sha256(DRAFT),"expected_disk_sha256":sha256(ORIGINAL),"text":DRAFT}})
}

/// A real input-thread registry supplies the same tracked cancellation object used at the wire boundary.
fn tracked(value: &Value) -> (protocol::Message, stream::TrackedRequest) {
    let mut bytes = Vec::new();
    rpc_framing::write_frame(&mut bytes, &serde_json::to_vec(value).unwrap()).unwrap();
    let incoming = stream::Incoming::start(BufReader::new(Cursor::new(bytes))).unwrap();
    let stream::Envelope::Message(message, Some(tracked)) = incoming.receive().unwrap() else {
        panic!("expected tracked request");
    };
    (message, tracked)
}

/// Pre-commit cancellation retains disk and overlay; it is still a normal cancellable response.
#[test]
fn save_cancelled_before_commit_preserves_current_editor_and_saved_bytes() {
    let (_directory, root, pin) = project();
    let cli = arguments(&root, &pin);
    let localizer = Localizer::try_new(Locale::EnUs).unwrap();
    let mut server = server(&cli, &localizer);
    let save = initialized(&mut server, &root);
    let (message, tracked) = tracked(&save);
    tracked.control.cancel();
    let step = server.process(&message, Some(&tracked.control));
    assert!(matches!(step, Step::Continue(Some(_))));
    let response: Value = serde_json::from_slice(
        &server
            .encode_step_response(step, Some(&tracked))
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(response["error"]["code"], -32800, "{response}");
    assert_eq!(
        fs::read_to_string(root.join("src").join(MODULE)).unwrap(),
        ORIGINAL
    );
    let input = server
        .session
        .as_ref()
        .unwrap()
        .module(&eska::project_core::source_format::SourcePath::parse(MODULE).unwrap())
        .unwrap();
    assert_eq!(
        input.origin,
        eska::analysis_core::InputOrigin::Overlay {
            document_version: 1
        }
    );
    assert_eq!(input.source_sha256.as_deref(), Some(sha256(DRAFT).as_str()));
}

/// Neither a late cancellation nor an import appearing before serialization may retract the committed receipt.
#[test]
fn confirmed_save_survives_late_cancel_and_import_until_its_wire_result() {
    let (_directory, root, pin) = project();
    let cli = arguments(&root, &pin);
    let localizer = Localizer::try_new(Locale::EnUs).unwrap();
    let mut server = server(&cli, &localizer);
    let save = initialized(&mut server, &root);
    let (message, tracked) = tracked(&save);
    let step = server.process(&message, Some(&tracked.control));
    assert!(matches!(step, Step::Committed(_)));
    assert_eq!(
        fs::read_to_string(root.join("src").join(MODULE)).unwrap(),
        DRAFT
    );
    tracked.control.cancel();
    fs::write(root.join(PROJECT_SOURCE_IMPORT_RECEIPT), "{}").unwrap();
    let response: Value = serde_json::from_slice(
        &server
            .encode_step_response(step, Some(&tracked))
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(response["result"]["outcome"], "published", "{response}");
    assert_eq!(response["result"]["snapshot"], save["params"]["snapshot"]);
    assert!(response.get("error").is_none());
    assert!(server.session.is_some());
    assert!(server.ensure_current().is_err());
    assert!(server.session.is_none());
    assert!(server.save_authority.is_none());
}

/// Resource retirement follows the confirmed frame and cannot reinterpret a committed Save as rejected.
#[test]
fn confirmed_save_is_written_before_resource_retirement() {
    let (_directory, root, pin) = project();
    let cli = arguments(&root, &pin);
    let localizer = Localizer::try_new(Locale::EnUs).unwrap();
    let mut server = server(&cli, &localizer);
    let save = initialized(&mut server, &root);
    // Force pressure at this publication boundary, independently of how an earlier input was admitted.
    let mut exhausted = arguments(&root, &pin);
    exhausted.max_process_source_bytes = 0;
    server.resource_budget = Some(super::resource_budget::Policy::new(&exhausted, true));
    let (message, tracked) = tracked(&save);
    let step = server.process(&message, Some(&tracked.control));
    assert!(matches!(step, Step::Committed(_)));
    tracked.control.cancel();
    let body = server
        .encode_step_response(step, Some(&tracked))
        .unwrap()
        .unwrap();
    let mut output = Vec::new();
    rpc_framing::write_frame(&mut output, &body).unwrap();
    server.acknowledge_response();
    let notification = server.check_resource_budget().unwrap().unwrap();
    rpc_framing::write_frame(&mut output, &serde_json::to_vec(&notification).unwrap()).unwrap();
    let mut reader = Cursor::new(output);
    let receipt: Value =
        serde_json::from_slice(&rpc_framing::read_frame(&mut reader).unwrap().unwrap()).unwrap();
    let retired: Value =
        serde_json::from_slice(&rpc_framing::read_frame(&mut reader).unwrap().unwrap()).unwrap();
    assert_eq!(receipt["result"]["outcome"], "published");
    assert_eq!(receipt["result"]["snapshot"], save["params"]["snapshot"]);
    assert!(receipt.get("error").is_none());
    assert_eq!(retired["method"], "eska/restartRequired");
    assert_eq!(retired["params"]["snapshot"], receipt["result"]["snapshot"]);
    assert_eq!(retired["params"]["cause"], "resource_limit");
    assert_eq!(
        retired["params"]["resource_limit"]["exceeded"],
        json!(["cumulative_ready_source_bytes"])
    );
    assert_eq!(
        fs::read_to_string(root.join("src").join(MODULE)).unwrap(),
        DRAFT
    );
    assert!(server.session.is_none());
    assert!(server.save_authority.is_none());
    assert_eq!(
        server.ensure_current().unwrap_err()["data"]["kind"],
        "project_restart_required"
    );
    assert!(server.check_resource_budget().unwrap().is_none());
}
