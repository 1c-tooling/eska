//! A project copy preserves current edits and never promotes them to the independently pinned base.

use super::*;
use eska::project_core::source_format::{AcceptedBase, AcceptedRef, SourceReader};

/// Default preview is locale-free and read-only; explicit publication leaves the original usable.
#[test]
fn migration_preview_and_publication_preserve_working_edits_and_independent_base() {
    let test = TestDir::new();
    native(&test.0);
    let pin = import(&test.0, "old", "sample");
    let old = test.0.join("old");
    let yaml = fs::read_to_string(old.join("src/configuration.yaml")).unwrap();
    fs::write(
        old.join("src/configuration.yaml"),
        yaml.replace("Accepted", "Working edit"),
    )
    .unwrap();
    let settings = fs::read(old.join("eska.toml")).unwrap();
    let source = fs::read(old.join("src/configuration.yaml")).unwrap();
    let authority = fs::read(old.join(".eska/accepted/accepted.json")).unwrap();
    fs::write(old.join("notes.md"), "User notes").unwrap();
    let mut previews = Vec::new();
    for locale in ["ru", "en"] {
        previews.push(success(
            &old,
            locale,
            &[
                "migrate",
                "../new",
                "--accepted-base-sha256",
                &pin,
                "--format",
                "json",
            ],
        ));
        assert!(!test.0.join("new").exists());
    }
    assert_eq!(previews[0], previews[1]);
    let data = &previews[0]["data"];
    assert_eq!(data["source_layout"], "readable");
    assert_eq!(data["written"], false);
    assert_eq!(data["publication"], Value::Null);
    assert_eq!(data["ci_pin_updated"], false);
    assert!(!data["source_changes"].as_array().unwrap().is_empty());
    assert!(!data["accepted_changes"].as_array().unwrap().is_empty());
    let new_pin = data["baseline"].as_str().unwrap();
    assert_ne!(pin, new_pin);
    let written = success(
        &old,
        "ru",
        &[
            "migrate",
            "../new",
            "--accepted-base-sha256",
            &pin,
            "--write",
            "--format",
            "json",
        ],
    );
    assert_eq!(written["data"]["publication"], "created");
    assert_eq!(written["data"]["baseline"], new_pin);
    assert_eq!(written["data"]["source_changes"], data["source_changes"]);
    let new = test.0.join("new");
    assert_eq!(fs::read(new.join("eska.toml")).unwrap(), settings);
    assert!(
        fs::read_to_string(new.join("src/configuration.yaml"))
            .unwrap()
            .contains("Working edit")
    );
    assert_eq!(
        SourceReader::open(&new.join("src"))
            .unwrap()
            .manifest()
            .schema_version,
        3
    );
    let base = AcceptedBase::open(
        &new.join(".eska/accepted"),
        &AcceptedRef {
            sha256: new_pin.into(),
        },
    )
    .unwrap();
    base.restore(&test.0.join("restored")).unwrap();
    assert!(
        !fs::read_to_string(test.0.join("restored/configuration.yaml"))
            .unwrap()
            .contains("Working edit")
    );
    assert_prepared_project(&new, new_pin);
    assert_eq!(fs::read(old.join("eska.toml")).unwrap(), settings);
    assert_eq!(
        fs::read(old.join("src/configuration.yaml")).unwrap(),
        source
    );
    assert_eq!(
        fs::read(old.join(".eska/accepted/accepted.json")).unwrap(),
        authority
    );
    assert_eq!(
        fs::read_to_string(old.join("notes.md")).unwrap(),
        "User notes"
    );
    assert!(!new.join("notes.md").exists());
    assert!(!new.join(".git").exists());
}

/// Exercise protected export separately from inspecting migration publication and old inputs.
fn assert_prepared_project(root: &Path, pin: &str) {
    let prepared: Vec<_> = ["ru", "en"]
        .into_iter()
        .map(|locale| {
            success(
                root,
                locale,
                &["prepare", "--accepted-base-sha256", pin, "--format", "json"],
            )
        })
        .collect();
    assert_eq!(prepared[0], prepared[1]);
    let xml = Path::new(prepared[0]["data"]["xml"].as_str().unwrap());
    assert!(
        fs::read_to_string(xml.join("Configuration.xml"))
            .unwrap()
            .contains("Working edit")
    );
    assert_eq!(
        fs::read(xml.join("Module.bsl")).unwrap(),
        b"\xef\xbb\xbf// native\r\n"
    );
}

/// Exact repeat writes nothing, and an already readable copy retains its independent pin and YAML.
#[test]
fn migration_repeat_and_readable_noop_preserve_exact_inputs() {
    let test = TestDir::new();
    native(&test.0);
    let pin = import(&test.0, "old", "sample");
    let old = test.0.join("old");
    let initial = success(
        &old,
        "en",
        &[
            "migrate",
            "../new",
            "--accepted-base-sha256",
            &pin,
            "--write",
            "--format",
            "json",
        ],
    );
    let mut repeated = Vec::new();
    for locale in ["ru", "en"] {
        repeated.push(success(
            &old,
            locale,
            &[
                "migrate",
                "../new",
                "--accepted-base-sha256",
                &pin,
                "--write",
                "--format",
                "json",
            ],
        ));
    }
    assert_eq!(repeated[0], repeated[1]);
    assert_eq!(repeated[0]["data"]["publication"], "unchanged");
    let new_pin = initial["data"]["baseline"].as_str().unwrap();
    let new = test.0.join("new");
    let next = success(
        &new,
        "ru",
        &[
            "migrate",
            "../again",
            "--accepted-base-sha256",
            new_pin,
            "--write",
            "--format",
            "json",
        ],
    );
    assert_eq!(next["data"]["baseline"], new_pin);
    assert_eq!(next["data"]["layout_changed"], false);
    assert!(
        next["data"]["source_changes"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        next["data"]["accepted_changes"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        fs::read(new.join("src/configuration.yaml")).unwrap(),
        fs::read(test.0.join("again/src/configuration.yaml")).unwrap()
    );
}

/// Missing/bad pins and foreign destinations fail identically in both locales without writes.
#[test]
fn migration_refusals_and_help_work_in_both_locales() {
    let test = TestDir::new();
    native(&test.0);
    let pin = import(&test.0, "old", "sample");
    let old = test.0.join("old");
    let invalid = "0".repeat(64);
    let mut errors = Vec::new();
    for locale in ["ru", "en"] {
        let help = tool(&old, locale, &["migrate", "--help"]);
        assert_eq!(help.status.code(), Some(0));
        assert!(help.stderr.is_empty());
        let help_text = String::from_utf8_lossy(&help.stdout);
        assert!(help_text.contains("--write"));
        assert!(help_text.contains("--accepted-base-sha256"));
        let mut selected = Vec::new();
        for (args, code) in [
            (
                vec!["migrate", "../refused", "--format", "json"],
                "accepted_base_pin_required",
            ),
            (
                vec![
                    "migrate",
                    "../refused",
                    "--accepted-base-sha256",
                    &invalid,
                    "--write",
                    "--format",
                    "json",
                ],
                "source_baseline_untrusted",
            ),
            (
                vec![
                    "migrate",
                    "nested",
                    "--accepted-base-sha256",
                    &pin,
                    "--write",
                    "--format",
                    "json",
                ],
                "source_tree_overlap",
            ),
        ] {
            let output = tool(&old, locale, &args);
            assert_eq!(output.status.code(), Some(1));
            assert!(!output.stderr.is_empty());
            let value: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(value["command"], "migrate");
            assert_eq!(value["error"]["code"], code);
            selected.push(value);
        }
        errors.push(selected);
    }
    assert_eq!(errors[0], errors[1]);
    assert!(!test.0.join("refused").exists());
    assert!(!old.join("nested").exists());
    fs::write(old.join("src/configuration.yaml"), "invalid yaml").unwrap();
    let output = tool(
        &old,
        "en",
        &[
            "migrate",
            "../refused",
            "--accepted-base-sha256",
            &pin,
            "--write",
            "--format",
            "json",
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(!test.0.join("refused").exists());
}

/// Workspace inheritance and incomplete project publication produce explicit localized failures.
#[test]
fn migration_refuses_workspace_scope_and_blocks_incomplete_project_consumers() {
    let test = TestDir::new();
    native(&test.0);
    let pin = import(&test.0, "old", "sample");
    let old = test.0.join("old");
    fs::write(
        test.0.join("eska.toml"),
        "[workspace]\nmembers = [\"old\"]\n",
    )
    .unwrap();
    for locale in ["ru", "en"] {
        let output = tool(
            &old,
            locale,
            &[
                "migrate",
                "../refused",
                "--accepted-base-sha256",
                &pin,
                "--write",
                "--format",
                "json",
            ],
        );
        assert_eq!(output.status.code(), Some(1));
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["error"]["code"], "source_migration_scope_unsupported");
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .to_lowercase()
                .contains("workspace")
        );
    }
    fs::remove_file(test.0.join("eska.toml")).unwrap();
    fs::write(
        old.join(eska::project_core::source_format::PROJECT_LAYOUT_RECEIPT),
        "unready",
    )
    .unwrap();
    for locale in ["ru", "en"] {
        let output = tool(&old, locale, &["inspect", "--format", "json"]);
        assert_eq!(output.status.code(), Some(1));
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["error"]["code"], "source_migration_incomplete");
    }
    assert!(!test.0.join("refused").exists());
}
