//! Source options are format-scoped, and selection/path semantics precede native work.

use super::fixture::{Fixture, argument};
use serde_json::Value;
use std::fs;

/// Read exactly the existing facade error schema on parent preflight failure.
fn failure(output: &std::process::Output, code: &str) {
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["kind"], "import");
    assert_eq!(value["error"]["code"], code);
}

#[test]
/// XML explicitly refuses source-only flags before Git/platform checks; an env pin is unrelated.
fn xml_refuses_explicit_source_options_in_both_locales() {
    let fixture = Fixture::new();
    fs::write(
        fixture.0.0.join("eska.toml"),
        "[project]\ntype='configuration'\n[build]\nplatform_version='8.3.27.2325'\n",
    )
    .unwrap();
    let digest = "a".repeat(64);
    for locale in ["ru", "en"] {
        for options in [
            vec!["--accepted-base-sha256", &digest],
            vec!["--ancestor-sha256", &digest],
            vec!["--ancestor", "source store", "--ancestor-sha256", &digest],
            vec!["--expected-plan-sha256", &digest],
            vec!["--write", "--receipt", "recovery.json"],
        ] {
            let output = fixture
                .command(locale)
                .args(["import", "incoming.cf", "--format", "json"])
                .args(options)
                .output()
                .unwrap();
            failure(&output, "import-source-options-incompatible");
            assert!(!fixture.0.0.join("probe.bin").exists());
            assert!(!fixture.0.0.join(".eska").exists());
        }
    }
}

#[test]
/// A support environment pin does not alter the legacy XML route or its error document.
fn xml_ignores_the_unrelated_support_pin_environment() {
    let fixture = Fixture::new();
    fs::write(
        fixture.0.0.join("eska.toml"),
        "[project]\ntype='configuration'\n[build]\nplatform_version='8.3.27.2325'\n",
    )
    .unwrap();
    let run = |approved: bool| {
        let mut command = fixture.command("en");
        command.args([
            "import",
            "incoming.cf",
            "--ibcmd",
            "missing-native-tool",
            "--format",
            "json",
        ]);
        if approved {
            command.env("ESKA_ACCEPTED_BASE_SHA256", "a".repeat(64));
        }
        command.output().unwrap()
    };
    let plain = run(false);
    let approved = run(true);
    assert_eq!(plain.status.code(), approved.status.code());
    assert_eq!(plain.stdout, approved.stdout);
    assert_eq!(plain.stderr, approved.stderr);
    assert!(!fixture.0.0.join("probe.bin").exists());
}

#[test]
/// Default preview retains the environment pin, while an explicit valid pin takes precedence.
fn preview_and_pin_precedence_do_not_invent_local_ancestry() {
    let fixture = Fixture::new();
    fixture.companion();
    let output = fixture
        .command("en")
        .env("ESKA_ACCEPTED_BASE_SHA256", "a".repeat(64))
        .args([
            "import",
            "incoming.erf",
            "--ancestor-sha256",
            &"b".repeat(64),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(23), "{output:?}");
    assert!(
        !fixture
            .arguments()
            .iter()
            .any(|value| value == "--write" || value == "--accepted-base-sha256")
    );
    let output = fixture
        .command("en")
        .env("ESKA_ACCEPTED_BASE_SHA256", "invalid")
        .args([
            "import",
            "incoming.erf",
            "--accepted-base-sha256",
            &"A".repeat(64),
            "--ancestor-sha256",
            &"b".repeat(64),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(23), "{output:?}");
    argument(
        &fixture.arguments(),
        "--accepted-base-sha256",
        &"A".repeat(64),
    );
}

#[test]
/// The global base controls file/ancestor/receipt; the selected member retains root/name/inheritance.
fn workspace_selection_keeps_current_member_and_original_shell_base() {
    let fixture = Fixture::new();
    fixture.companion();
    fs::create_dir(fixture.0.0.join("workspace")).unwrap();
    let root = fixture.0.0.join("workspace");
    fs::create_dir(root.join("member")).unwrap();
    fs::create_dir(root.join("member/src")).unwrap();
    fs::write(
        root.join("eska.toml"),
        "[workspace]\nmembers=['member']\n[build]\nplatform_version='8.3.27.2325'\n",
    )
    .unwrap();
    fs::write(root.join("member/eska.toml"), "[project]\nname='target'\ntype='report'\nsource_format='eska'\n[eska]\naccepted_base='.eska/accepted'\n").unwrap();
    let output = fixture
        .command("en")
        .arg("--project-dir")
        .arg(&root)
        .args([
            "import",
            "incoming.erf",
            "--accepted-base-sha256",
            &"a".repeat(64),
            "--ancestor-sha256",
            &"b".repeat(64),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    failure(&output, "project-selection");
    assert!(!fixture.0.0.join("probe.bin").exists());
    let output = fixture
        .command("en")
        .arg("--project-dir")
        .arg(&root)
        .args([
            "import",
            "incoming.erf",
            "-p",
            "target",
            "--accepted-base-sha256",
            &"a".repeat(64),
            "--ancestor-sha256",
            &"b".repeat(64),
            "--ancestor",
            "ancestor",
            "--write",
            "--receipt",
            "recovery.json",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(23), "{output:?}");
    let values = fixture.arguments();
    argument(
        &values,
        "--project-dir",
        root.join("member").to_str().unwrap(),
    );
    argument(&values, "--project", "target");
    argument(
        &values,
        "--ancestor",
        root.join("ancestor").to_str().unwrap(),
    );
    argument(
        &values,
        "--receipt",
        root.join("recovery.json").to_str().unwrap(),
    );
    assert!(
        values
            .iter()
            .any(|value| value == root.join("incoming.erf").to_str().unwrap())
    );
    let output = fixture
        .command("en")
        .arg("--project-dir")
        .arg(root.join("member/src"))
        .args([
            "import",
            "incoming.erf",
            "--accepted-base-sha256",
            &"a".repeat(64),
            "--ancestor-sha256",
            &"b".repeat(64),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(23), "{output:?}");
    argument(&fixture.arguments(), "--project", "target");
}

#[test]
/// Clap catches write-mode contradictions without touching source or starting a probe.
fn write_receipt_and_preview_modes_require_an_explicit_consistent_request() {
    let fixture = Fixture::new();
    let pin = "b".repeat(64);
    for options in [
        vec!["--write"],
        vec!["--receipt", "recovery.json"],
        vec!["--write", "--receipt", "recovery.json", "--dry-run"],
        vec!["--write", "--receipt", "recovery.json", "--force"],
    ] {
        let output = fixture
            .command("en")
            .args([
                "import",
                "incoming.erf",
                "--ancestor-sha256",
                &pin,
                "--format",
                "json",
            ])
            .args(options)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{output:?}");
        assert!(!fixture.0.0.join("probe.bin").exists());
    }
}

#[test]
/// A child failure document and its raw stderr are not wrapped into a second facade result.
fn child_error_json_and_exact_status_are_inherited() {
    let fixture = Fixture::new();
    fixture.companion();
    let output = fixture
        .command("en")
        .env("IMPORT_FIXTURE_ERROR", "1")
        .args([
            "import",
            "incoming.erf",
            "--accepted-base-sha256",
            &"a".repeat(64),
            "--ancestor-sha256",
            &"b".repeat(64),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(17));
    assert_eq!(output.stderr, b"raw child error\r\n");
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["tool"], "eska-build");
    assert_eq!(value["status"], "error");
    assert_eq!(value["error"]["code"], "source_import_conflicts");
    assert_eq!(value["error"]["diagnostics"]["receipt"], "recovery.json");
    assert!(value.get("kind").is_none());
}

#[test]
/// Missing binaries and timed-out probes refuse before actual child invocation or native storage.
fn missing_or_timed_out_companion_keeps_source_and_starts_no_native_work() {
    let fixture = Fixture::new();
    let before = fs::read(fixture.0.0.join("src/user.bsl")).unwrap();
    let output = fixture
        .command("en")
        .env("PATH", "")
        .args([
            "import",
            "incoming.erf",
            "--accepted-base-sha256",
            &"a".repeat(64),
            "--ancestor-sha256",
            &"b".repeat(64),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    failure(&output, "import-tool-missing");
    fixture.companion();
    let output = fixture
        .command("en")
        .env("IMPORT_FIXTURE_MODE", "hang")
        .args([
            "import",
            "incoming.erf",
            "--accepted-base-sha256",
            &"a".repeat(64),
            "--ancestor-sha256",
            &"b".repeat(64),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    failure(&output, "import-tool-probe-timeout");
    assert!(!fixture.0.0.join("arguments.bin").exists());
    assert!(!fixture.0.0.join(".eska").exists());
    assert_eq!(fs::read(fixture.0.0.join("src/user.bsl")).unwrap(), before);
}
