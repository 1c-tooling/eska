//! Failed or intervening child publications retain roots and never enroll unverifiable candidates.

use super::fixture::Fixture;
use serde_json::Value;
use std::{fs, process::Command};

/// The wrapper invokes the real source-only project tool and records canonical locale arguments.
pub(super) fn wrapper(fixture: &Fixture, locale: &str, mode: &str) -> Command {
    let mut command = fixture.command(locale);
    command
        .env("NEW_FIXTURE_ROOT", &fixture.0.0)
        .env("NEW_FIXTURE_MODE", mode)
        .env(
            "NEW_FIXTURE_REAL_PROJECT",
            env!("CARGO_BIN_EXE_eska-project"),
        );
    command
}

#[test]
/// The original receipt fences settings, authored source, accepted bytes and initial control state.
fn refuses_intervening_child_changes_and_keeps_all_created_or_foreign_files() {
    for (mode, code) in [
        ("manifest", "new-manifest-changed"),
        ("source", "new-source-changed"),
        ("accepted", "new-source-changed"),
        ("state", "new-source-changed"),
        ("failed-ack", "new-child-failed"),
        ("foreign", "new-child-response-invalid"),
        ("git", "new-git-reservation-failed"),
    ] {
        let fixture = Fixture::new();
        fixture.guarded_tool();
        let output = wrapper(&fixture, "en", mode)
            .args([
                "new",
                "project",
                "--source-format",
                "eska",
                "--type",
                "configuration",
                "--workflow",
                "trunk",
                "--format",
                "json",
            ])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{mode}: {output:?}");
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["error"]["code"], code, "{mode}: {value}");
        assert_eq!(value["diagnostics"]["destination_removed"], false);
        assert_eq!(value["diagnostics"]["support_approved"], false);
        let root = fixture.0.0.join("project");
        assert!(root.is_dir());
        if mode == "foreign" {
            assert_eq!(
                fs::read(root.join("user.txt")).unwrap(),
                b"foreign publication"
            );
            assert_eq!(value["diagnostics"]["project_created"], Value::Null);
            assert!(!root.join("eska.toml").exists());
        } else {
            assert!(root.join("eska.toml").is_file());
            assert!(root.join("src/configuration.yaml").is_file());
            if mode != "failed-ack" {
                assert!(root.join("user.txt").is_file());
            }
        }
        if mode == "git" {
            assert_eq!(
                fs::read(root.join(".git/foreign.txt")).unwrap(),
                b"foreign Git metadata"
            );
        }
        if mode == "manifest" {
            assert!(
                fs::read_to_string(root.join("eska.toml"))
                    .unwrap()
                    .ends_with("# intervening user edit\n")
            );
        }
        if mode == "failed-ack" {
            assert_eq!(value["diagnostics"]["child_exit_code"], 23);
        }
    }
}

#[test]
/// A changed workspace remains byte-for-byte intact and the verified child stays unlisted.
fn refuses_raced_enrollment_without_removing_published_member() {
    let fixture = Fixture::new();
    fixture.guarded_tool();
    let original = "# preserve root\n[workspace]\nmembers=[]\n";
    fs::write(fixture.0.0.join("eska.toml"), original).unwrap();
    let output = wrapper(&fixture, "ru", "workspace")
        .args([
            "new",
            "member",
            "--source-format",
            "eska",
            "--type",
            "configuration",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["error"]["code"], "new-policy-changed");
    assert_eq!(value["diagnostics"]["source_verified"], true);
    assert!(fixture.0.0.join("src/member/eska.toml").is_file());
    assert_eq!(
        fs::read_to_string(fixture.0.0.join("eska.toml")).unwrap(),
        format!("{original}\n# intervening workspace edit\n")
    );
}

#[test]
/// Probes are bounded and run before parent/source/staging mutations in both locales.
fn incompatible_or_bounded_probes_create_no_member_parent_or_native_stage() {
    for (mode, code) in [
        ("probe-version", "new-companion-incompatible"),
        ("probe-hang", "new-companion-probe-timeout"),
        ("probe-oversized", "new-companion-probe-oversized"),
    ] {
        let fixture = Fixture::new();
        fixture.guarded_tool();
        fs::write(fixture.0.0.join("eska.toml"), "[workspace]\nmembers=[]\n").unwrap();
        let output = wrapper(&fixture, "ru", mode)
            .args([
                "new",
                "member",
                "--source-format",
                "eska",
                "--type",
                "configuration",
                "--format",
                "json",
            ])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["error"]["code"], code);
        assert!(!fixture.0.0.join("src").exists());
        assert!(!fixture.0.0.join(".eska").exists());
        let arguments = fs::read_to_string(fixture.0.0.join("probe.bin")).unwrap();
        assert!(arguments.contains("ru-RU"));
    }
}

#[test]
/// A missing sibling falls back to PATH, while a present incompatible sibling is authoritative.
fn resolves_path_only_after_missing_sibling_and_never_retries_a_failed_sibling() {
    let fixture = Fixture::new();
    let path = fixture.0.0.join("path-tools");
    fs::create_dir(&path).unwrap();
    let executable = path.join(format!("eska-project{}", std::env::consts::EXE_SUFFIX));
    fs::hard_link(env!("CARGO_BIN_EXE_eska-project"), &executable)
        .or_else(|_| fs::copy(env!("CARGO_BIN_EXE_eska-project"), &executable).map(|_| ()))
        .unwrap();
    let output = fixture
        .command("en")
        .env("PATH", &path)
        .args([
            "new",
            "project",
            "--source-format",
            "eska",
            "--type",
            "report",
            "--workflow",
            "trunk",
            "--no-vcs",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    super::fixture::success(&output);
    fixture.guarded_tool();
    let output = wrapper(&fixture, "en", "probe-version")
        .env("PATH", &path)
        .args([
            "new",
            "other",
            "--source-format",
            "eska",
            "--type",
            "report",
            "--workflow",
            "trunk",
            "--no-vcs",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["error"]["code"], "new-companion-incompatible");
    assert!(!fixture.0.0.join("other").exists());
    let missing = Fixture::new();
    let output = missing
        .command("ru")
        .env("PATH", "")
        .args([
            "new",
            "project",
            "--source-format",
            "eska",
            "--type",
            "report",
            "--workflow",
            "trunk",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["error"]["code"], "new-companion-missing");
    assert!(!missing.0.0.join("project").exists());
}
