//! Private companion paths avoid changing the shared target directory or installed user tools.

use crate::support::TestDir;
use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

pub(in crate::cli) struct Fixture(pub(in crate::cli) TestDir);

impl Fixture {
    /// Every executable and output path belongs to one uniquely claimed playground directory.
    pub(in crate::cli) fn new() -> Self {
        let test = TestDir::new();
        fs::create_dir(test.0.join("bin")).unwrap();
        install(
            env!("CARGO_BIN_EXE_eska"),
            &test
                .0
                .join(format!("bin/eska{}", std::env::consts::EXE_SUFFIX)),
        );
        Self(test)
    }

    /// Empty creation uses the actual independently compiled source-only tool.
    pub(in crate::cli) fn project_tool(&self) {
        install(
            env!("CARGO_BIN_EXE_eska-project"),
            &self
                .0
                .0
                .join(format!("bin/eska-project{}", std::env::consts::EXE_SUFFIX)),
        );
    }

    /// Native new-from adds only the actual separately compiled build companion.
    #[cfg(feature = "build-cli")]
    pub(super) fn build_tool(&self) {
        install(
            env!("CARGO_BIN_EXE_eska-build"),
            &self
                .0
                .0
                .join(format!("bin/eska-build{}", std::env::consts::EXE_SUFFIX)),
        );
    }

    /// Machine settings and trust environment never leak into this compound creation invocation.
    pub(in crate::cli) fn command(&self, locale: &str) -> Command {
        let mut command = Command::new(
            self.0
                .0
                .join(format!("bin/eska{}", std::env::consts::EXE_SUFFIX)),
        );
        command
            .current_dir(&self.0.0)
            .env_remove("ESKA_LANG")
            .env_remove("ESKA_IBCMD")
            .env_remove("ESKA_DISTROBOX")
            .env_remove("ESKA_PLATFORM_ARCH")
            .env_remove("ESKA_ACCEPTED_BASE_SHA256")
            .env("ESKA_CONFIG_DIR", self.0.0.join("machine"))
            .args(["--lang", locale]);
        command
    }

    /// The dependency-free wrapper mutates child results only within the same owned test directory.
    pub(super) fn guarded_tool(&self) {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cli/new_eska/companion.rs");
        self.guarded_tool_from(&source);
    }

    /// Compound init/new guards share isolation while keeping their own mutation protocol.
    pub(in crate::cli) fn guarded_tool_from(&self, source: &Path) {
        let status = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
            .args([
                "--edition=2024",
                "--crate-name=eska_test_onboarding_companion",
            ])
            .arg(source)
            .arg("-o")
            .arg(
                self.0
                    .0
                    .join(format!("bin/eska-project{}", std::env::consts::EXE_SUFFIX)),
            )
            .status()
            .unwrap();
        assert!(status.success());
    }
}

/// Hard links share binary bytes while each invocation retains a private sibling directory.
fn install(source: &str, destination: &Path) {
    fs::hard_link(source, destination)
        .or_else(|_| fs::copy(source, destination).map(|_| ()))
        .unwrap();
}

/// The compound shell emits one complete machine document with no intermediate child successes.
pub(super) fn success(output: &Output) -> Value {
    assert!(output.status.success(), "{output:?}");
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["tool"], "eska");
    assert_eq!(value["command"], "new");
    assert_eq!(value["status"], "success");
    value["data"].clone()
}
