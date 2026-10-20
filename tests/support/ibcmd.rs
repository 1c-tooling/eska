//! Compile the portable native process fixture once per integration-test executable.

use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

static FIXTURE: OnceLock<PathBuf> = OnceLock::new();

/// All consumers use the same immutable executable and a separate Cargo target directory.
pub fn executable(repository: &Path) -> PathBuf {
    FIXTURE
        .get_or_init(|| {
            let manifest = repository.join("tests/fixtures/ibcmd/Cargo.toml");
            let target = repository.join("target/test-fixtures/ibcmd");
            let status = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
                .args([
                    "build",
                    "--quiet",
                    "--locked",
                    "--offline",
                    "--manifest-path",
                ])
                .arg(&manifest)
                .arg("--target-dir")
                .arg(&target)
                .status()
                .expect("build portable ibcmd fixture");
            assert!(status.success(), "portable ibcmd fixture build failed");
            let executable = target
                .join("debug")
                .join(format!("eska-test-ibcmd{}", std::env::consts::EXE_SUFFIX));
            assert!(executable.is_file(), "portable ibcmd fixture is missing");
            executable
        })
        .clone()
}
