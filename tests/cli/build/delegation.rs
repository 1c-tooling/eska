//! Facade scenarios exercise real child processes without modifying the shared binary directory.

mod fixture;
mod handshakes;
mod selection;

#[cfg(feature = "build-cli")]
mod actual;

use fixture::Fixture;
use serde_json::Value;
use std::fs;

#[test]
/// Numeric status, separate raw streams and complete argument values cross the facade unchanged.
fn preserves_child_arguments_streams_status_and_canonical_locale() {
    let fixture = Fixture::new();
    fixture.sibling();
    let config = fixture.0.0.join("eska.toml");
    fs::write(
        &config,
        fs::read_to_string(&config)
            .unwrap()
            .replace("type='configuration'", "type='extension'"),
    )
    .unwrap();
    let base = fixture.0.0.join("база context.cf");
    fs::write(&base, b"context").unwrap();
    for (locale, canonical) in [("ru", "ru-RU"), ("en", "en-US")] {
        let output = fixture
            .command(locale)
            .args([
                "build",
                "--format",
                "json",
                "--accepted-base-sha256",
                &"a".repeat(64),
                "--output",
                "result каталог/demo.cfe",
                "--generations",
                "derived каталог",
                "--allow-exclusions",
                "--platform-version",
                "8.3.27.2325",
                "--platform-arch",
                "x86-64",
                "--distrobox",
                "test-container",
                "--recreate-infobase",
                "--manifest",
                "--ibcmd",
                "tool каталог/ibcmd",
                "--base-configuration",
            ])
            .arg(&base)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(23), "{output:?}");
        assert_eq!(output.stdout, b"{\"delegated\":true}\n");
        assert_eq!(output.stderr, b"native stderr\r\n");
        let arguments = fixture.arguments();
        fixture::argument(&arguments, "--lang", canonical);
        fixture::argument(&arguments, "--format", "json");
        fixture::argument(&arguments, "--accepted-base-sha256", &"a".repeat(64));
        fixture::argument(&arguments, "--output", "result каталог/demo.cfe");
        fixture::argument(&arguments, "--generations", "derived каталог");
        fixture::argument(&arguments, "--ibcmd", "tool каталог/ibcmd");
        fixture::argument(&arguments, "--base-configuration", base.to_str().unwrap());
        fixture::argument(&arguments, "--platform-version", "8.3.27.2325");
        fixture::argument(&arguments, "--platform-arch", "x86-64");
        fixture::argument(&arguments, "--distrobox", "test-container");
        assert!(arguments.iter().any(|argument| argument == "build"));
        assert!(
            arguments
                .iter()
                .any(|argument| argument == "--allow-exclusions")
        );
        assert!(
            arguments
                .iter()
                .any(|argument| argument == "--recreate-infobase")
        );
        assert!(!arguments.iter().any(|argument| argument == "--manifest"));
    }
}

#[test]
/// An unsupported ESKA preview never probes a tool or starts platform execution.
fn refuses_eska_preview_before_tool_discovery() {
    let fixture = Fixture::new();
    for locale in ["ru", "en"] {
        let output = fixture
            .command(locale)
            .args(["build", "--dry-run", "--format", "json"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["error"]["code"], "build-eska-dry-run-unsupported");
    }
    assert!(!fixture.0.0.join("arguments.bin").exists());
    assert!(!fixture.0.0.join("probe.bin").exists());
}

#[cfg(unix)]
#[test]
/// A Unix child killed by a signal becomes the corresponding conventional shell exit status.
fn preserves_signal_status() {
    let fixture = Fixture::new();
    fixture.sibling();
    let output = fixture
        .command("en")
        .env("FAKE_ESKA_BUILD_SIGNAL", "15")
        .arg("build")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(143), "{output:?}");
}
