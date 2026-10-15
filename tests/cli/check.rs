//! The shell/checker boundary preserves real process results and refuses incompatible siblings.

use serde_json::Value;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

use crate::support::TestDir;

struct Fixture(TestDir);

impl Fixture {
    /// Own both the shell link and fake companion without modifying the shared cargo target directory.
    fn new() -> Self {
        let directory = TestDir::new();
        fs::create_dir(directory.0.join("bin")).unwrap();
        fs::hard_link(env!("CARGO_BIN_EXE_eska"), directory.0.join(binary("eska")))
            .or_else(|_| {
                fs::copy(env!("CARGO_BIN_EXE_eska"), directory.0.join(binary("eska"))).map(|_| ())
            })
            .unwrap();
        Self(directory)
    }

    /// A no-PATH child proves sibling discovery; each test can opt into one owned fallback directory.
    fn command(&self, locale: &str) -> Command {
        let mut command = Command::new(self.0.0.join(binary("eska")));
        command
            .current_dir(&self.0.0)
            .env("PATH", "")
            .env_remove("ESKA_LANG")
            .env("ESKA_TEST_CHECK_ROOT", &self.0.0)
            .env("ESKA_TEST_CHECK_VERSION", env!("CARGO_PKG_VERSION"))
            .args(["--lang", locale]);
        command
    }

    /// Compile a dependency-free child in the owned directory, keeping real compatibility probes.
    fn sibling(&self) {
        write_tool(&self.0.0.join(binary("eska-check")));
    }

    /// Native argument strings contain no NUL; the test records exact adjacent option values.
    fn arguments(&self) -> Vec<String> {
        fs::read(self.0.0.join("arguments.bin"))
            .unwrap()
            .split(|byte| *byte == 0)
            .map(|value| String::from_utf8(value.to_vec()).unwrap())
            .collect()
    }
}

/// Executable suffix follows the current OS and no test assumes a fixed user path.
fn binary(name: &str) -> PathBuf {
    PathBuf::from(format!("bin/{name}{}", std::env::consts::EXE_SUFFIX))
}

/// A direct rustc invocation needs no project, shell script or platform runtime.
fn write_tool(path: &Path) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cli/check/companion.rs");
    let status = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
        .args(["--edition=2024", "--crate-name=eska_test_check_companion"])
        .arg(source)
        .arg("-o")
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
}

/// A probe error is one machine result, with localized stderr and the checker's fatal exit convention.
fn failure(output: &Output, expected: &str) -> Value {
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    assert!(!output.stderr.is_empty());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["tool"], "eska");
    assert_eq!(report["command"], "check");
    assert_eq!(report["error"]["code"], expected);
    report
}

/// Profile, locale, quotas, paths, raw streams, stdin and numeric exit cross the facade unchanged.
#[test]
fn check_preserves_child_arguments_streams_stdin_and_exit() {
    let fixture = Fixture::new();
    fixture.sibling();
    for (locale, canonical, profile, format) in [
        ("ru", "ru-RU", "syntax", "json"),
        ("en", "en-US", "semantic", "human"),
    ] {
        let mut child = fixture
            .command(locale)
            .args([
                "--project-dir",
                "member каталог",
                "check",
                "--profile",
                profile,
                "--format",
                format,
                "--project",
                "Member Name",
                "--config",
                "settings каталог/bsl-analyzer.toml",
                "--timeout-seconds",
                "17",
                "--max-document-bytes",
                "65536",
                "--max-retained-source-bytes",
                "1048576",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"stdin\0\xff\r\n")
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert_eq!(output.status.code(), Some(23), "{output:?}");
        assert_eq!(output.stdout, b"{\"checker\":true}\r\n");
        assert_eq!(output.stderr, b"checker stderr\r\n");
        assert_eq!(
            fs::read(fixture.0.0.join("stdin.bin")).unwrap(),
            b"stdin\0\xff\r\n"
        );
        let arguments = fixture.arguments();
        for (option, expected) in [
            ("--lang", canonical),
            ("--profile", profile),
            ("--format", format),
            ("--project-dir", "member каталог"),
            ("--project", "Member Name"),
            ("--config", "settings каталог/bsl-analyzer.toml"),
            ("--timeout-seconds", "17"),
            ("--max-document-bytes", "65536"),
            ("--max-retained-source-bytes", "1048576"),
        ] {
            let values: Vec<_> = arguments
                .windows(2)
                .filter(|pair| pair[0] == option)
                .map(|pair| pair[1].as_str())
                .collect();
            assert_eq!(values, [expected], "{arguments:?}");
        }
        assert!(
            !arguments
                .iter()
                .any(|argument| argument == "check" || argument == "--capabilities")
        );
        let probe = fs::read_to_string(fixture.0.0.join("probe.bin")).unwrap();
        assert_eq!(
            probe,
            ["--lang", canonical, "--capabilities", "--format", "json"].join("\0")
        );
    }
}

/// Admission failure never executes analysis and is machine-identical across locales.
#[test]
fn check_refuses_missing_and_incompatible_checker_before_execution() {
    let fixture = Fixture::new();
    let mut missing = vec![];
    for locale in ["ru", "en"] {
        missing.push(failure(
            &fixture
                .command(locale)
                .args(["check", "--profile", "syntax", "--format", "json"])
                .output()
                .unwrap(),
            "check_tool_missing",
        ));
    }
    assert_eq!(missing[0], missing[1]);
    fixture.sibling();
    for (mode, code) in [
        ("version", "check_tool_incompatible"),
        ("profile", "check_tool_incompatible"),
        ("malformed", "check_tool_incompatible"),
        ("failure", "check_tool_probe_failed"),
    ] {
        for locale in ["ru", "en"] {
            failure(
                &fixture
                    .command(locale)
                    .env("ESKA_TEST_CHECK_MODE", mode)
                    .args(["check", "--profile", "syntax", "--format", "json"])
                    .output()
                    .unwrap(),
                code,
            );
        }
        assert!(!fixture.0.0.join("arguments.bin").exists());
    }
}

/// A PATH candidate works only when no sibling exists; an incompatible sibling cannot be replaced.
#[test]
fn check_uses_path_fallback_and_keeps_sibling_precedence() {
    let fixture = Fixture::new();
    let fallback = fixture.0.0.join("fallback каталог");
    fs::create_dir(&fallback).unwrap();
    let target = fallback.join(format!("eska-check{}", std::env::consts::EXE_SUFFIX));
    write_tool(&target);
    let output = fixture
        .command("en")
        .env("PATH", &fallback)
        .args(["check", "--profile", "syntax"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(23));
    fs::remove_file(fixture.0.0.join("arguments.bin")).unwrap();
    fixture.sibling();
    let output = fixture
        .command("en")
        .env("PATH", &fallback)
        .env("ESKA_TEST_CHECK_MODE", "version")
        .args(["check", "--profile", "syntax", "--format", "json"])
        .output()
        .unwrap();
    failure(&output, "check_tool_incompatible");
    assert!(!fixture.0.0.join("arguments.bin").exists());
    assert_eq!(
        PathBuf::from(fs::read_to_string(fixture.0.0.join("probe-executable.txt")).unwrap()),
        fixture.0.0.join(binary("eska-check"))
    );
}

/// Probes must finish or be terminated before the real checker receives an operation.
#[test]
fn check_bounds_probe_time_and_stdout() {
    let fixture = Fixture::new();
    fixture.sibling();
    for (mode, code) in [
        ("hang", "check_tool_probe_timeout"),
        ("oversized", "check_tool_probe_oversized"),
    ] {
        let started = Instant::now();
        failure(
            &fixture
                .command("en")
                .env("ESKA_TEST_CHECK_MODE", mode)
                .args(["check", "--profile", "syntax", "--format", "json"])
                .output()
                .unwrap(),
            code,
        );
        assert!(started.elapsed() < Duration::from_secs(12));
        assert!(!fixture.0.0.join("arguments.bin").exists());
    }
}

/// Localized help is available without a companion, while analysis always requires a profile.
#[test]
fn check_help_is_localized_and_profile_is_required() {
    let fixture = Fixture::new();
    for (locale, expected) in [
        ("ru", "самостоятельным checker"),
        ("en", "standalone checker"),
    ] {
        let output = fixture
            .command(locale)
            .args(["check", "--help"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.contains(expected), "{help}");
        assert!(help.contains("--profile") && help.contains("--config"));
        let output = fixture.command(locale).arg("check").output().unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(!fixture.0.0.join("probe.bin").exists());
    }
}
