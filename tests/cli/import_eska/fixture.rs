//! Transport fixtures keep every executable and source sentinel inside one owned playground root.

use crate::support::TestDir;
use std::{fs, path::Path, process::Command};

pub(super) struct Fixture(pub(super) TestDir);

impl Fixture {
    /// A private facade executable controls sibling lookup without changing the shared build bin.
    pub(super) fn new() -> Self {
        let root = TestDir::new();
        fs::create_dir(root.0.join("bin")).unwrap();
        fs::create_dir(root.0.join("src")).unwrap();
        let binary = root
            .0
            .join(format!("bin/eska{}", std::env::consts::EXE_SUFFIX));
        fs::hard_link(env!("CARGO_BIN_EXE_eska"), &binary)
            .or_else(|_| fs::copy(env!("CARGO_BIN_EXE_eska"), &binary).map(|_| ()))
            .unwrap();
        fs::write(root.0.join("eska.toml"), "[project]\nname='transport'\ntype='report'\nsource_format='eska'\n[eska]\naccepted_base='.eska/accepted'\n[build]\nplatform_version='8.3.27.2325'\n").unwrap();
        fs::write(
            root.0.join("src/user.bsl"),
            "\u{feff}// preserved user source\r\n",
        )
        .unwrap();
        Self(root)
    }

    /// Runtime flags and environment are isolated from the installed platform and user approval.
    pub(super) fn command(&self, locale: &str) -> Command {
        let mut command = Command::new(
            self.0
                .0
                .join(format!("bin/eska{}", std::env::consts::EXE_SUFFIX)),
        );
        command
            .current_dir(&self.0.0)
            .env_remove("ESKA_LANG")
            .env_remove("ESKA_ACCEPTED_BASE_SHA256")
            .env_remove("ESKA_IBCMD")
            .env_remove("ESKA_PLATFORM_ARCH")
            .env_remove("ESKA_DISTROBOX")
            .env("ESKA_CONFIG_DIR", self.0.0.join("machine"))
            .env("IMPORT_FIXTURE_ROOT", &self.0.0)
            .env("IMPORT_FIXTURE_VERSION", env!("CARGO_PKG_VERSION"))
            .args(["--lang", locale]);
        command
    }

    /// Rustc emits only an exclusively owned fixture path; capabilities never spawn native work.
    pub(super) fn companion(&self) {
        let source =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cli/import_eska/companion.rs");
        let status = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
            .args(["--edition=2024", "--crate-name=eska_test_import_companion"])
            .arg(source)
            .arg("-o")
            .arg(
                self.0
                    .0
                    .join(format!("bin/eska-build{}", std::env::consts::EXE_SUFFIX)),
            )
            .status()
            .unwrap();
        assert!(status.success());
    }

    /// NUL-separated argv values prove boundaries without inventing a shell quoting convention.
    pub(super) fn arguments(&self) -> Vec<String> {
        fs::read(self.0.0.join("arguments.bin"))
            .unwrap()
            .split(|byte| *byte == 0)
            .map(|argument| String::from_utf8(argument.to_owned()).unwrap())
            .collect()
    }
}

/// Every path flag remains a separate native argument with an exact value.
pub(super) fn argument(values: &[String], name: &str, expected: &str) {
    let index = values.iter().position(|value| value == name).unwrap();
    assert_eq!(values[index + 1], expected, "{name}");
}
