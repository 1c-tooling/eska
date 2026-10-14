//! Invalid capabilities are rejected before the companion can execute a build.

use super::fixture::{self, Fixture};
use std::{fs, time::Instant};

#[test]
/// Protocol/package/profile claims must all match, independently of the selected human locale.
fn rejects_incompatible_probes_without_running_build() {
    let fixture = Fixture::new();
    fixture.sibling();
    for (mode, code) in [
        ("version", "build-tool-incompatible"),
        ("protocol", "build-tool-incompatible"),
        ("tool", "build-tool-incompatible"),
        ("artifact", "build-tool-incompatible"),
        ("malformed", "build-tool-incompatible"),
        ("failure", "build-tool-probe-failed"),
        ("oversized", "build-tool-probe-oversized"),
    ] {
        let mut results = Vec::new();
        for locale in ["ru", "en"] {
            let output = fixture
                .command(locale)
                .env("FAKE_ESKA_BUILD_MODE", mode)
                .args(["build", "--format", "json"])
                .output()
                .unwrap();
            results.push(fixture::failure(&output, code));
        }
        assert_eq!(results[0], results[1]);
        assert!(!fixture.0.0.join("arguments.bin").exists());
    }
}

#[test]
/// A hanging companion is reaped within the probe deadline and cannot launch native execution.
fn bounds_hanging_probe() {
    let fixture = Fixture::new();
    fixture.sibling();
    let start = Instant::now();
    let output = fixture
        .command("en")
        .env("FAKE_ESKA_BUILD_MODE", "hang")
        .args(["build", "--format", "json"])
        .output()
        .unwrap();
    fixture::failure(&output, "build-tool-probe-timeout");
    assert!(start.elapsed().as_secs() < 15);
    assert!(!fixture.0.0.join("arguments.bin").exists());
}

#[test]
/// Missing siblings fall back to PATH, but an incompatible sibling never selects another tool.
fn preserves_sibling_priority_and_path_fallback() {
    let fixture = Fixture::new();
    let path = fixture.0.0.join("path");
    fs::create_dir(&path).unwrap();
    fixture::write_tool(&path.join(format!("eska-build{}", std::env::consts::EXE_SUFFIX)));
    let mut paths = vec![path];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let search = std::env::join_paths(paths).unwrap();
    let output = fixture
        .command("en")
        .env("PATH", &search)
        .arg("build")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(23), "{output:?}");
    assert_eq!(
        fs::read_to_string(fixture.0.0.join("probe-executable.txt")).unwrap(),
        fixture
            .0
            .0
            .join(format!("path/eska-build{}", std::env::consts::EXE_SUFFIX))
            .to_str()
            .unwrap()
    );
    fs::remove_file(fixture.0.0.join("arguments.bin")).unwrap();
    fixture.sibling();
    let output = fixture
        .command("en")
        .env("PATH", &search)
        .env("FAKE_ESKA_BUILD_MODE", "version")
        .args(["build", "--format", "json"])
        .output()
        .unwrap();
    fixture::failure(&output, "build-tool-incompatible");
    assert_eq!(
        fs::read_to_string(fixture.0.0.join("probe-executable.txt")).unwrap(),
        fixture
            .0
            .0
            .join(format!("bin/eska-build{}", std::env::consts::EXE_SUFFIX))
            .to_str()
            .unwrap()
    );
    assert!(!fixture.0.0.join("arguments.bin").exists());
}

#[test]
/// Failure to locate a companion does not consult the platform or create source/build state.
fn missing_tool_is_stable_in_both_locales() {
    let fixture = Fixture::new();
    let mut results = Vec::new();
    for locale in ["ru", "en"] {
        let output = fixture
            .command(locale)
            .env("PATH", "")
            .args(["build", "--format", "json"])
            .output()
            .unwrap();
        results.push(fixture::failure(&output, "build-tool-missing"));
    }
    assert_eq!(results[0], results[1]);
    assert!(!fixture.0.0.join("build").exists());
}
