//! An independent framed client exercises recovery with only the standalone project executable.

use super::*;
use eska_project_core::project::metadata_edit::snapshot;

const ID: &str = "10000000-0000-4000-8000-000000000001";
const PARTS: [&str; 4] = ["metadata", "ownerText", "base", "draft"];

/// Each test owns a project and a separate private checkpoint directory in the sibling playground.
fn fixture() -> (support::TestDir, std::path::PathBuf, std::path::PathBuf) {
    let directory = support::TestDir::new();
    let root = directory.0.join("Проект");
    create::create(
        &root,
        &ProjectConfig::new(ProjectType::Configuration)
            .with_source("src".into())
            .unwrap(),
    )
    .unwrap();
    fs::write(root.join("src/Configuration.xml"),
        "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><Configuration uuid='11111111-1111-1111-1111-111111111111'><Properties><Name>Fixture</Name></Properties><ChildObjects/></Configuration></MetaDataObject>").unwrap();
    let private = directory.0.join("private recovery");
    (directory, root, private)
}

/// Recovery has its own optional capability and never requires source-edit permission.
fn initialize(client: &mut Client, locale: &str) {
    let hello = client.request(
        "initialize",
        &json!({"apiVersion":{"major":1,"minor":14},
        "client":{"name":"independent-recovery-test","version":"1"},"locale":locale}),
    );
    assert_eq!(hello["capabilities"]["queryRecovery"]["schemaVersion"], 1);
    assert_eq!(hello["capabilities"]["queryRecovery"]["writesOwner"], false);
    assert_eq!(hello["capabilities"]["readOnly"], true);
}

/// The journal handle is independent of the optional metadata workspace handle.
fn connect(client: &mut Client, root: &Path, private: &Path, locale: &str) -> Value {
    initialize(client, locale);
    let workspace = client.request(
        "workspace/open",
        &json!({"start":{"value":root,"encoding":"utf-8"},
        "selection":{"kind":"current"},"diskCache":false}),
    );
    let opened = client.request(
        "query/recoveryOpen",
        &json!({"directory":{"value":private,"encoding":"utf-8"}}),
    );
    assert_eq!(opened["limited"], false);
    assert_eq!(opened["damaged"], json!([]));
    json!({"sessionId":workspace["sessionId"],"storeId":opened["storeId"],"records":opened["records"]})
}

/// Opaque paths and remembered writable are persisted data, never instructions to edit those paths.
fn parts(owner: &str, draft: &str) -> [String; 4] {
    let metadata = json!({"schemaVersion":1,"id":ID,"projectKey":"same owner in another IDE",
        "node":{"kind":"object","objectId":"opaque owner"},
        "source":{"value":"Ext/Template.xml","encoding":"utf-8"},
        "descriptor":{"value":"Template.xml","encoding":"utf-8"},"descriptorSnapshot":"a".repeat(64),
        "diskSnapshot":"b".repeat(64),"ownerSnapshot":snapshot(owner),"ownerVersion":"7",
        "contextFingerprint":"c".repeat(64),"application":"app","identity":{"kind":"dataSet","path":["A"]},"writable":true});
    [
        metadata.to_string(),
        owner.into(),
        "SELECT 1".into(),
        draft.into(),
    ]
}

/// The helper composes protocol fields without using production serialization or RPC code.
fn args(binding: &Value, extra: Value) -> Value {
    let mut value = json!({"storeId":binding["storeId"]});
    let Value::Object(extra) = extra else {
        panic!("fixture arguments are an object");
    };
    value.as_object_mut().unwrap().extend(extra);
    value
}

/// Hashes identify each exact UTF-8 component independently of JSON escaping and character count.
fn descriptions(parts: &[String; 4]) -> Value {
    let mut value = json!({});
    for (name, text) in PARTS.into_iter().zip(parts) {
        value[name] = json!({"bytes":text.len(),"snapshot":snapshot(text)});
    }
    value
}

/// Acknowledged offsets determine which immutable parts can be reused without another owner upload.
fn begin(
    client: &mut Client,
    binding: &Value,
    parts: &[String; 4],
    revision: Option<&str>,
) -> Value {
    client.request(
        "query/recoveryBegin",
        &args(
            binding,
            json!({"id":ID,"expectedRevision":revision,"parts":descriptions(parts)}),
        ),
    )
}

/// Keep even JSON-control-heavy chunks below the original 1 MiB input frame limit.
fn append_parts(client: &mut Client, binding: &Value, upload: &Value, parts: &[String; 4]) {
    for (name, text) in PARTS.into_iter().zip(parts) {
        let mut offset = usize::try_from(upload["received"][name].as_u64().unwrap()).unwrap();
        assert!(offset == 0 || offset == text.len());
        while offset < text.len() {
            let mut end = (offset + 131_072).min(text.len());
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            let result = client.request(
                "query/recoveryAppend",
                &args(
                    binding,
                    json!({"updateId":upload["updateId"],
                "part":name,"offset":offset,"text":&text[offset..end]}),
                ),
            );
            assert_eq!(result["receivedBytes"], end);
            assert_eq!(result["ready"], end == text.len());
            offset = end;
        }
    }
}

/// Commit is the only publication point; staging alone never writes a source or recovery record.
fn save(
    client: &mut Client,
    binding: &Value,
    parts: &[String; 4],
    revision: Option<&str>,
) -> Value {
    let upload = begin(client, binding, parts, revision);
    append_parts(client, binding, &upload, parts);
    client.request(
        "query/recoveryCommit",
        &args(binding, json!({"updateId":upload["updateId"]})),
    )
}

/// Every chunk is checked against the header's byte count and exact final content hash.
fn read_part(client: &mut Client, binding: &Value, header: &Value, name: &str) -> String {
    let mut text = String::new();
    loop {
        let result = client.request(
            "query/recoveryRead",
            &args(
                binding,
                json!({"id":header["id"],
            "revision":header["revision"],"part":name,"offset":text.len()}),
            ),
        );
        let chunk = result["text"].as_str().unwrap();
        assert!(chunk.len() <= 131_072);
        text.push_str(chunk);
        assert_eq!(result["nextOffset"], text.len());
        if result["complete"] == true {
            break;
        }
        assert_ne!(chunk, "");
    }
    assert_eq!(header["parts"][name]["bytes"], text.len());
    assert_eq!(header["parts"][name]["snapshot"], snapshot(&text));
    text
}

/// Acknowledged dirty owners and drafts survive a killed server and a fresh process in either locale.
#[test]
fn standalone_recovery_survives_process_kill_with_exact_unicode_and_no_owner_write() {
    for locale in ["ru-RU", "en-US"] {
        let cli_locale = if locale == "ru-RU" { "ru" } else { "en" };
        let (_directory, root, private) = fixture();
        let source = fs::read(root.join("src/Configuration.xml")).unwrap();
        let manifest = fs::read(root.join("eska.toml")).unwrap();
        let owner = format!("\u{feff}// unsaved 😀\r\n{}", "\0😀\r\n".repeat(40_000));
        let values = parts(&owner, "SELECT 99\n// unfinished 😀\0");
        let mut first = Client::new(&root, cli_locale);
        let binding = connect(&mut first, &root, &private, locale);
        assert_eq!(binding["records"], json!([]));
        let committed = save(&mut first, &binding, &values, None);
        first.child.kill().unwrap();
        first.child.wait().unwrap();
        let mut restarted = Client::new(&root, cli_locale);
        let binding = connect(&mut restarted, &root, &private, locale);
        assert_eq!(binding["records"].as_array().unwrap().len(), 1);
        let header = binding["records"][0].clone();
        assert_eq!(header["revision"], committed["revision"]);
        assert_eq!(header["metadata"]["application"], "app");
        for (name, value) in PARTS.into_iter().zip(&values) {
            assert_eq!(read_part(&mut restarted, &binding, &header, name), *value);
        }
        restarted.finish();
        assert_eq!(
            fs::read(root.join("src/Configuration.xml")).unwrap(),
            source
        );
        assert_eq!(fs::read(root.join("eska.toml")).unwrap(), manifest);
        assert!(!root.join("bsl-analyzer.toml").exists());
    }
}

/// Independent backend processes cannot silently overwrite or discard the same recovered draft.
#[test]
fn standalone_recovery_reuses_unchanged_parts_and_rejects_second_window_conflicts() {
    let (_directory, root, private) = fixture();
    let values = parts("// dirty owner\r\n", "SELECT 2");
    let mut first = Client::new(&root, "en");
    let first_binding = connect(&mut first, &root, &private, "en-US");
    let committed = save(&mut first, &first_binding, &values, None);
    let revision = committed["revision"].as_str().unwrap();
    let mut second = Client::new(&root, "ru");
    let second_binding = connect(&mut second, &root, &private, "ru-RU");
    let mut updated = values.clone();
    updated[3] = "SELECT 99".into();
    let upload = begin(&mut first, &first_binding, &updated, Some(revision));
    assert_eq!(upload["received"]["ownerText"], values[1].len());
    assert_eq!(upload["received"]["base"], values[2].len());
    assert_eq!(upload["received"]["draft"], 0);
    append_parts(&mut first, &first_binding, &upload, &updated);
    let latest = first.request(
        "query/recoveryCommit",
        &args(&first_binding, json!({"updateId":upload["updateId"]})),
    );
    let upload = begin(&mut second, &second_binding, &values, Some(revision));
    append_parts(&mut second, &second_binding, &upload, &values);
    let failure = second.response(
        "query/recoveryCommit",
        &args(&second_binding, json!({"updateId":upload["updateId"]})),
    );
    assert_eq!(
        failure["error"]["data"]["details"]["reason"],
        "recovery_conflict"
    );
    let failure = second.response(
        "query/recoveryRemove",
        &args(&second_binding, json!({"id":ID,"revision":revision})),
    );
    assert_eq!(
        failure["error"]["data"]["details"]["reason"],
        "recovery_conflict"
    );
    assert_eq!(
        read_part(&mut first, &first_binding, &latest, "draft"),
        "SELECT 99"
    );
    let pending = begin(
        &mut first,
        &first_binding,
        &values,
        latest["revision"].as_str(),
    );
    first.request(
        "query/recoveryRemove",
        &args(
            &first_binding,
            json!({"id":ID,"revision":latest["revision"]}),
        ),
    );
    let failure = first.response(
        "query/recoveryCommit",
        &args(&first_binding, json!({"updateId":pending["updateId"]})),
    );
    assert_eq!(
        failure["error"]["data"]["details"]["reason"],
        "unknown_update"
    );
    let failure = first.response(
        "query/recoveryBegin",
        &args(
            &first_binding,
            json!({"id":ID,"expectedRevision":latest["revision"],"parts":descriptions(&values)}),
        ),
    );
    assert_eq!(
        failure["error"]["data"]["details"]["reason"],
        "recovery_conflict"
    );
    first.finish();
    second.finish();
}

/// Partial data, wrong offsets and checksum mismatches never publish a checkpoint.
#[test]
fn standalone_recovery_rejects_partial_or_forged_uploads_without_publishing() {
    let (_directory, root, private) = fixture();
    let mut client = Client::new(&root, "en");
    let binding = connect(&mut client, &root, &private, "en-US");
    let values = parts("// dirty owner", "SELECT 2");
    let upload = begin(&mut client, &binding, &values, None);
    let failure = client.response(
        "query/recoveryCommit",
        &args(&binding, json!({"updateId":upload["updateId"]})),
    );
    assert_eq!(
        failure["error"]["data"]["details"]["reason"],
        "upload_incomplete"
    );
    let failure = client.response(
        "query/recoveryAppend",
        &args(
            &binding,
            json!({"updateId":upload["updateId"],"part":"draft","offset":1,"text":"x"}),
        ),
    );
    assert_eq!(
        failure["error"]["data"]["details"]["reason"],
        "upload_offset"
    );
    client.request(
        "query/recoveryCancel",
        &args(&binding, json!({"updateId":upload["updateId"]})),
    );
    assert!(!private.exists());
    let mut forged = descriptions(&values);
    forged["draft"]["snapshot"] = json!("f".repeat(64));
    let upload = client.request(
        "query/recoveryBegin",
        &args(
            &binding,
            json!({"id":ID,"expectedRevision":null,"parts":forged}),
        ),
    );
    let failure = client.response(
        "query/recoveryAppend",
        &args(
            &binding,
            json!({"updateId":upload["updateId"],"part":"draft","offset":0,"text":values[3]}),
        ),
    );
    assert_eq!(
        failure["error"]["data"]["details"]["reason"],
        "snapshot_mismatch"
    );
    assert!(!private.exists());
    let mut bad_binding = values.clone();
    let mut metadata: Value = serde_json::from_str(&bad_binding[0]).unwrap();
    metadata["ownerSnapshot"] = json!("0".repeat(64));
    bad_binding[0] = metadata.to_string();
    let upload = begin(&mut client, &binding, &bad_binding, None);
    append_parts(&mut client, &binding, &upload, &bad_binding);
    let failure = client.response(
        "query/recoveryCommit",
        &args(&binding, json!({"updateId":upload["updateId"]})),
    );
    assert_eq!(
        failure["error"]["data"]["details"]["reason"],
        "recovery_invalid"
    );
    assert!(!private.exists());
    save(&mut client, &binding, &values, None);
    client.finish();
}

/// The backend refuses project storage and keeps legacy API clients read-only with respect to recovery.
#[test]
fn standalone_recovery_is_optional_and_cannot_select_a_project_source_directory() {
    let (_directory, root, private) = fixture();
    let mut legacy = Client::new(&root, "en");
    let hello = legacy.request(
        "initialize",
        &json!({"apiVersion":{"major":1,"minor":13},"client":{"name":"old","version":"1"}}),
    );
    assert!(hello["capabilities"].get("queryRecovery").is_none());
    let workspace = legacy.request("workspace/open", &json!({"start":{"value":root,"encoding":"utf-8"},"selection":{"kind":"current"},"diskCache":false}));
    let failure = legacy.response("query/recoveryOpen", &json!({"sessionId":workspace["sessionId"],"directory":{"value":private,"encoding":"utf-8"}}));
    assert_eq!(failure["error"]["code"], -32601);
    assert!(!private.exists());
    legacy.finish();
    let mut client = Client::new(&root, "en");
    let binding = connect(&mut client, &root, &private, "en-US");
    for directory in [&root, &root.join("src/recovery")] {
        let failure = client.response("query/recoveryOpen", &json!({"sessionId":binding["sessionId"],"directory":{"value":directory,"encoding":"utf-8"}}));
        assert_eq!(
            failure["error"]["data"]["details"]["reason"],
            "recovery_in_project"
        );
    }
    assert!(!root.join("src/recovery").exists());
    client.finish();
}

/// Broken metadata must not prevent preserving a draft or reading it after the metadata session closes.
#[test]
fn standalone_recovery_remains_available_when_metadata_requires_reopen() {
    let (_directory, root, private) = fixture();
    let mut client = Client::new(&root, "en");
    let binding = connect(&mut client, &root, &private, "en-US");
    let info = client.request("project/info", &json!({"sessionId":binding["sessionId"]}));
    let manifest = root.join("eska.toml");
    fs::write(
        &manifest,
        format!(
            "{}\n# context changed\n",
            fs::read_to_string(&manifest).unwrap()
        ),
    )
    .unwrap();
    client.send(
        &json!({"jsonrpc":"2.0","method":"workspace/didChangeFiles","params":{
        "sessionId":binding["sessionId"],"projectId":info["projects"][0]["projectId"],
        "sequence":"1","paths":[],"manifestChanged":true}}),
    );
    let current = client.request("project/info", &json!({"sessionId":binding["sessionId"]}));
    assert_eq!(current["projects"][0]["requiresReopen"], true);
    let failure = client.response("metadata/root", &json!({"sessionId":binding["sessionId"],
        "projectId":info["projects"][0]["projectId"],"generation":info["projects"][0]["generation"]}));
    assert_eq!(failure["error"]["data"]["kind"], "reopen_required");
    let saved = save(
        &mut client,
        &binding,
        &parts("// original dirty owner", "SELECT 7"),
        None,
    );
    client.request(
        "workspace/close",
        &json!({"sessionId":binding["sessionId"]}),
    );
    assert_eq!(
        read_part(&mut client, &binding, &saved, "draft"),
        "SELECT 7"
    );
    let failure = client.response("metadata/root", &json!({"sessionId":binding["sessionId"],
        "projectId":info["projects"][0]["projectId"],"generation":info["projects"][0]["generation"]}));
    assert_eq!(failure["error"]["data"]["kind"], "unknown_session");
    let opened = client.request("workspace/open", &json!({"start":{"value":root,"encoding":"utf-8"},"selection":{"kind":"current"},"diskCache":false}));
    assert_ne!(opened["sessionId"], binding["sessionId"]);
    let recovery = client.request(
        "query/recoveryOpen",
        &json!({"directory":{"value":private,"encoding":"utf-8"}}),
    );
    assert_eq!(recovery["records"][0]["revision"], saved["revision"]);
    let failure = client.response(
        "query/recoveryRead",
        &args(
            &binding,
            json!({"id":ID,"revision":saved["revision"],"part":"draft","offset":0}),
        ),
    );
    assert_eq!(
        failure["error"]["data"]["details"]["reason"],
        "unknown_store"
    );
    client.finish();
}

/// A fresh process can preserve and inspect recovery data even when the manifest cannot be parsed.
#[test]
fn standalone_recovery_does_not_require_a_successful_metadata_open() {
    let (_directory, root, private) = fixture();
    let broken = "[project\nmalformed fixture manifest";
    fs::write(root.join("eska.toml"), broken).unwrap();
    let source = fs::read(root.join("src/Configuration.xml")).unwrap();
    let mut client = Client::new(&root, "ru");
    initialize(&mut client, "ru-RU");
    let opened = client.request(
        "query/recoveryOpen",
        &json!({"directory":{"value":private,"encoding":"utf-8"}}),
    );
    let binding = json!({"storeId":opened["storeId"]});
    let values = parts("// dirty parent without metadata 😀\r\n", "SELECT 8");
    let saved = save(&mut client, &binding, &values, None);
    let failure = client.response("workspace/open", &json!({"start":{"value":root,"encoding":"utf-8"},"selection":{"kind":"current"},"diskCache":false}));
    assert_eq!(failure["error"]["data"]["kind"], "project_open_failed");
    assert_eq!(
        read_part(&mut client, &binding, &saved, "ownerText"),
        values[1]
    );
    assert_eq!(read_part(&mut client, &binding, &saved, "draft"), values[3]);
    assert_eq!(fs::read_to_string(root.join("eska.toml")).unwrap(), broken);
    assert_eq!(
        fs::read(root.join("src/Configuration.xml")).unwrap(),
        source
    );
    client.finish();
}

/// A context change cannot authorize new journal writes when its directory has become part of the project.
#[test]
fn standalone_recovery_rechecks_storage_location_after_metadata_reopen() {
    let (directory, root, private) = fixture();
    let mut client = Client::new(&root, "en");
    let binding = connect(&mut client, &root, &private, "en-US");
    let values = parts("// original owner", "SELECT 2");
    let saved = save(&mut client, &binding, &values, None);
    let original = fs::read(private.join(format!("{ID}.json"))).unwrap();
    client.request(
        "workspace/close",
        &json!({"sessionId":binding["sessionId"]}),
    );
    fs::copy(root.join("eska.toml"), directory.0.join("eska.toml")).unwrap();
    fs::create_dir(directory.0.join("src")).unwrap();
    fs::copy(
        root.join("src/Configuration.xml"),
        directory.0.join("src/Configuration.xml"),
    )
    .unwrap();
    client.request("workspace/open", &json!({"start":{"value":directory.0,"encoding":"utf-8"},"selection":{"kind":"current"},"diskCache":false}));
    let failure = client.response(
        "query/recoveryBegin",
        &args(
            &binding,
            json!({"id":ID,"expectedRevision":saved["revision"],"parts":descriptions(&values)}),
        ),
    );
    assert_eq!(
        failure["error"]["data"]["details"]["reason"],
        "recovery_in_project"
    );
    assert_eq!(read_part(&mut client, &binding, &saved, "draft"), values[3]);
    assert_eq!(
        fs::read(private.join(format!("{ID}.json"))).unwrap(),
        original
    );
    client.finish();
}
