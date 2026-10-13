//! New supplier authority is selected explicitly; unresolved source changes never get published.

use super::*;

/// Supply a declared module rather than inferring ownership from an arbitrary BSL filename.
fn supplies(test: &TestDir) -> (String, String) {
    native(&test.0);
    let root = test.0.join("native/Configuration.xml");
    fs::write(
        &root,
        fs::read_to_string(&root).unwrap().replace(
            "</Configuration>",
            "<ChildObjects><CommonModule>Tools</CommonModule></ChildObjects></Configuration>",
        ),
    )
    .unwrap();
    let common = test.0.join("native/CommonModules");
    fs::create_dir_all(common.join("Tools/Ext")).unwrap();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/eska-format/designer/CommonModules/Tools.xml"),
        common.join("Tools.xml"),
    )
    .unwrap();
    fs::write(
        common.join("Tools/Ext/Module.bsl"),
        b"\xef\xbb\xbf// Accepted module\r\n",
    )
    .unwrap();
    let old = import(&test.0, "old", "old-label");
    fs::write(
        &root,
        fs::read_to_string(&root)
            .unwrap()
            .replace("Accepted", "Incoming descriptor"),
    )
    .unwrap();
    let new = import(&test.0, "incoming", "incoming-label");
    (old, new)
}

/// RU/EN share exact report bytes; publication keeps user data and independent approvals separate.
#[test]
fn preview_and_write_select_only_the_two_approved_bases() {
    let test = TestDir::new();
    let (old_pin, new_pin) = supplies(&test);
    let old = test.0.join("old");
    let local = b"\xef\xbb\xbf// Local module edit\r\n";
    fs::write(old.join("src/CommonModules/Tools/Ext/Module.bsl"), local).unwrap();
    fs::write(
        test.0
            .join("incoming/src/CommonModules/Tools/Ext/Module.bsl"),
        "// Incoming unapproved edit",
    )
    .unwrap();
    let settings = fs::read(old.join("eska.toml")).unwrap();
    let args = [
        "--format",
        "json",
        "update-base",
        "../incoming",
        "../updated",
        "--accepted-base-sha256",
        &old_pin,
        "--incoming-base-sha256",
        &new_pin,
    ];
    let ru = success(&old, "ru", &args);
    assert_eq!(ru, success(&old, "en", &args));
    let data = &ru["data"];
    assert_eq!(ru["command"], "update-base");
    assert_eq!(data["previous_baseline"], old_pin);
    assert_eq!(data["baseline"], new_pin);
    assert_eq!(data["can_write"], true);
    assert_eq!(data["written"], false);
    assert_eq!(data["incoming_working_source_used"], false);
    assert_eq!(data["ci_pin_updated"], false);
    assert!(!test.0.join("updated").exists());
    let write: Vec<_> = args.into_iter().chain(["--write"]).collect();
    assert_eq!(
        success(&old, "ru", &write)["data"]["publication"],
        "created"
    );
    assert_eq!(
        success(&old, "en", &write)["data"]["publication"],
        "unchanged"
    );
    let updated = test.0.join("updated");
    assert_eq!(fs::read(updated.join("eska.toml")).unwrap(), settings);
    assert_eq!(
        fs::read(updated.join("src/CommonModules/Tools/Ext/Module.bsl")).unwrap(),
        local
    );
    assert!(
        fs::read_to_string(updated.join("src/configuration.yaml"))
            .unwrap()
            .contains("Incoming descriptor")
    );
    for locale in ["ru", "en"] {
        success(
            &updated,
            locale,
            &[
                "--format",
                "json",
                "prepare",
                "--accepted-base-sha256",
                &new_pin,
            ],
        );
        let rejected = tool(
            &updated,
            locale,
            &[
                "--format",
                "json",
                "prepare",
                "--accepted-base-sha256",
                &old_pin,
            ],
        );
        assert_eq!(rejected.status.code(), Some(1));
        let error: Value = serde_json::from_slice(&rejected.stdout).unwrap();
        assert_eq!(error["error"]["code"], "source_baseline_untrusted");
    }
    assert_eq!(
        fs::read(old.join("src/CommonModules/Tools/Ext/Module.bsl")).unwrap(),
        local
    );
}

/// Conflicts are available in read-only preview and localized human output; write returns all details.
#[test]
fn concurrent_changes_and_wrong_approval_have_stable_locale_free_errors() {
    let test = TestDir::new();
    let (old_pin, _) = supplies(&test);
    fs::write(
        test.0.join("native/CommonModules/Tools/Ext/Module.bsl"),
        "// Supplier module\n",
    )
    .unwrap();
    let new_pin = import(&test.0, "conflicting", "conflicting");
    let old = test.0.join("old");
    let local = b"// Keep local code\r\n";
    fs::write(old.join("src/CommonModules/Tools/Ext/Module.bsl"), local).unwrap();
    let args = [
        "--format",
        "json",
        "update-base",
        "../conflicting",
        "../refused",
        "--accepted-base-sha256",
        &old_pin,
        "--incoming-base-sha256",
        &new_pin,
    ];
    let preview = success(&old, "ru", &args);
    assert_eq!(preview, success(&old, "en", &args));
    assert_eq!(preview["data"]["can_write"], false);
    assert!(
        preview["data"]["conflicts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|conflict| conflict["kind"] == "both_modified")
    );
    let write: Vec<_> = args.into_iter().chain(["--write"]).collect();
    let mut errors = Vec::new();
    for (locale, phrase) in [
        ("ru", "Согласуйте конфликты"),
        ("en", "Reconcile the reported conflicts"),
    ] {
        let output = tool(&old, locale, &write);
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains(phrase));
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["status"], "error");
        assert_eq!(value["error"]["code"], "source_update_conflicts");
        assert_eq!(
            value["error"]["diagnostics"]["conflicts"],
            preview["data"]["conflicts"]
        );
        errors.push(value);
        let mut human: Vec<_> = args.into_iter().skip(2).collect();
        human.extend(["--format", "human"]);
        let output = tool(&old, locale, &human);
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("CommonModules/Tools/Ext/Module.bsl")
        );
    }
    assert_eq!(errors[0], errors[1]);
    assert!(!test.0.join("refused").exists());
    assert_eq!(
        fs::read(old.join("src/CommonModules/Tools/Ext/Module.bsl")).unwrap(),
        local
    );
    let wrong = tool(
        &old,
        "en",
        &[
            "--format",
            "json",
            "update-base",
            "../incoming",
            "../refused",
            "--accepted-base-sha256",
            &old_pin,
            "--incoming-base-sha256",
            &"0".repeat(64),
        ],
    );
    assert_eq!(wrong.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<Value>(&wrong.stdout).unwrap()["error"]["code"],
        "source_baseline_untrusted"
    );
}
