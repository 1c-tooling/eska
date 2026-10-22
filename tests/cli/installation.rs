//! Installation probes identify each actual executable without project or runtime dependencies.

use crate::support::TestDir;
use serde_json::Value;
use std::{fs, path::Path, process::Command};

/// Installer metadata is exact, locale-independent and obtained from the installed executable itself.
#[test]
fn every_tool_exposes_a_complete_contract_without_other_components() {
    let directory = TestDir::new();
    for name in [
        "eska",
        "eska-project",
        "eska-check",
        "eska-build",
        "eska-vcs",
    ] {
        let binary = Path::new(env!("CARGO_BIN_EXE_eska"))
            .with_file_name(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        let mut previous = None;
        for locale in ["ru_RU.UTF-8", "en_US.UTF-8"] {
            let output = Command::new(&binary)
                .arg("__tool-contract")
                .env("LANG", locale)
                .env("LC_ALL", locale)
                .env("PATH", "")
                .current_dir(&directory.0)
                .output()
                .unwrap();
            assert!(output.status.success(), "{name}: {output:?}");
            assert!(output.stderr.is_empty(), "{name}: {output:?}");
            let value: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert!(
                eska_tool_runtime::contract::compatible(&value, name),
                "{value}"
            );
            assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));
            if let Some(previous) = &previous {
                assert_eq!(previous, &output.stdout);
            }
            previous = Some(output.stdout);
        }
    }
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
}

/// Direct entry points reject an incompatible managed receipt before any command-specific writes.
#[test]
fn managed_receipts_gate_direct_tools_and_shell_delegation() {
    let directory = TestDir::new();
    let root = directory.0.join("installation");
    fs::create_dir_all(root.join("bin")).unwrap();
    fs::create_dir(root.join("components")).unwrap();
    let cases: &[(&str, &[&str])] = &[
        (
            "eska",
            &["new", "would-write", "--type", "processing", "--no-vcs"],
        ),
        (
            "eska-project",
            &["new", "would-write", "--type", "processing"],
        ),
        ("eska-build", &["config", "init"]),
        ("eska-vcs", &["status", "--format", "json"]),
        ("eska-check", &["--report", "would-write.json"]),
    ];
    for (name, arguments) in cases {
        let source = Path::new(env!("CARGO_BIN_EXE_eska"))
            .with_file_name(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        let binary = root.join("bin").join(source.file_name().unwrap());
        if fs::hard_link(&source, &binary).is_err() {
            fs::copy(&source, &binary).unwrap();
        }
        let contract = Command::new(&source)
            .arg("__tool-contract")
            .output()
            .unwrap();
        assert!(contract.status.success());
        let mut contract: Value = serde_json::from_slice(&contract.stdout).unwrap();
        contract["bridge_schema"] = serde_json::json!(99);
        let receipt = serde_json::json!({"schema_version":1,"method":"component-archive","archive_sha256":"a".repeat(64),
            "package":{"schema_version":1,"tool":name,"version":env!("CARGO_PKG_VERSION"),"target":contract["target"],"contract":contract,
            "files":[{"path":format!("bin/{}",source.file_name().unwrap().to_str().unwrap()),"size":source.metadata().unwrap().len(),"sha256":"b".repeat(64),"executable":true},
            {"path":"licenses/LICENSE","size":0,"sha256":"c".repeat(64),"executable":false}]}});
        fs::write(
            root.join("components").join(format!("{name}.json")),
            serde_json::to_vec(&receipt).unwrap(),
        )
        .unwrap();
        for locale in ["ru", "en"] {
            let output = Command::new(&binary)
                .args(["--lang", locale])
                .args(*arguments)
                .env("ESKA_CONFIG_DIR", directory.0.join("machine-config"))
                .current_dir(&directory.0)
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(1), "{name}: {output:?}");
            let stderr = String::from_utf8(output.stderr).unwrap();
            assert!(
                stderr.contains(if locale == "ru" {
                    "несовместимым"
                } else {
                    "incompatible"
                }),
                "{name}: {stderr}"
            );
            assert!(!directory.0.join("would-write").exists());
            assert!(!directory.0.join("would-write.json").exists());
            assert!(!directory.0.join("machine-config").exists());
            let help = Command::new(&binary)
                .args(["--lang", locale, "--help"])
                .output()
                .unwrap();
            assert!(help.status.success(), "{name}: {help:?}");
        }
    }
    for locale in ["ru", "en"] {
        let output = Command::new(env!("CARGO_BIN_EXE_eska"))
            .args([
                "--lang",
                locale,
                "new",
                "would-write",
                "--type",
                "processing",
                "--no-vcs",
            ])
            .env(
                "ESKA_PROJECT_BINARY",
                root.join("bin")
                    .join(format!("eska-project{}", std::env::consts::EXE_SUFFIX)),
            )
            .current_dir(&directory.0)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("receipt"));
        assert!(!directory.0.join("would-write").exists());
    }
}
