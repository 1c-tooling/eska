//! An import appearing after dispatch must not publish stale results or acknowledge editor drafts.

use std::{fmt::Write as _, fs, path::Path, path::PathBuf};

use clap::{CommandFactory, FromArgMatches};
use eska::{
    project_core::source_format::PROJECT_SOURCE_IMPORT_RECEIPT,
    tool_cli::localization::{Locale, Localizer},
};
use sha2::{Digest, Sha256};
use url::Url;

use super::super::{Cli, Phase, completion, hover, response, signature_help};
use super::*;
use crate::RequestCancellation;
use crate::session::tests::{fixture, support::TestDirectory};

const OWNER: &str = "Catalogs/Sample/object.yaml";
const MODULE: &str = "Catalogs/Sample/Ext/ObjectModule.bsl";

/// Every scenario owns its complete project under the configured adjacent playground.
fn project() -> (TestDirectory, PathBuf) {
    let (directory, _) = fixture();
    fs::write(
        directory.0.join("eska.toml"),
        "[project]\nname='publication-fixture'\ntype='configuration'\nsource='source'\nsource_format='eska'\n[eska]\naccepted_base='.eska/accepted'\n",
    )
    .unwrap();
    let root = directory.0.clone();
    (directory, root)
}

/// Use the same argument decoder and semantic profile as the real server.
fn arguments(root: &Path, lang: &str) -> Cli {
    let matches = Cli::command()
        .version(env!("CARGO_PKG_VERSION"))
        .try_get_matches_from([
            "eska-analyzer",
            "--project-dir",
            root.to_str().unwrap(),
            "--lang",
            lang,
            "--profile",
            "semantic",
        ])
        .unwrap();
    Cli::from_arg_matches(&matches).unwrap()
}

/// Protocol parsing gives the worker a real request or notification without guessed internal fields.
fn message(value: &Value) -> protocol::Message {
    let protocol::Incoming::Message(message) =
        protocol::parse(&serde_json::to_vec(value).unwrap()).unwrap()
    else {
        panic!("expected a worker message");
    };
    message
}

/// Tests interpose only between worker dispatch and wire publication, never inside production code.
fn dispatched(server: &mut Server<'_>, value: &Value) -> Value {
    let cancellation = RequestCancellation::default();
    let Step::Continue(Some(response)) = server.process(&message(value), Some(&cancellation))
    else {
        panic!("expected a framed worker result");
    };
    response
}

/// Full-sync input owns BOM, CRLF and non-BMP bytes until the server acknowledges their version.
fn opened(root: &Path, source: &str, version: i32, text: &str) -> Value {
    json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":Url::from_file_path(root.join("source").join(source)).unwrap().as_str(),
            "languageId":"yaml","version":version,"text":text}}})
}

/// Initialize and publish normally before exposing the selected worker to the scenario.
fn initialized<'a>(cli: &'a Cli, localizer: &'a Localizer) -> (Server<'a>, Value) {
    let mut server = Server {
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
    };
    let mut response = dispatched(
        &mut server,
        &json!({"jsonrpc":"2.0","id":"initialize","method":"initialize","params":{
            "capabilities":{},"initializationOptions":{"eska":{"protocolVersion":protocol::VERSION}}}}),
    );
    assert!(response.get("result").is_some(), "{response}");
    let snapshot = response["result"]["capabilities"]["experimental"]["eska"]["snapshot"].clone();
    assert!(!server.fence_response(&mut response).unwrap());
    let _ = wire(&response);
    server.acknowledge_response();
    assert!(matches!(
        server.process(
            &message(&json!({"jsonrpc":"2.0","method":"initialized","params":{}})),
            None,
        ),
        Step::Continue(None)
    ));
    (server, snapshot)
}

/// The production serializer must expose the rewritten complete envelope rather than stale success bytes.
fn wire(response: &Value) -> Value {
    serde_json::from_slice(&response::encode(response, None, 1024 * 1024).unwrap()).unwrap()
}

/// Independent hashing proves replay retains exact authoring bytes, including comments and line endings.
fn sha256(text: &str) -> String {
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(text.as_bytes()) {
        write!(output, "{byte:02x}").unwrap();
    }
    output
}

/// A fully computed semantic response remains private until its final publication checks succeed.
#[test]
fn late_import_rejects_computed_semantics_with_the_last_published_snapshot() {
    for (locale, lang) in [(Locale::RuRu, "ru-RU"), (Locale::EnUs, "en-US")] {
        let (_directory, root) = project();
        let cli = arguments(&root, lang);
        let localizer = Localizer::try_new(locale).unwrap();
        let (mut server, published) = initialized(&cli, &localizer);
        let mut response = dispatched(
            &mut server,
            &json!({"jsonrpc":"2.0","id":"semantic","method":"textDocument/diagnostic","params":{
                "textDocument":{"uri":Url::from_file_path(root.join("source").join(MODULE)).unwrap().as_str()},
                "eska":{"snapshot":published}}}),
        );
        assert_eq!(response["result"]["kind"], "full", "{response}");
        assert_eq!(response["result"]["eska"]["profile"], "semantic");
        assert_ne!(response["result"]["eska"]["snapshot"], published);
        assert_eq!(server.published_snapshot.as_ref(), Some(&published));
        fs::write(root.join(PROJECT_SOURCE_IMPORT_RECEIPT), "guard").unwrap();
        assert!(server.fence_response(&mut response).unwrap());
        let response = wire(&response);
        assert_eq!(response["id"], "semantic");
        assert!(response.get("result").is_none());
        assert_eq!(
            response["error"]["data"]["kind"],
            "project_restart_required"
        );
        assert_eq!(response["error"]["data"]["details"]["snapshot"], published);
        assert!(server.session.is_none());
        server.acknowledge_response();
        assert_eq!(server.published_snapshot.as_ref(), Some(&published));
    }
}

/// Readonly policy success is also private until publication, and retirement releases captured handles.
#[test]
fn late_import_discards_permissions_and_retires_navigation_approval_handles() {
    let (_directory, root) = project();
    let cli = arguments(&root, "en-US");
    let localizer = Localizer::try_new(Locale::EnUs).unwrap();
    let (mut server, published) = initialized(&cli, &localizer);
    let mut response = dispatched(
        &mut server,
        &json!({"jsonrpc":"2.0","id":"permissions","method":"eska/metadata/permissions",
        "params":{"snapshot":published,"sources":[OWNER,MODULE]}}),
    );
    assert!(response.get("result").is_some(), "{response}");
    assert!(server.navigation.is_some());
    assert!(server.read_permissions.is_some());
    fs::write(root.join(PROJECT_SOURCE_IMPORT_RECEIPT), "late owned guard").unwrap();
    assert!(server.fence_response(&mut response).unwrap());
    let response = wire(&response);
    assert_eq!(response["id"], "permissions");
    assert!(response.get("result").is_none());
    assert_eq!(response["error"]["data"]["details"]["snapshot"], published);
    assert!(server.navigation.is_none());
    assert!(server.read_permissions.is_none());
}

/// A cancellation accepted before policy evaluation cannot report successful unknown/empty protection.
#[test]
fn permissions_cancelled_before_dispatch_keep_the_published_snapshot() {
    let (_directory, root) = project();
    let cli = arguments(&root, "en-US");
    let localizer = Localizer::try_new(Locale::EnUs).unwrap();
    let (mut server, published) = initialized(&cli, &localizer);
    let control = RequestCancellation::default();
    control.cancel();
    let Step::Continue(Some(response)) = server.process(
        &message(
            &json!({"jsonrpc":"2.0","id":"cancelled","method":"eska/metadata/permissions",
        "params":{"snapshot":published,"sources":[OWNER]}}),
        ),
        Some(&control),
    ) else {
        panic!("expected cancelled policy response");
    };
    assert_eq!(response["error"]["code"], -32800);
    assert_eq!(response["error"]["data"]["kind"], "analysis_cancelled");
    assert_eq!(response["error"]["data"]["details"], published);
    assert!(server.navigation.is_some());
    assert!(server.read_permissions.is_some());
    assert_eq!(server.published_snapshot.as_ref(), Some(&published));
}

/// A version accepted only inside the worker cannot displace the preceding acknowledged editor snapshot.
#[test]
fn late_import_rejects_document_acknowledgement_and_retains_dirty_bytes_for_replay() {
    for (locale, lang) in [(Locale::RuRu, "ru-RU"), (Locale::EnUs, "en-US")] {
        let (_directory, root) = project();
        let cli = arguments(&root, lang);
        let localizer = Localizer::try_new(locale).unwrap();
        let saved = fs::read(root.join("source").join(OWNER)).unwrap();
        let original = std::str::from_utf8(&saved).unwrap();
        let accepted_text = format!("# \u{feff}Я😀 acknowledged draft.\r\n{original}");
        let dirty_text = format!("# \u{feff}Я😀 unacknowledged draft.\r\n{original}");
        let (mut server, _) = initialized(&cli, &localizer);
        let mut accepted = dispatched(&mut server, &opened(&root, OWNER, 7, &accepted_text));
        assert_eq!(accepted["method"], "eska/inputChanged", "{accepted}");
        assert!(!server.fence_response(&mut accepted).unwrap());
        let accepted = wire(&accepted);
        let published = accepted["params"]["snapshot"].clone();
        server.acknowledge_response();
        let mut response = dispatched(
            &mut server,
            &json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
                "textDocument":{"uri":Url::from_file_path(root.join("source").join(OWNER)).unwrap().as_str(),"version":8},
                "contentChanges":[{"text":dirty_text}]}}),
        );
        assert_eq!(response["method"], "eska/inputChanged", "{response}");
        assert_eq!(
            response["params"]["input"]["state"]["origin"]["document_version"],
            8
        );
        assert_ne!(response["params"]["snapshot"], published);
        let guard = root.join(PROJECT_SOURCE_IMPORT_RECEIPT);
        fs::write(&guard, "guard").unwrap();
        assert!(server.fence_response(&mut response).unwrap());
        let response = wire(&response);
        assert_eq!(response["method"], "eska/inputRejected");
        assert_eq!(response["params"]["operation"], "textDocument/didChange");
        assert_eq!(response["params"]["snapshot"], published);
        assert_eq!(
            response["params"]["error"]["data"]["kind"],
            "project_restart_required"
        );
        assert_eq!(
            response["params"]["error"]["data"]["details"]["snapshot"],
            published
        );
        assert!(response["params"].get("input").is_none());
        server.acknowledge_response();
        fs::remove_file(guard).unwrap();
        assert!(server.ensure_current().is_err());
        let (mut reopened, _) = initialized(&cli, &localizer);
        let mut replayed = dispatched(&mut reopened, &opened(&root, OWNER, 8, &dirty_text));
        assert_eq!(replayed["method"], "eska/inputChanged", "{replayed}");
        assert_eq!(
            replayed["params"]["input"]["state"]["origin"]["document_version"],
            8
        );
        assert_eq!(
            replayed["params"]["input"]["state"]["source_sha256"],
            sha256(&dirty_text)
        );
        assert!(!reopened.fence_response(&mut replayed).unwrap());
        let _ = wire(&replayed);
        reopened.acknowledge_response();
        assert_eq!(fs::read(root.join("source").join(OWNER)).unwrap(), saved);
    }
}
