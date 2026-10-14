//! Every facade and fake companion lives in the test's exclusively owned playground directory.

use crate::support::TestDir;
use serde_json::Value;
use std::{fs, path::Path, process::Command};

pub(super) struct Fixture(pub(super) TestDir);

impl Fixture {
    /// A private hardlink avoids copying the large development binary into every process fixture.
    pub(super) fn new() -> Self {
        let root = TestDir::new();
        fs::create_dir(root.0.join("bin")).unwrap();
        fs::create_dir(root.0.join("src")).unwrap();
        fs::hard_link(
            env!("CARGO_BIN_EXE_eska"),
            root.0
                .join(format!("bin/eska{}", std::env::consts::EXE_SUFFIX)),
        )
        .or_else(|_| {
            fs::copy(
                env!("CARGO_BIN_EXE_eska"),
                root.0
                    .join(format!("bin/eska{}", std::env::consts::EXE_SUFFIX)),
            )
            .map(|_| ())
        })
        .unwrap();
        fs::write(
            root.0.join("eska.toml"),
            concat!(
                "[project]\nname='facade-demo'\ntype='configuration'\nsource_format='eska'\n",
                "[eska]\naccepted_base='.eska/accepted'\n",
                "[build]\nplatform_version='8.3.27.2325'\n"
            ),
        )
        .unwrap();
        Self(root)
    }

    /// Keep ordinary environment/settings independent from the installed platform and user locale.
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
            .env("ESKA_CONFIG_DIR", self.0.0.join("machine"))
            .env("FAKE_ESKA_BUILD_ROOT", &self.0.0)
            .env("FAKE_ESKA_BUILD_VERSION", env!("CARGO_PKG_VERSION"))
            .args(["--lang", locale]);
        command
    }

    /// Advertise a valid protocol by default; individual commands select isolated failure modes.
    pub(super) fn sibling(&self) {
        write_tool(
            &self
                .0
                .0
                .join(format!("bin/eska-build{}", std::env::consts::EXE_SUFFIX)),
        );
    }

    /// Native arguments cannot contain NUL; the fixture retains each UTF-8 value without escaping.
    pub(super) fn arguments(&self) -> Vec<String> {
        let bytes = fs::read(self.0.0.join("arguments.bin")).unwrap();
        bytes
            .split(|byte| *byte == 0)
            .map(|value| String::from_utf8(value.to_vec()).unwrap())
            .collect()
    }
}

/// Compile the dependency-free companion inside the owned playground directory, never the shared bin.
pub(super) fn write_tool(path: &Path) {
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cli/build/delegation/companion.rs");
    let status = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
        .args(["--edition=2024", "--crate-name=eska_test_build_companion"])
        .arg(source)
        .arg("-o")
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success(), "compile companion fixture");
}

/// Assert an option has one exact adjacent value in the child process's native argument vector.
pub(super) fn argument(arguments: &[String], option: &str, expected: &str) {
    let values: Vec<_> = arguments
        .windows(2)
        .filter(|pair| pair[0] == option)
        .map(|pair| pair[1].as_str())
        .collect();
    assert_eq!(values, [expected], "{arguments:?}");
}

/// Handshake stdout never leaks into the operation's single machine error result.
pub(super) fn failure(output: &std::process::Output, code: &str) -> Value {
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["error"]["code"], code);
    value
}
