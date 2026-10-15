//! Compatibility queries are standalone, locale-independent and do not admit a project.

use serde_json::Value;
use std::{fs, process::Command};

use super::support;

/// Invalid neighboring controls cannot trigger discovery while querying compatibility.
#[test]
fn capabilities_need_no_project_or_profile_and_preserve_inputs_in_both_locales() {
    let directory = support::TestDirectory::new();
    let marker = directory.0.join("eska.toml");
    fs::write(&marker, b"this is deliberately invalid TOML").unwrap();
    let mut reports = vec![];
    for locale in ["ru-RU", "en-US"] {
        let output = Command::new(env!("CARGO_BIN_EXE_eska-check"))
            .current_dir(&directory.0)
            .env("PATH", "")
            .args(["--lang", locale, "--capabilities", "--format", "json"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stderr.is_empty());
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["schema_version"], 1);
        assert_eq!(report["tool"], "eska-check");
        assert_eq!(report["command"], "capabilities");
        assert_eq!(report["status"], "success");
        assert_eq!(report["data"]["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(
            report["data"]["source_core_version"],
            eska::tool_cli::PACKAGE_VERSION
        );
        assert_eq!(report["data"]["protocol_version"], 1);
        assert_eq!(
            report["data"]["profiles"],
            serde_json::json!(["syntax", "semantic"])
        );
        assert_eq!(
            report["data"]["source_formats"],
            serde_json::json!(["eska"])
        );
        assert_eq!(report["data"]["report_schema_version"], 1);
        reports.push(report);
        let human = Command::new(env!("CARGO_BIN_EXE_eska-check"))
            .current_dir(&directory.0)
            .env("PATH", "")
            .args(["--lang", locale, "--capabilities"])
            .output()
            .unwrap();
        assert_eq!(human.status.code(), Some(0));
        assert!(human.stderr.is_empty());
        assert!(
            String::from_utf8(human.stdout)
                .unwrap()
                .contains(if locale == "ru-RU" {
                    "Доступные профили"
                } else {
                    "Available check profiles"
                })
        );
    }
    assert_eq!(reports[0], reports[1]);
    assert_eq!(
        fs::read(&marker).unwrap(),
        b"this is deliberately invalid TOML"
    );
    assert!(!directory.0.join(".eska").exists());
}

/// Compatibility mode cannot silently swallow a requested analysis or its resource limits.
#[test]
fn capabilities_refuse_analysis_arguments_with_stable_exit_three() {
    let directory = support::TestDirectory::new();
    for option in [
        "--profile",
        "--project-dir",
        "--config",
        "--timeout-seconds",
    ] {
        let value = match option {
            "--profile" => "syntax",
            "--timeout-seconds" => "1",
            _ => ".",
        };
        let output = Command::new(env!("CARGO_BIN_EXE_eska-check"))
            .current_dir(&directory.0)
            .args(["--capabilities", "--format", "json", option, value])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(3));
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["error"]["code"], "arguments_invalid");
        assert_eq!(report["error"]["diagnostics"]["parser"], "ArgumentConflict");
    }
}
