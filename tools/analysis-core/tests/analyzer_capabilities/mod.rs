//! Project-free compatibility output is separate from initialized IDE readiness.

use serde_json::Value;
use std::{fs, process::Command};

use super::{Client, initialize, notification, support};

/// A real initialized project reports the same linked core identity as the standalone binary probe.
#[test]
fn initialized_session_reports_linked_source_core_version_in_both_locales() {
    let (_directory, root) = support::project("// Current module.\n", false);
    for locale in ["ru-RU", "en-US"] {
        let mut client = Client::with_options(&root, locale, &[]);
        let response = client.ask(&initialize());
        assert_eq!(
            response["result"]["capabilities"]["experimental"]["eska"]["sourceCoreVersion"],
            eska::tool_cli::PACKAGE_VERSION
        );
        assert_eq!(
            response["result"]["serverInfo"]["version"],
            env!("CARGO_PKG_VERSION")
        );
        client.send(&notification("initialized"));
        client.finish();
    }
}

/// No project, stdin frames, accepted pin or platform is needed to identify a compatible binary.
#[test]
fn analyzer_capabilities_are_project_free_and_locale_independent() {
    let directory = support::TestDirectory::new();
    let marker = directory.0.join("eska.toml");
    fs::write(&marker, b"invalid neighboring TOML").unwrap();
    let mut reports = vec![];
    for locale in ["ru-RU", "en-US"] {
        let output = Command::new(env!("CARGO_BIN_EXE_eska-analyzer"))
            .current_dir(&directory.0)
            .env("PATH", "")
            .env(
                "ESKA_ACCEPTED_BASE_SHA256",
                "invalid pin is irrelevant to a binary probe",
            )
            .args(["--lang", locale, "--capabilities", "--format", "json"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stderr.is_empty());
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["schema_version"], 1);
        assert_eq!(report["tool"], "eska-analyzer");
        assert_eq!(report["command"], "capabilities");
        assert_eq!(report["status"], "success");
        assert_eq!(report["data"]["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(
            report["data"]["source_core_version"],
            eska::tool_cli::PACKAGE_VERSION
        );
        assert_eq!(report["data"]["protocol_version"], "0.1");
        assert_eq!(report["data"]["commands"], serde_json::json!(["serve"]));
        assert_eq!(
            report["data"]["profiles"],
            serde_json::json!(["syntax", "semantic"])
        );
        assert!(report["data"].get("snapshot").is_none());
        assert!(report["data"].get("ready").is_none());
        reports.push(report);
        let output = Command::new(env!("CARGO_BIN_EXE_eska-analyzer"))
            .current_dir(&directory.0)
            .env("PATH", "")
            .args(["--lang", locale, "--capabilities"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        assert!(
            String::from_utf8(output.stdout)
                .unwrap()
                .contains(if locale == "ru-RU" {
                    "Протокол ESKA IDE"
                } else {
                    "ESKA IDE protocol"
                })
        );
    }
    assert_eq!(reports[0], reports[1]);
    assert_eq!(fs::read(marker).unwrap(), b"invalid neighboring TOML");
    assert!(!directory.0.join(".eska").exists());
}

/// Human/JSON compatibility mode cannot be mistaken for project selection or a running server.
#[test]
fn analyzer_capabilities_refuse_server_arguments_and_format_requires_capabilities() {
    let directory = support::TestDirectory::new();
    for (option, value) in [
        ("--project-dir", "."),
        ("--profile", "syntax"),
        ("--accepted-base-sha256", "bad"),
        ("--timeout-seconds", "1"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_eska-analyzer"))
            .current_dir(&directory.0)
            .args(["--capabilities", "--format", "json", option, value])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(3));
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["error"]["code"], "arguments_invalid");
        assert_eq!(report["error"]["diagnostics"]["parser"], "ArgumentConflict");
    }
    let output = Command::new(env!("CARGO_BIN_EXE_eska-analyzer"))
        .current_dir(&directory.0)
        .args(["--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        report["error"]["diagnostics"]["parser"],
        "MissingRequiredArgument"
    );
}
