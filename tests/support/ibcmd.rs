//! One portable native-process fixture shared by independent and installed CLIs.

use std::{path::PathBuf, process::Command, sync::OnceLock};

static EXECUTABLE: OnceLock<PathBuf> = OnceLock::new();

/// Build a process fixture once per integration executable, independently of the production runner.
pub fn executable() -> PathBuf {
    EXECUTABLE
        .get_or_init(|| {
            let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let manifest = repository.join("tests/fixtures/ibcmd/Cargo.toml");
            let target = repository.join("target/test-fixtures/ibcmd");
            let status = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
                .args(["build", "--quiet", "--locked", "--manifest-path"])
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
