//! Real framed children retire only after publishing the accepted operation's exact response.

use std::fmt::Write as _;

use sha2::{Digest, Sha256};

use super::*;

const OWNER: &str = "CommonModules/Tools/object.yaml";
const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";
const MAX: &str = "18446744073709551615";
const KEYS: [&str; 4] = [
    "cumulative_ready_source_bytes",
    "accepted_publications",
    "type_kernel_entries",
    "interner_norm_entries",
];

/// Isolate one publication threshold from process-global vocabulary and native type growth.
fn limits(publications: &str) -> Value {
    json!({"cumulative_ready_source_bytes":MAX,"accepted_publications":publications,
        "type_kernel_entries":MAX,"interner_norm_entries":MAX})
}

/// A normal client may omit or explicitly decline the independent restart negotiation.
fn start(root: &Path, locale: &str, opt_in: Option<bool>, publications: &str) -> Client {
    let mut client = Client::with_options(
        root,
        locale,
        &[
            "--max-process-source-bytes",
            MAX,
            "--max-process-input-publications",
            publications,
            "--max-type-kernel-entries",
            MAX,
            "--max-interner-norm-entries",
            MAX,
        ],
    );
    let mut init = initialize();
    init["params"]["initializationOptions"]["eska"]["querySyntax"] = json!(true);
    if let Some(value) = opt_in {
        init["params"]["initializationOptions"]["eska"]["resourceRestart"] = json!(value);
    }
    let response = client.ask(&init);
    assert!(response.get("error").is_none(), "{response}");
    assert_eq!(
        response["result"]["capabilities"]["experimental"]["eska"]["resourceRestart"],
        json!({"version":1,"enabled":opt_in == Some(true),"scope":"process",
            "notification":"eska/restartRequired","cause":"resource_limit",
            "checkBoundary":"after-published-frame","overshoot":"one-operation",
            "processRssLimit":false,"limits":limits(publications)})
    );
    client.send(&notification("initialized"));
    client
}

/// Every input URI comes from the selected fixture's declared source tree.
fn uri(source: &Path, relative: &str) -> String {
    url::Url::from_file_path(source.join(relative))
        .unwrap()
        .into()
}

/// The full document and version belong to an ordinary didOpen, without a mutating RPC.
fn open(uri: &str, language: &str, text: &str) -> Value {
    json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{
        "uri":uri,"languageId":language,"version":7,"text":text}}})
}

/// Full replacement versions exercise the host's accepted-publication boundary.
fn change(uri: &str, version: i32, text: &str) -> Value {
    json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
        "textDocument":{"uri":uri,"version":version},"contentChanges":[{"text":text}]}})
}

/// Fingerprints cover the transmitted UTF-8 bytes, including BOM, CRLF and emoji.
fn hash(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .fold(String::with_capacity(64), |mut value, byte| {
            write!(value, "{byte:02x}").unwrap();
            value
        })
}

/// The first frame is an exact input ACK, rather than a replacement retirement event.
fn assert_ack(ack: &Value, source: &str, kind: &str, revision: &str, version: i32, text: &str) {
    assert_eq!(ack["method"], "eska/inputChanged");
    assert!(ack.get("id").is_none());
    assert_eq!(ack["params"]["snapshot"], snapshot(revision));
    assert_eq!(ack["params"]["root"], 0);
    assert_eq!(ack["params"]["source"], source);
    assert_eq!(ack["params"]["input"]["kind"], kind);
    assert_eq!(
        ack["params"]["input"]["state"]["origin"],
        json!({"kind":"overlay","document_version":version})
    );
    assert_eq!(ack["params"]["input"]["state"]["source_sha256"], hash(text));
}

/// Retirement exposes exact decimal counters and a closed, non-RSS policy schema.
fn assert_state(state: &Value, revision: &str, publications: &str, bytes: usize) {
    assert_eq!(state["snapshot"], snapshot(revision));
    assert_eq!(state["changes"], json!([]));
    assert_eq!(state["cause"], "resource_limit");
    let report = &state["resource_limit"];
    assert_eq!(report.as_object().unwrap().len(), 7);
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["limits"], limits(publications));
    assert_eq!(report["exceeded"], json!(["accepted_publications"]));
    assert_eq!(report["checked_after"], "published-frame");
    assert_eq!(report["overshoot"], "one-operation");
    assert_eq!(report["process_rss_limit"], false);
    assert_eq!(report["counters"].as_object().unwrap().len(), KEYS.len());
    for key in KEYS {
        let decimal = report["counters"][key].as_str().unwrap();
        assert_eq!(decimal.parse::<u64>().unwrap().to_string(), decimal);
    }
    assert_eq!(report["counters"]["accepted_publications"], publications);
    assert_eq!(
        report["counters"]["cumulative_ready_source_bytes"],
        bytes.to_string()
    );
}

/// The separately framed notice follows the ACK before any subsequent request is sent.
fn retirement(client: &mut Client, revision: &str, publications: &str, bytes: usize) -> Value {
    let notice = client.receive();
    assert_eq!(notice["method"], "eska/restartRequired");
    assert!(notice.get("id").is_none());
    assert_state(&notice["params"], revision, publications, bytes);
    notice["params"].clone()
}

/// Retired reads and inputs keep the same published snapshot before graceful shutdown.
fn assert_blocked(client: &mut Client, state: &Value, document: &str) {
    let response = client.ask(&request(
        json!("retired-read"),
        "eska/metadata/read",
        Value::Null,
    ));
    assert_eq!(response["id"], "retired-read");
    assert_eq!(
        response["error"]["data"]["kind"],
        "project_restart_required"
    );
    assert_eq!(response["error"]["data"]["details"], *state);
    let rejected = client.ask(&change(document, 8, "// Newer caller-owned draft Я😀\r\n"));
    assert_eq!(rejected["method"], "eska/inputRejected");
    assert_eq!(rejected["params"]["operation"], "textDocument/didChange");
    assert_eq!(rejected["params"]["snapshot"], state["snapshot"]);
    assert_eq!(
        rejected["params"]["error"]["data"]["kind"],
        "project_restart_required"
    );
    assert_eq!(rejected["params"]["error"]["data"]["details"], *state);
    client.finish();
    assert_eq!(client.child.try_wait().unwrap().unwrap().code(), Some(0));
}

/// Both YAML and BSL acknowledge exact drafts before the reached threshold retires their child.
#[test]
fn yaml_and_bsl_ack_precedes_resource_notice_in_both_locales() {
    let (_directory, root) = support::project("// Saved body.\r\n", false);
    let source = root.join("src");
    let saved_yaml = fs::read_to_string(source.join(OWNER)).unwrap();
    let saved_bsl = fs::read(source.join(MODULE)).unwrap();
    let settings = fs::read(root.join("eska.toml")).unwrap();
    let yaml_draft = support::property::server_value(&saved_yaml, "false");
    for (relative, language, kind, text) in [
        (OWNER, "eska-source", "metadata", yaml_draft.as_str()),
        (MODULE, "bsl", "module", "\u{feff}// Current draft Я😀\r\n"),
    ] {
        for locale in ["ru-RU", "en-US"] {
            let mut client = start(&root, locale, Some(true), "1");
            let document = uri(&source, relative);
            let ack = client.ask(&open(&document, language, text));
            assert_ack(&ack, relative, kind, "1", 7, text);
            let state = retirement(&mut client, "1", "1", text.len());
            assert_blocked(&mut client, &state, &document);
        }
    }
    assert_eq!(fs::read_to_string(source.join(OWNER)).unwrap(), saved_yaml);
    assert_eq!(fs::read(source.join(MODULE)).unwrap(), saved_bsl);
    assert_eq!(fs::read(root.join("eska.toml")).unwrap(), settings);
    assert!(!root.join(".eska").exists());
}

/// Omitted and false opt-ins still enforce the limit without inserting a notification frame.
#[test]
fn unnegotiated_clients_observe_retirement_on_the_next_rpc() {
    let (_directory, root) = support::project("// Saved.\n", false);
    let document = uri(&root.join("src"), MODULE);
    let text = "// Unnegotiated draft Я😀\r\n";
    for opt_in in [None, Some(false)] {
        for locale in ["ru-RU", "en-US"] {
            let mut client = start(&root, locale, opt_in, "1");
            let ack = client.ask(&open(&document, "bsl", text));
            assert_ack(&ack, MODULE, "module", "1", 7, text);
            let response = client.ask(&request(
                json!("first-after-ack"),
                "eska/metadata/read",
                Value::Null,
            ));
            assert_eq!(response["id"], "first-after-ack");
            assert_eq!(
                response["error"]["data"]["kind"],
                "project_restart_required"
            );
            let state = &response["error"]["data"]["details"];
            assert_state(state, "1", "1", text.len());
            assert_blocked(&mut client, state, &document);
        }
    }
}

/// Stale and undeclared inputs cannot spend a publication or cumulative-source allowance.
#[test]
fn rejected_inputs_do_not_advance_the_wire_resource_boundary() {
    let (_directory, root) = support::project("// Saved.\n", false);
    let source = root.join("src");
    let document = uri(&source, MODULE);
    let first = "// First Я😀\r\n";
    let second = "// Second editor version Я😀\r\n";
    for locale in ["ru-RU", "en-US"] {
        let mut client = start(&root, locale, Some(true), "2");
        assert_ack(
            &client.ask(&open(&document, "bsl", first)),
            MODULE,
            "module",
            "1",
            7,
            first,
        );
        let unknown = client.ask(&open(
            &uri(&source, "Unknown/Module.bsl"),
            "bsl",
            "// Unknown\n",
        ));
        assert_eq!(unknown["method"], "eska/inputRejected");
        assert_eq!(unknown["params"]["error"]["data"]["kind"], "unknown_source");
        assert_eq!(unknown["params"]["snapshot"], snapshot("1"));
        let rejected_version = client.ask(&change(
            &document,
            7,
            "// Rejected duplicate version with more bytes\n",
        ));
        assert_eq!(rejected_version["method"], "eska/inputRejected");
        assert_eq!(
            rejected_version["params"]["error"]["data"]["kind"],
            "stale_document_version"
        );
        assert_eq!(rejected_version["params"]["snapshot"], snapshot("1"));
        assert_ack(
            &client.ask(&change(&document, 8, second)),
            MODULE,
            "module",
            "2",
            8,
            second,
        );
        let state = retirement(&mut client, "2", "2", first.len() + second.len());
        assert_blocked(&mut client, &state, &document);
    }
    assert!(!source.join("Unknown").exists());
}

/// Standalone SDBL keeps its exact accepted text boundary before process retirement.
#[test]
fn query_input_ack_precedes_resource_notice_in_both_locales() {
    let project = support::query_project::project("configuration");
    let before = support::query_project::snapshot(&project);
    let document = uri(&project.root.join("src"), project.source.as_str());
    let text = support::query_project::BAD;
    for locale in ["ru-RU", "en-US"] {
        let mut client = start(&project.root, locale, Some(true), "1");
        let ack = client.ask(&open(&document, "sdbl", text));
        assert_ack(&ack, project.source.as_str(), "query", "1", 7, text);
        let state = retirement(&mut client, "1", "1", text.len());
        assert_blocked(&mut client, &state, &document);
    }
    assert_eq!(support::query_project::snapshot(&project), before);
}

/// XML rights resources share the same accepted-publication and exact-byte rotation contract.
#[test]
fn rights_input_ack_precedes_resource_notice_in_both_locales() {
    let (directory, root) = support::project("// Saved.\n", false);
    support::role_project::add_roles(&root, &directory.0.join("native"));
    let source = root.join("src-with-roles");
    let relative = "Roles/Reader/Ext/Rights.xml";
    let original = fs::read(source.join(relative)).unwrap();
    let document = uri(&source, relative);
    let text = support::role_project::rights("Catalog.Items.Attribute.Code", true);
    for locale in ["ru-RU", "en-US"] {
        let mut client = start(&root, locale, Some(true), "1");
        let ack = client.ask(&open(&document, "xml", &text));
        assert_ack(&ack, relative, "role_rights", "1", 7, &text);
        let state = retirement(&mut client, "1", "1", text.len());
        assert_blocked(&mut client, &state, &document);
    }
    assert_eq!(fs::read(source.join(relative)).unwrap(), original);
}

/// Invalid opt-in types fail before discovery can read the deliberately invalid project settings.
#[test]
fn invalid_resource_opt_in_is_rejected_before_project_reads() {
    let directory = support::TestDirectory::new();
    let settings = directory.0.join("eska.toml");
    fs::write(&settings, "invalid neighboring TOML").unwrap();
    for value in [Value::Null, json!("true"), json!(1), json!({}), json!([])] {
        for locale in ["ru-RU", "en-US"] {
            let mut client = Client::with_options(&directory.0, locale, &[]);
            let mut init = initialize();
            init["params"]["initializationOptions"]["eska"]["resourceRestart"] = value.clone();
            let response = client.ask(&init);
            assert_eq!(response["id"], 1);
            assert_eq!(response["error"]["code"], -32602);
            assert_eq!(response["error"]["data"]["kind"], "rpc_invalid_params");
            assert!(response.get("result").is_none());
            client.send(&notification("exit"));
            assert_eq!(client.child.wait().unwrap().code(), Some(1));
        }
    }
    assert_eq!(
        fs::read_to_string(settings).unwrap(),
        "invalid neighboring TOML"
    );
    assert!(!directory.0.join(".eska").exists());
}

/// Full-width limits remain decimal strings and do not retire initialization or a read-only page.
#[test]
fn maximum_process_limits_round_trip_without_json_precision_loss() {
    let (_directory, root) = support::project("// Saved.\n", false);
    for locale in ["ru-RU", "en-US"] {
        let mut client = start(&root, locale, Some(true), MAX);
        let page = client.ask(&request(
            json!("owners"),
            "eska/metadata/owners",
            json!({"snapshot":snapshot("0"),"offset":0,"limit":200}),
        ));
        assert_eq!(page["id"], "owners");
        assert_eq!(page["result"]["snapshot"], snapshot("0"));
        client.finish();
        assert_eq!(client.child.try_wait().unwrap().unwrap().code(), Some(0));
    }
}

/// An already exceeded native limit cannot retire initialization before the client becomes ready.
#[test]
fn zero_native_limit_is_checked_only_after_initialization_completes() {
    let (_directory, root) = support::project("// Saved.\n", false);
    for locale in ["ru-RU", "en-US"] {
        let mut client = Client::with_options(
            &root,
            locale,
            &[
                "--max-process-source-bytes",
                MAX,
                "--max-process-input-publications",
                MAX,
                "--max-type-kernel-entries",
                "0",
                "--max-interner-norm-entries",
                MAX,
            ],
        );
        let mut init = initialize();
        init["params"]["initializationOptions"]["eska"]["resourceRestart"] = json!(true);
        let response = client.ask(&init);
        assert_eq!(response["id"], 1);
        assert_eq!(
            response["result"]["capabilities"]["experimental"]["eska"]["snapshot"],
            snapshot("0")
        );
        let before_ready = client.ask(&request(
            json!("not-ready"),
            "eska/metadata/owners",
            Value::Null,
        ));
        assert_eq!(before_ready["id"], "not-ready");
        assert_eq!(before_ready["error"]["code"], -32002);
        client.send(&notification("initialized"));
        let notice = client.receive();
        assert_eq!(notice["method"], "eska/restartRequired");
        let state = &notice["params"];
        assert_eq!(state["snapshot"], snapshot("0"));
        assert_eq!(state["cause"], "resource_limit");
        assert_eq!(
            state["resource_limit"]["exceeded"],
            json!(["type_kernel_entries"])
        );
        assert_eq!(
            state["resource_limit"]["limits"]["type_kernel_entries"],
            "0"
        );
        assert_eq!(
            state["resource_limit"]["counters"]["accepted_publications"],
            "0"
        );
        assert_blocked(&mut client, state, &uri(&root.join("src"), MODULE));
    }
}

/// Both localized help screens expose every process lifetime flag without project discovery.
#[test]
fn process_limit_flags_are_present_in_both_localized_help_screens() {
    let directory = support::TestDirectory::new();
    for locale in ["ru-RU", "en-US"] {
        let output = Command::new(env!("CARGO_BIN_EXE_eska-analyzer"))
            .current_dir(&directory.0)
            .env("PATH", "")
            .args(["--lang", locale, "--help"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        let help = String::from_utf8(output.stdout).unwrap();
        for flag in [
            "--max-process-source-bytes",
            "--max-process-input-publications",
            "--max-type-kernel-entries",
            "--max-interner-norm-entries",
        ] {
            assert!(help.contains(flag), "{flag}: {help}");
        }
        assert!(help.contains(if locale == "ru-RU" {
            "Ротация IDE-процесса"
        } else {
            "Rotate the IDE process"
        }));
    }
    assert!(!directory.0.join(".eska").exists());
}
