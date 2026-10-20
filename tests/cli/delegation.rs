//! External components retain exact streams, native arguments, exit status and cancellation.

#[cfg(unix)]
mod unix {
    use crate::support::TestDir;
    use nix::{
        sys::signal::{Signal, kill},
        unistd::Pid,
    };
    use serde_json::Value;
    use std::{
        ffi::OsString,
        fs,
        io::Write,
        os::unix::ffi::{OsStrExt, OsStringExt},
        path::{Path, PathBuf},
        process::{Child, Command, Output, Stdio},
        thread,
        time::{Duration, Instant},
    };

    /// Compare OS argument bytes with Python's reversible fsencoding independently of UTF-8.
    fn hex(bytes: &[u8]) -> String {
        use std::fmt::Write;
        let mut encoded = String::new();
        for byte in bytes {
            write!(encoded, "{byte:02x}").unwrap();
        }
        encoded
    }

    /// The executable inode stays immutable while parallel tests start the fixture.
    fn fixture() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/component.py")
    }

    struct Owned(Child);

    impl Drop for Owned {
        /// Reap the test's wrapper even when an assertion fails before normal shutdown.
        fn drop(&mut self) {
            if !matches!(self.0.try_wait(), Ok(Some(_))) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
    }

    /// Stop polling within a fixed deadline instead of hanging the suite on a delegation failure.
    fn finish(child: &mut Child) -> Output {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                use std::io::Read;
                let mut stdout = Vec::new();
                let mut stderr = Vec::new();
                child
                    .stdout
                    .take()
                    .unwrap()
                    .read_to_end(&mut stdout)
                    .unwrap();
                child
                    .stderr
                    .take()
                    .unwrap()
                    .read_to_end(&mut stderr)
                    .unwrap();
                return Output {
                    status,
                    stdout,
                    stderr,
                };
            }
            assert!(Instant::now() < deadline, "wrapper did not finish");
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// Preserve argument boundaries, Unicode, arbitrary bytes and inherited binary standard streams.
    #[test]
    fn exact_streams_arguments_and_child_exit_are_preserved() {
        for directory_arg in [
            OsString::from("status"),
            OsString::from("каталог status с пробелами"),
            OsString::from_vec(b"arbitrary-\xff-path".to_vec()),
        ] {
            let directory = TestDir::new();
            let record = directory.0.join("arguments.json");
            let mut args: Vec<OsString> = [
                "--lang",
                "en",
                "--project-dir",
                "status",
                "status",
                "--format",
                "json",
            ]
            .into_iter()
            .map(Into::into)
            .collect();
            // A global option value identical to a command must remain an option value.
            args[3] = directory_arg;
            let mut child = Owned(
                Command::new(env!("CARGO_BIN_EXE_eska"))
                    .current_dir(&directory.0)
                    .env("ESKA_VCS_BINARY", fixture())
                    .env("ESKA_COMPONENT_FIXTURE_ARGS", &record)
                    .args(&args)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap(),
            );
            let input = b"stdin:\x00\xff\r\n";
            child.0.stdin.take().unwrap().write_all(input).unwrap();
            let output = finish(&mut child.0);
            assert_eq!(output.status.code(), Some(197));
            assert_eq!(output.stdout, [b"stdout:\x00".as_slice(), input].concat());
            assert_eq!(output.stderr, b"stderr:\xff\x00");
            let record: Value = serde_json::from_slice(&fs::read(record).unwrap()).unwrap();
            assert_eq!(record["cwd"], directory.0.to_str().unwrap());
            let encoded: Vec<String> = args.iter().map(|arg| hex(arg.as_bytes())).collect();
            assert_eq!(record["args"], serde_json::json!(encoded));
        }
    }

    /// The project-dir value named ide cannot be mistaken for the command being adapted.
    #[test]
    fn legacy_protocol_identity_is_selected_only_for_the_ide_command() {
        let directory = TestDir::new();
        let record = directory.0.join("arguments.json");
        let output = Command::new(env!("CARGO_BIN_EXE_eska"))
            .current_dir(&directory.0)
            .env("ESKA_PROJECT_BINARY", fixture())
            .env("ESKA_COMPONENT_FIXTURE_ARGS", &record)
            .args(["--project-dir", "ide", "ide", "--stdio"])
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(197));
        let record: Value = serde_json::from_slice(&fs::read(record).unwrap()).unwrap();
        let expected: Vec<String> = [
            "--project-dir",
            "ide",
            "serve",
            "--legacy-identity",
            "--stdio",
        ]
        .into_iter()
        .map(|value| hex(value.as_bytes()))
        .collect();
        assert_eq!(record["args"], serde_json::json!(expected));
    }

    /// Programmatic wrapper signals reach its owned child and preserve the child's cleanup status.
    #[test]
    fn signals_are_forwarded_and_the_child_is_reaped() {
        for signal in [Signal::SIGINT, Signal::SIGTERM, Signal::SIGHUP] {
            let directory = TestDir::new();
            let ready = directory.0.join("ready");
            let mut child = Owned(
                Command::new(env!("CARGO_BIN_EXE_eska"))
                    .current_dir(&directory.0)
                    .env("ESKA_VCS_BINARY", fixture())
                    .env("ESKA_COMPONENT_FIXTURE_MODE", "signals")
                    .env("ESKA_COMPONENT_FIXTURE_READY", &ready)
                    .args(["--lang", "en", "status", "--format", "json"])
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap(),
            );
            let deadline = Instant::now() + Duration::from_secs(5);
            while !ready.is_file() {
                assert!(Instant::now() < deadline, "component was not ready");
                assert!(child.0.try_wait().unwrap().is_none());
                thread::sleep(Duration::from_millis(10));
            }
            let pid: i32 = fs::read_to_string(&ready).unwrap().parse().unwrap();
            kill(Pid::from_raw(i32::try_from(child.0.id()).unwrap()), signal).unwrap();
            let output = finish(&mut child.0);
            assert_eq!(output.status.code(), Some(70 + signal as i32), "{output:?}");
            assert_eq!(
                output.stderr,
                format!("signal:{}\n", signal as i32).as_bytes()
            );
            assert_eq!(
                kill(Pid::from_raw(pid), None),
                Err(nix::errno::Errno::ESRCH)
            );
        }
    }
}

use crate::support::TestDir;
use std::{fs, process::Command};

/// An explicitly broken component path must not fall back to an installed sibling or modify files.
#[test]
fn explicit_missing_component_is_localized_and_help_remains_local() {
    let fixture = TestDir::new();
    for (locale, expected) in [("ru", "Компонент"), ("en", "Component")] {
        let missing = fixture.0.join("missing component");
        let output = Command::new(env!("CARGO_BIN_EXE_eska"))
            .current_dir(&fixture.0)
            .env("ESKA_VCS_BINARY", &missing)
            .args(["--lang", locale, "status", "--format", "json"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{output:?}"
        );
        let help = Command::new(env!("CARGO_BIN_EXE_eska"))
            .current_dir(&fixture.0)
            .env("ESKA_VCS_BINARY", &missing)
            .args(["--lang", locale, "--help"])
            .output()
            .unwrap();
        assert!(help.status.success(), "{help:?}");
        assert!(help.stderr.is_empty());
    }
    assert!(fs::read_dir(&fixture.0).unwrap().next().is_none());
}

/// Required VCS absence is detected before new/init writes; no-vcs keeps structural work usable.
#[test]
fn onboarding_preflights_required_vcs_before_writing() {
    for locale in ["ru", "en"] {
        let fixture = TestDir::new();
        let missing = fixture.0.join("missing vcs");
        let create = |no_vcs: bool| {
            let mut command = Command::new(env!("CARGO_BIN_EXE_eska"));
            command
                .current_dir(&fixture.0)
                .env("ESKA_VCS_BINARY", &missing)
                .args([
                    "--lang",
                    locale,
                    "new",
                    "project",
                    "--type",
                    "configuration",
                    "--workflow",
                    "trunk",
                ]);
            if no_vcs {
                command.arg("--no-vcs");
            }
            command.output().unwrap()
        };
        let refused = create(false);
        assert_eq!(refused.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&refused.stderr).contains("eska-vcs"));
        assert!(!fixture.0.join("project").exists());
        assert!(create(true).status.success());
        assert!(!fixture.0.join("project/.git").exists());
        let root = fixture.0.join("existing");
        let ibcmd = super::build::fake_ibcmd(&fixture);
        fs::create_dir(&root).unwrap();
        let bytes = b"<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\"><ExternalDataProcessor uuid=\"11111111-1111-1111-1111-111111111111\"><Properties><Name>Existing</Name></Properties></ExternalDataProcessor></MetaDataObject>\r\n";
        fs::write(root.join("Existing.xml"), bytes).unwrap();
        let initialize = |no_vcs: bool| {
            let mut command = Command::new(env!("CARGO_BIN_EXE_eska"));
            command
                .current_dir(&root)
                .env("ESKA_VCS_BINARY", &missing)
                .args([
                    "--lang",
                    locale,
                    "init",
                    "--workflow",
                    "trunk",
                    "--platform-version",
                    "8.3.27.2325",
                    "--ibcmd",
                ])
                .arg(&ibcmd);
            if no_vcs {
                command.arg("--no-vcs");
            }
            command.output().unwrap()
        };
        let refused = initialize(false);
        assert_eq!(refused.status.code(), Some(1), "{refused:?}");
        assert!(String::from_utf8_lossy(&refused.stderr).contains("eska-vcs"));
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        assert_eq!(fs::read(root.join("Existing.xml")).unwrap(), bytes);
        assert!(initialize(true).status.success());
        assert!(!root.join(".git").exists());
    }
}
