//! Single ESKA import exposes the child protocol unchanged while XML retains its existing result.

#[cfg(feature = "build-cli")]
mod actual;
mod admission;
mod fixture;

use fixture::{Fixture, argument};
use serde_json::Value;
use std::fs;

#[test]
/// Independent authorities, exact plan and write receipt cross the boundary without local execution.
fn inherited_import_keeps_child_streams_status_locale_and_shell_scoped_paths() {
    let fixture = Fixture::new();
    fixture.companion();
    let original = fs::read(fixture.0.0.join("src/user.bsl")).unwrap();
    for (locale, canonical) in [("ru", "ru-RU"), ("en", "en-US")] {
        let output = fixture
            .command(locale)
            .args([
                "import",
                "artifact alias.erf",
                "--accepted-base-sha256",
                &"a".repeat(64),
                "--ancestor-sha256",
                &"b".repeat(64),
                "--ancestor",
                "ancestor store",
                "--expected-plan-sha256",
                &"c".repeat(64),
                "--write",
                "--receipt",
                "recovery receipt.json",
                "--format",
                "json",
                "--ibcmd",
                "platform path/ibcmd",
                "--platform-arch",
                "x86-64",
                "--distrobox",
                "native-container",
            ])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(23), "{output:?}");
        assert_eq!(output.stdout, b"{\"delegated\":true}\n");
        assert_eq!(output.stderr, b"native stderr\r\n");
        let values = fixture.arguments();
        argument(&values, "--lang", canonical);
        argument(&values, "--accepted-base-sha256", &"a".repeat(64));
        argument(&values, "--ancestor-sha256", &"b".repeat(64));
        argument(&values, "--expected-plan-sha256", &"c".repeat(64));
        argument(
            &values,
            "--ancestor",
            fixture.0.0.join("ancestor store").to_str().unwrap(),
        );
        argument(
            &values,
            "--receipt",
            fixture.0.0.join("recovery receipt.json").to_str().unwrap(),
        );
        argument(&values, "--ibcmd", "platform path/ibcmd");
        argument(&values, "--platform-arch", "x86-64");
        argument(&values, "--distrobox", "native-container");
        assert!(
            values
                .iter()
                .any(|value| value == fixture.0.0.join("artifact alias.erf").to_str().unwrap())
        );
        assert!(values.iter().any(|value| value == "--write"));
        assert_eq!(
            fs::read(fixture.0.0.join("src/user.bsl")).unwrap(),
            original
        );
    }
}

#[test]
/// Legacy force never selects ESKA conflict bytes; redundant dry-run remains a read-only preview.
fn force_and_missing_independent_ancestry_refuse_before_child_execution() {
    let fixture = Fixture::new();
    fixture.companion();
    for force in [true, false] {
        let mut command = fixture.command("en");
        command.args([
            "import",
            "incoming.erf",
            "--accepted-base-sha256",
            &"a".repeat(64),
            "--format",
            "json",
        ]);
        if force {
            command.args(["--force", "--ancestor-sha256", &"b".repeat(64)]);
        }
        let output = command.output().unwrap();
        assert!(!output.status.success());
        assert!(!fixture.0.0.join("arguments.bin").exists());
    }
    let output = fixture
        .command("en")
        .args([
            "import",
            "incoming.erf",
            "--accepted-base-sha256",
            &"a".repeat(64),
            "--ancestor-sha256",
            &"b".repeat(64),
            "--dry-run",
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
            .any(|value| matches!(value.as_str(), "--write" | "--force" | "--dry-run"))
    );
}

#[test]
/// Mode, ancestry and recovery capabilities are required alongside exact package/protocol agreement.
fn incompatible_source_import_capabilities_keep_sources_and_prevent_actual_invocation() {
    for mode in [
        "version",
        "protocol",
        "command",
        "type",
        "format",
        "mode",
        "ancestor",
        "recovery",
        "malformed",
        "failure",
        "oversized",
    ] {
        let fixture = Fixture::new();
        fixture.companion();
        let before = fs::read(fixture.0.0.join("src/user.bsl")).unwrap();
        let output = fixture
            .command("ru")
            .env("IMPORT_FIXTURE_MODE", mode)
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
        assert!(!output.status.success(), "{mode}: {output:?}");
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(value["error"]["code"].as_str().is_some());
        assert!(!fixture.0.0.join("arguments.bin").exists());
        assert_eq!(fs::read(fixture.0.0.join("src/user.bsl")).unwrap(), before);
    }
}

#[cfg(unix)]
#[test]
/// The signal fixture kills only itself and the shell waits before returning its conventional status.
fn child_signal_status_is_retained_without_background_fixture_processes() {
    let fixture = Fixture::new();
    fixture.companion();
    let output = fixture
        .command("en")
        .env("IMPORT_FIXTURE_SIGNAL", "1")
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
    assert_eq!(output.status.code(), Some(143), "{output:?}");
}
