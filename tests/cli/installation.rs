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
