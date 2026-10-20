//! The framed protocol uses the same plans as the standalone CLI, without starting LSP.

use super::*;
use std::fs;

/// Negotiate the additive configuration API without changing legacy client behavior.
fn initialize(client: &mut Client, locale: &str) {
    let response=client.ok("initialize",json!({"apiVersion":{"major":1,"minor":12},"client":{"name":"config-test","version":"1"},"locale":locale}));
    assert_eq!(response["capabilities"]["analyzerConfiguration"], true);
}

/// Encode absolute native paths with the established DTO rather than implicit cwd resolution.
fn start(root: &std::path::Path) -> Value {
    json!({"start":{"value":root,"encoding":"utf-8"}})
}

/// Legacy clients retain the tree; new clients get locale-independent read-only config state.
#[test]
fn context_and_freshness_are_read_only_and_negotiate_api_12() {
    let fixture_dir = TestDir::new();
    fixture(&fixture_dir.0, "configuration");
    let mut old = Client::new("en-US");
    old.initialize("en-US");
    let rejected = old.request("analyzer/context", start(&fixture_dir.0));
    assert_eq!(rejected["error"]["code"], -32601);
    old.open(&fixture_dir.0);
    old.finish();
    let mut expected = None;
    for locale in ["ru-RU", "en-US"] {
        let mut client = Client::new(locale);
        initialize(&mut client, locale);
        let context = client.ok("analyzer/context", start(&fixture_dir.0));
        let status = client.ok("analyzer/configStatus", start(&fixture_dir.0));
        assert_eq!(status["status"], "missing");
        if let Some(expected) = &expected {
            assert_eq!(&(context, status), expected);
        } else {
            expected = Some((context, status));
        }
        let mut invalid = start(&fixture_dir.0);
        invalid["application"] = Value::Null;
        let rejected = client.request("analyzer/configStatus", invalid);
        assert_eq!(rejected["error"]["code"], -32602);
        client.finish();
    }
    assert!(!fixture_dir.0.join(".eska").exists());
    assert!(!fixture_dir.0.join("bsl-analyzer.toml").exists());
}

/// Only orchestration is mocked here; upstream validation has a separate explicit real-binary test.
#[cfg(unix)]
fn fake(root: &std::path::Path) -> std::path::PathBuf {
    let path = root.join("analyzer fixture");
    // Keep the executable inode immutable while parallel tests spawn child processes.
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/analyzer-config.py");
    std::os::unix::fs::symlink(fixture, &path).unwrap();
    path
}

/// Cancellation reaches an already running validator and cannot publish its candidate.
#[cfg(unix)]
#[test]
fn running_validation_honors_cancellation_and_disconnect() {
    for disconnect in [false, true] {
        let directory = TestDir::new();
        fixture(&directory.0, "configuration");
        let binary = fake(&directory.0);
        fs::write(directory.0.join("sleep"), b"").unwrap();
        let mut client = Client::new("en-US");
        initialize(&mut client, "en-US");
        let mut args = start(&directory.0);
        args["analyzer"] = json!({"value":binary,"encoding":"utf-8"});
        client.send(
            &json!({"jsonrpc":"2.0","id":999,"method":"analyzer/configPrepare","params":args}),
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while !directory.0.join("check-started").exists() {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        if disconnect {
            drop(client.input.take());
            assert_eq!(client.wait_exit(), 0);
        } else {
            client.send(&json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":999}}));
            let response = client.receive();
            assert_eq!(response["id"], 999);
            assert_eq!(response["error"]["data"]["details"]["code"], "cancelled");
            client.finish();
        }
        assert!(!directory.0.join("bsl-analyzer.toml").exists());
        #[cfg(target_os = "linux")]
        {
            let pid = fs::read_to_string(directory.0.join("check-started")).unwrap();
            assert!(!std::path::Path::new("/proc").join(pid).exists());
        }
    }
}

/// Human upstream logs use a notification, leaving the error envelope stable for either UI locale.
#[cfg(unix)]
#[test]
fn validation_failure_reports_upstream_output_without_localized_json_errors() {
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    let binary = fake(&directory.0);
    fs::write(directory.0.join("reject"), b"").unwrap();
    let mut client = Client::new("ru-RU");
    initialize(&mut client, "ru-RU");
    let mut args = start(&directory.0);
    args["analyzer"] = json!({"value":binary,"encoding":"utf-8"});
    let response = client.request("analyzer/configPrepare", args);
    assert_eq!(
        response["error"]["data"]["details"],
        json!({"code":"check_config","data":{"exit_code":3}})
    );
    assert!(client.events.iter().any(|event| {
        event["method"] == "analyzer/validationOutput"
            && event["params"]["stderr"]
                .as_str()
                .is_some_and(|text| text.contains("rejected fixture"))
    }));
    assert!(!directory.0.join("bsl-analyzer.toml").exists());
    client.finish();
}

/// Two real protocol processes share a context and release their leases independently of the tree.
#[test]
#[ignore = "requires ESKA_BSL_ANALYZER pointing to published bsl-analyzer-app 0.3.2"]
fn published_validation_and_multi_client_leases_use_the_shared_generator() {
    let binary = std::env::var_os("ESKA_BSL_ANALYZER")
        .map(std::path::PathBuf::from)
        .expect("explicit upstream executable");
    let directory = TestDir::new();
    fixture(&directory.0, "configuration");
    let mut first = Client::new("ru-RU");
    initialize(&mut first, "ru-RU");
    let mut second = Client::new("en-US");
    initialize(&mut second, "en-US");
    let mut args = start(&directory.0);
    args["analyzer"] = json!({"value":binary,"encoding":"utf-8"});
    let prepared = first.ok("analyzer/configPrepare", args.clone());
    assert_eq!(prepared["status"], "current");
    let first_lease = first.ok("analyzer/acquire", start(&directory.0));
    let second_lease = second.ok("analyzer/acquire", start(&directory.0));
    assert_eq!(first_lease, second_lease);
    first.open(&directory.0);
    second.open(&directory.0);
    let manifest = directory.0.join("eska.toml");
    let mut contents = fs::read(&manifest).unwrap();
    contents.extend_from_slice(b"\n# context input changed\n");
    fs::write(&manifest, contents).unwrap();
    let rejected = first.request("analyzer/configPrepare", args.clone());
    assert_eq!(rejected["error"]["data"]["details"]["code"], "busy");
    first.ok(
        "analyzer/release",
        json!({"leaseId":first_lease["leaseId"]}),
    );
    let rejected = first.request("analyzer/configPrepare", args.clone());
    assert_eq!(rejected["error"]["data"]["details"]["code"], "busy");
    second.ok(
        "analyzer/release",
        json!({"leaseId":second_lease["leaseId"]}),
    );
    let prepared = first.ok("analyzer/configPrepare", args);
    assert_eq!(prepared["status"], "current");
    assert_eq!(prepared["changed"], true);
    first.finish();
    second.finish();
}
