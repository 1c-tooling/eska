//! Real tool attachment preserves existing source, independent history and user files in both locales.

use super::*;
use eska::project_core::{discovery::discover, native_version::VersionScheme};
use sha2::{Digest, Sha256};
use std::fmt::Write;

/// Create owned unmanaged source from the actual constructor without relying on hand-written format data.
fn unmanaged(test: &TestDir, kind: &str, retain_evidence: bool) -> Value {
    let created = success(
        &test.0,
        "en",
        &[
            "new", "project", "--name", "original", "--type", kind, "--format", "json",
        ],
    );
    let root = test.0.join("project");
    fs::remove_file(root.join("eska.toml")).unwrap();
    if !retain_evidence {
        fs::remove_dir_all(root.join(".eska")).unwrap();
    }
    created["data"].clone()
}

/// All profiles and locales publish the same schema while authoring and unrelated user bytes stay exact.
#[test]
fn raw_sources_initialize_all_profiles_in_both_locales_without_platform_or_git() {
    for kind in ["configuration", "extension", "processing", "report"] {
        for locale in ["ru", "en"] {
            let test = TestDir::new();
            unmanaged(&test, kind, false);
            let root = test.0.join("project");
            fs::write(root.join("user.txt"), b"user text\r\n").unwrap();
            fs::write(root.join(".gitignore"), b"user ignored files\r\n").unwrap();
            let source_map = fs::read(root.join("src/source.json")).unwrap();
            let report = success(
                &test.0,
                locale,
                &["init", "project", "--name", "attached", "--format", "json"],
            );
            assert_eq!(report["command"], "init");
            let data = &report["data"];
            assert_eq!(
                data.as_object()
                    .unwrap()
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                [
                    "accepted_base",
                    "accepted_base_created",
                    "accepted_base_sha256",
                    "ancestor_sha256",
                    "ancestry_preserved",
                    "manifest_sha256",
                    "metadata_name",
                    "name",
                    "root",
                    "source",
                    "source_format",
                    "source_layout",
                    "source_layout_version",
                    "type",
                    "version",
                    "version_scheme",
                ]
            );
            assert_eq!(data["type"], kind);
            assert_eq!(data["name"], "attached");
            assert_eq!(data["metadata_name"], "original");
            assert_eq!(data["version"], "0.1.0");
            assert_eq!(data["version_scheme"], "x.y.z");
            assert_eq!(data["source_format"], "eska");
            assert_eq!(data["source_layout"], "queries");
            assert_eq!(
                data["source_layout_version"],
                if matches!(kind, "processing" | "report") {
                    6
                } else {
                    5
                }
            );
            assert_eq!(data["accepted_base_created"], true);
            assert_eq!(data["ancestry_preserved"], false);
            assert_eq!(data["root"], root.to_str().unwrap());
            assert_eq!(data["source"], root.join("src").to_str().unwrap());
            let digest = Sha256::digest(fs::read(root.join("eska.toml")).unwrap())
                .iter()
                .fold(String::with_capacity(64), |mut value, byte| {
                    write!(value, "{byte:02x}").unwrap();
                    value
                });
            assert_eq!(data["manifest_sha256"], digest);
            assert_ne!(data["ancestor_sha256"], data["accepted_base_sha256"]);
            assert_eq!(source_map, fs::read(root.join("src/source.json")).unwrap());
            assert_eq!(fs::read(root.join("user.txt")).unwrap(), b"user text\r\n");
            assert_eq!(
                fs::read(root.join(".gitignore")).unwrap(),
                b"user ignored files\r\n"
            );
            assert!(!root.join(".git").exists());
            assert_eq!(
                discover(&root).unwrap().configuration().version_scheme(),
                Some(VersionScheme::ThreePart)
            );
            success(
                &root,
                locale,
                &[
                    "prepare",
                    "--accepted-base-sha256",
                    data["accepted_base_sha256"].as_str().unwrap(),
                    "--format",
                    "json",
                ],
            );
        }
    }
}

/// Existing acceptance/history retain exact bytes and require an independently provided support digest.
#[test]
fn retained_evidence_requires_pin_and_reports_unchanged_ancestry_and_human_status() {
    for (locale, message) in [("ru", "Исходники подключены"), ("en", "Sources attached")]
    {
        let test = TestDir::new();
        let created = unmanaged(&test, "report", true);
        let root = test.0.join("project");
        let initial = fs::read(root.join(".eska/source-import/state.json")).unwrap();
        let pin = created["accepted_base_sha256"].as_str().unwrap();
        let denied = tool(
            &test.0,
            locale,
            &["init", "project", "--name", "attached", "--format", "json"],
        );
        assert_eq!(denied.status.code(), Some(1));
        assert!(!denied.stderr.is_empty());
        assert_eq!(
            serde_json::from_slice::<Value>(&denied.stdout).unwrap()["error"]["code"],
            "accepted_base_pin_required"
        );
        assert!(!root.join("eska.toml").exists());
        let output = tool(
            &test.0,
            locale,
            &[
                "init",
                "project",
                "--name",
                "attached",
                "--accepted-base-sha256",
                pin,
            ],
        );
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stderr.is_empty());
        assert!(String::from_utf8_lossy(&output.stdout).contains(message));
        assert_eq!(
            initial,
            fs::read(root.join(".eska/source-import/state.json")).unwrap()
        );
    }
    let test = TestDir::new();
    let created = unmanaged(&test, "configuration", true);
    let report = success(
        &test.0,
        "en",
        &[
            "init",
            "project",
            "--name",
            "attached",
            "--accepted-base-sha256",
            created["accepted_base_sha256"].as_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert_eq!(report["data"]["accepted_base_created"], false);
    assert_eq!(report["data"]["ancestry_preserved"], true);
    assert_eq!(
        report["data"]["accepted_base_sha256"],
        created["accepted_base_sha256"]
    );
    assert_eq!(
        report["data"]["ancestor_sha256"],
        created["ancestor_sha256"]
    );
}

/// Invalid names, source paths and existing settings share stable JSON errors across locales.
#[test]
fn invalid_or_existing_attachment_refuses_without_altering_user_state() {
    let test = TestDir::new();
    unmanaged(&test, "report", false);
    for args in [
        vec!["init", "project", "--name", "Bad Name", "--format", "json"],
        vec![
            "init",
            "project",
            "--name",
            "attached",
            "--source",
            "../outside",
            "--format",
            "json",
        ],
    ] {
        let mut previous = None;
        for locale in ["ru", "en"] {
            let output = tool(&test.0, locale, &args);
            assert_eq!(output.status.code(), Some(1));
            assert!(!output.stderr.is_empty());
            let report: Value = serde_json::from_slice(&output.stdout).unwrap();
            if let Some(old) = previous {
                assert_eq!(old, report);
            }
            previous = Some(report);
            assert!(!test.0.join("project/eska.toml").exists());
            assert!(!test.0.join("project/.eska").exists());
        }
    }
    fs::write(test.0.join("project/eska.toml"), b"user settings\r\n").unwrap();
    for locale in ["ru", "en"] {
        let output = tool(
            &test.0,
            locale,
            &["init", "project", "--name", "attached", "--format", "json"],
        );
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap()["error"]["code"],
            "source_destination_exists"
        );
        assert_eq!(
            fs::read(test.0.join("project/eska.toml")).unwrap(),
            b"user settings\r\n"
        );
    }
}

/// Global project-dir resolves attachment paths and both locales expose each new help description.
#[test]
fn initialization_help_and_project_directory_are_explicit_in_both_locales() {
    for (locale, phrase) in [("ru", "Подключить существующие"), ("en", "Attach existing")]
    {
        let test = TestDir::new();
        let help = tool(&test.0, locale, &["init", "--help"]);
        assert_eq!(help.status.code(), Some(0));
        assert!(help.stderr.is_empty());
        let body = String::from_utf8_lossy(&help.stdout);
        assert!(body.contains(phrase));
        assert!(!body.contains("tool-option-"));
        unmanaged(&test, "processing", false);
        let report = success(
            &test.0,
            locale,
            &[
                "--project-dir",
                "project",
                "init",
                "--name",
                "attached",
                "--format",
                "json",
            ],
        );
        assert_eq!(
            report["data"]["root"],
            test.0.join("project").to_str().unwrap()
        );
    }
}
