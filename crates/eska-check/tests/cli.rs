//! Process boundaries prove standalone CI behavior, locale independence and explicit exit statuses.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

#[cfg(unix)]
use std::{
    process::{Child, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde_json::Value;

#[path = "../../../tests/support/mod.rs"]
mod support;

#[cfg(unix)]
struct Fixture {
    directory: support::TestDir,
    root: PathBuf,
    binary: PathBuf,
}

#[cfg(unix)]
struct OwnedChild(Option<Child>);

#[cfg(unix)]
impl Drop for OwnedChild {
    /// Failed assertions must still reap the owned CLI process.
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(unix)]
impl Fixture {
    /// Use an immutable external fixture and a real Designer root, with no full CLI on PATH.
    #[cfg(unix)]
    fn new() -> Self {
        let directory = support::TestDir::new();
        let root = directory.0.join("Проект");
        create(&root);
        let binary = directory.0.join("analyzer fixture");
        std::os::unix::fs::symlink(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/analyzer-check.py"),
            &binary,
        )
        .unwrap();
        Self {
            directory,
            root,
            binary,
        }
    }

    /// PATH contains no wrapper, Git, project server or platform executable.
    fn command(&self, locale: &str) -> Command {
        let mut command = command(&self.root, &self.binary, locale);
        command.env("PATH", "");
        command
    }
}

/// Generate only the input under test; UTF-8 BOM/CRLF must survive checks unchanged.
fn create(root: &Path) {
    fs::create_dir_all(root.join("Выгрузка XML/Ext")).unwrap();
    fs::write(
        root.join("eska.toml"),
        "[project]\ntype='configuration'\nsource='Выгрузка XML'\n",
    )
    .unwrap();
    fs::write(root.join("Выгрузка XML/Configuration.xml"), "\u{feff}<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\"><Configuration uuid=\"11111111-1111-1111-1111-111111111111\"><Properties><Name>Пример</Name><CompatibilityMode>Version8_3_27</CompatibilityMode></Properties><ChildObjects/></Configuration></MetaDataObject>\r\n").unwrap();
    fs::write(
        root.join("Выгрузка XML/Ext/ObjectModule.bsl"),
        "\u{feff}Процедура Пример()\r\n    А = 1;\r\nКонецПроцедуры\r\n",
    )
    .unwrap();
}

/// Every argument remains a separate OS string; neither paths nor JSON pass through a shell.
fn command(root: &Path, binary: &Path, locale: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_eska-check"));
    command
        .current_dir(root)
        .args(["--lang", locale, "--format", "json", "--project-dir"])
        .arg(root)
        .arg("--analyzer")
        .arg(binary);
    command
}

/// A single parseable envelope is the entire machine-readable stdout, even on errors.
fn document(output: &Output, code: i32) -> Value {
    assert_eq!(
        output.status.code(),
        Some(code),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

/// Finished diagnostics obey root policy and persist the same locale-independent report.
#[cfg(unix)]
#[test]
fn standalone_reports_passed_and_threshold_exceeded_in_both_locales() {
    let fixture = Fixture::new();
    let module = fixture.root.join("Выгрузка XML/Ext/ObjectModule.bsl");
    let before = fs::read(&module).unwrap();
    fs::write(fixture.root.join("stream-mode"), "warning").unwrap();
    for (threshold, code, status) in [(false, 0, "passed"), (true, 1, "threshold_exceeded")] {
        if threshold {
            fs::write(fixture.root.join("eska.toml"), "[project]\ntype='configuration'\nsource='Выгрузка XML'\n[tools.eska-check]\nmax_warnings=0\n").unwrap();
        }
        let mut expected = None;
        for locale in ["ru", "en"] {
            let path = fixture
                .directory
                .0
                .join(format!("{threshold}-{locale}.json"));
            let output = fixture
                .command(locale)
                .arg("--report")
                .arg(&path)
                .output()
                .unwrap();
            let report = document(&output, code);
            assert_eq!(report["status"], status);
            assert_eq!(report["complete"], true);
            assert_eq!(report["scope"], "full");
            assert_eq!(report["counts"]["warnings"], 1);
            assert_eq!(report["files"].as_array().unwrap().len(), 1);
            assert_eq!(report["context"]["limited_context"], false);
            assert_eq!(output.stderr, [] as [u8; 0]);
            assert_eq!(fs::read(path).unwrap(), output.stdout);
            if let Some(expected) = &expected {
                assert_eq!(&output.stdout, expected);
            } else {
                expected = Some(output.stdout);
            }
        }
    }
    assert_eq!(fs::read(module).unwrap(), before);
    assert!(!fixture.root.join(".git").exists());
}

/// Exit 3 and complete=false distinguish a tool failure from threshold exceedance.
#[cfg(unix)]
#[test]
fn incomplete_stream_and_wrong_version_are_never_green() {
    for mode in [
        "no-done",
        "failed-files",
        "exit-after-done",
        "wrong-version",
    ] {
        let fixture = Fixture::new();
        fs::write(fixture.root.join("stream-mode"), mode).unwrap();
        let mut expected = None;
        for locale in ["ru", "en"] {
            let output = fixture.command(locale).output().unwrap();
            let report = document(&output, 3);
            assert_eq!(report["status"], "incomplete");
            assert_eq!(report["complete"], false);
            assert!(report.get("files").is_none());
            assert_ne!(output.stderr, [] as [u8; 0]);
            if let Some(expected) = &expected {
                assert_eq!(&output.stdout, expected);
            } else {
                expected = Some(output.stdout);
            }
        }
    }
}

/// Preparation failures preserve foreign configuration and do not start an analysis process.
#[cfg(unix)]
#[test]
fn bad_manifest_foreign_config_and_missing_analyzer_are_incomplete() {
    for mode in [
        "manifest",
        "foreign-config",
        "missing-analyzer",
        "application",
    ] {
        let fixture = Fixture::new();
        match mode {
            "manifest" => fs::write(fixture.root.join("eska.toml"), "broken = [").unwrap(),
            "foreign-config" => {
                fs::write(fixture.root.join("bsl-analyzer.toml"), b"# user config\r\n").unwrap();
            }
            "missing-analyzer" => fs::remove_file(&fixture.binary).unwrap(),
            _ => {}
        }
        let mut command = fixture.command("en");
        if mode == "application" {
            command.args(["--application", "absent"]);
        }
        let output = command.output().unwrap();
        let report = document(&output, 3);
        assert_eq!(report["complete"], false);
        assert_eq!(
            report["error"]["code"],
            match mode {
                "foreign-config" => "migration_required",
                "missing-analyzer" => "process_start",
                _ => "context",
            }
        );
        assert!(!fixture.root.join("analysis-started").exists());
        if mode == "foreign-config" {
            assert_eq!(
                fs::read(fixture.root.join("bsl-analyzer.toml")).unwrap(),
                b"# user config\r\n"
            );
        }
    }
}

/// Report preflight preserves existing data and never creates an output inside analyzed sources.
#[cfg(unix)]
#[test]
fn report_destination_is_new_and_outside_sources() {
    let fixture = Fixture::new();
    let report_path = fixture.directory.0.join("existing.json");
    fs::write(&report_path, b"user bytes").unwrap();
    for path in [&report_path, &fixture.root.join("Выгрузка XML/new.bsl")] {
        let output = fixture
            .command("en")
            .arg("--report")
            .arg(path)
            .output()
            .unwrap();
        let report = document(&output, 3);
        assert_eq!(report["error"]["code"], "report_write");
        assert!(!fixture.root.join(".eska").exists());
    }
    assert_eq!(fs::read(report_path).unwrap(), b"user bytes");
    assert!(!fixture.root.join("Выгрузка XML/new.bsl").exists());
}

/// Human summaries and errors are localized while upstream messages remain intact.
#[cfg(unix)]
#[test]
fn human_output_is_localized_in_both_languages() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("stream-mode"), "warning").unwrap();
    for locale in ["ru", "en"] {
        let report_path = fixture.directory.0.join(format!("human-{locale}.json"));
        let output = Command::new(env!("CARGO_BIN_EXE_eska-check"))
            .current_dir(&fixture.root)
            .args(["--lang", locale, "--analyzer"])
            .arg(&fixture.binary)
            .arg("--report")
            .arg(&report_path)
            .env("PATH", "")
            .output()
            .unwrap();
        assert!(output.status.success());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains(if locale == "ru" {
            "Проверка пройдена"
        } else {
            "Check passed"
        }));
        assert!(text.contains(if locale == "ru" {
            "предупреждение FixtureRule"
        } else {
            "warning FixtureRule"
        }));
        assert_eq!(output.stderr, [] as [u8; 0]);
        let json = fixture.command(locale).output().unwrap();
        assert!(json.status.success());
        assert_eq!(fs::read(report_path).unwrap(), json.stdout);
    }
    fs::write(fixture.root.join("stream-mode"), "no-done").unwrap();
    for locale in ["ru", "en"] {
        let output = Command::new(env!("CARGO_BIN_EXE_eska-check"))
            .current_dir(&fixture.root)
            .args(["--lang", locale, "--analyzer"])
            .arg(&fixture.binary)
            .env("PATH", "")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(3));
        assert_eq!(output.stdout, [] as [u8; 0]);
        let text = String::from_utf8(output.stderr).unwrap();
        assert!(text.contains(if locale == "ru" {
            "неполный машинный результат"
        } else {
            "incomplete machine output"
        }));
    }
}

/// Timeouts and actual SIGINT cancellation leave neither a green report nor a surviving analyzer.
#[cfg(unix)]
#[test]
fn timeout_and_interrupt_have_distinct_exit_codes_and_reap_analyzer() {
    for interrupt in [false, true] {
        let fixture = Fixture::new();
        fs::write(fixture.root.join("stream-mode"), "hang").unwrap();
        let child = fixture
            .command("en")
            .args(["--timeout", "1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut child = OwnedChild(Some(child));
        let marker = fixture.root.join("analysis-started");
        let deadline = Instant::now() + Duration::from_secs(5);
        while !marker.exists() {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        if interrupt {
            assert!(
                Command::new("kill")
                    .args(["-INT", &child.0.as_ref().unwrap().id().to_string()])
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while child.0.as_mut().unwrap().try_wait().unwrap().is_none() {
            assert!(
                Instant::now() < deadline,
                "CLI must terminate after cancellation or timeout"
            );
            thread::sleep(Duration::from_millis(5));
        }
        let output = child.0.take().unwrap().wait_with_output().unwrap();
        let report = document(&output, if interrupt { 130 } else { 3 });
        assert_eq!(
            report["error"]["code"],
            if interrupt { "cancelled" } else { "timeout" }
        );
        assert_eq!(report["complete"], false);
        #[cfg(target_os = "linux")]
        {
            let pid = fs::read_to_string(marker).unwrap();
            assert!(!Path::new("/proc").join(pid).exists());
        }
    }
}

/// Ordinary help requires no project or external tools and never advertises changed-only analysis.
#[test]
fn help_is_localized_and_partial_flags_are_rejected() {
    for locale in ["ru", "en"] {
        let output = Command::new(env!("CARGO_BIN_EXE_eska-check"))
            .args(["--lang", locale, "--help"])
            .env("PATH", "")
            .output()
            .unwrap();
        assert!(output.status.success());
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.contains(if locale == "ru" {
            "Использование"
        } else {
            "Usage"
        }));
        assert!(help.contains("--report"));
        assert!(!help.contains("--changed-files"));
        let output = Command::new(env!("CARGO_BIN_EXE_eska-check"))
            .arg("--changed-files")
            .arg("a.bsl")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
    }
}

/// A real published executable supplies diagnostics with only the CI component installed on PATH.
#[test]
#[ignore = "requires ESKA_BSL_ANALYZER pointing to published bsl-analyzer-app 0.4.0"]
fn published_analyzer_produces_the_same_real_report_for_ru_and_en() {
    let directory = support::TestDir::new();
    let root = directory.0.join("Проект");
    create(&root);
    let binary = PathBuf::from(std::env::var_os("ESKA_BSL_ANALYZER").expect("explicit analyzer"));
    let before = fs::read(root.join("Выгрузка XML/Ext/ObjectModule.bsl")).unwrap();
    let mut expected = None;
    for locale in ["ru", "en"] {
        let output = command(&root, &binary, locale)
            .env("PATH", "")
            .output()
            .unwrap();
        assert!(
            matches!(output.status.code(), Some(0 | 1)),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["complete"], true);
        assert_ne!(
            report["files"][0]["diagnostics"]
                .as_array()
                .unwrap()
                .as_slice(),
            [] as [serde_json::Value; 0]
        );
        if let Some(expected) = &expected {
            assert_eq!(&output.stdout, expected);
        } else {
            expected = Some(output.stdout);
        }
    }
    assert_eq!(
        fs::read(root.join("Выгрузка XML/Ext/ObjectModule.bsl")).unwrap(),
        before
    );
}
