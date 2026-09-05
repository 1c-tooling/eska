#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

use serde_json::Value;

use crate::support::TestDir;

/// Run eska with an isolated locale and optional fake import failure.
fn eska(current_dir: &Path, locale: &str, ibcmd: &Path, args: &[&str], fail: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_eska"));
    command
        .current_dir(current_dir)
        .env_remove("ESKA_LANG")
        .env("FAKE_IBCMD_FAIL_IMPORT", if fail { "1" } else { "0" })
        .args(["--lang", locale])
        .args(args)
        .args(["--ibcmd"])
        .arg(ibcmd)
        .output()
        .expect("run eska")
}

/// Create a minimal project through its public CLI.
fn project(fixture: &TestDir, project_type: &str, name: &str) -> PathBuf {
    let output = Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(&fixture.0)
        .args([
            "--lang",
            "en",
            "new",
            name,
            "--type",
            project_type,
            "--workflow",
            "trunk",
            "--no-vcs",
        ])
        .output()
        .expect("create project");
    assert!(output.status.success(), "{output:?}");
    let root = fixture.0.join(name);
    if let Some(tag) = match project_type {
        "processing" => Some("ExternalDataProcessor"),
        "report" => Some("ExternalReport"),
        _ => None,
    } {
        fs::write(
            root.join("src").join(format!("{name}.xml")),
            format!(
                r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"><{tag}/></MetaDataObject>"#
            ),
        )
        .expect("write root descriptor");
    }
    root
}

/// Install an executable fake that implements the verified ibcmd calls.
fn fake_ibcmd(fixture: &TestDir) -> PathBuf {
    let path = fixture.0.join("ibcmd");
    fs::write(
        &path,
        r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "1C ibcmd version 8.3.27.2325"
  exit 0
fi
if [ "$1" = "infobase" ] && [ "$2" = "create" ]; then
  for argument in "$@"; do
    case "$argument" in --data=*) mkdir -p "${argument#--data=}";; esac
  done
  exit 0
fi
if [ "$1" = "config" ] && [ "$2" = "import" ]; then
  if [ "$FAKE_IBCMD_SLOW_IMPORT" = "1" ]; then
    exec sleep 30
  fi
  if [ "$FAKE_IBCMD_FAIL_IMPORT" = "1" ]; then
    echo "fake import failure" >&2
    exit 7
  fi
  for argument in "$@"; do
    case "$argument" in --out=*) printf 'native-artifact' > "${argument#--out=}";; esac
  done
  echo "[WARN] fake build warning"
  exit 0
fi
exit 9
"#,
    )
    .expect("write fake ibcmd");
    let mut permissions = fs::metadata(&path).expect("fake metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).expect("make fake executable");
    path
}

#[test]
/// Build every supported project type with its native extension and stable JSON schema.
fn builds_all_native_artifact_types_with_locale_independent_json() {
    let fixture = TestDir::new();
    let ibcmd = fake_ibcmd(&fixture);
    for (project_type, artifact_type, extension) in [
        ("configuration", "configuration", "cf"),
        ("extension", "extension", "cfe"),
        ("processing", "processing", "epf"),
        ("report", "report", "erf"),
    ] {
        let root = project(&fixture, project_type, &format!("Demo{extension}"));
        for locale in ["ru", "en"] {
            let output = eska(&root, locale, &ibcmd, &["build", "--format", "json"], false);
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                String::from_utf8_lossy(&output.stderr),
                "[WARN] fake build warning\n"
            );
            let document: Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
            assert_eq!(document["schema_version"], 1);
            assert_eq!(document["artifact"]["type"], artifact_type);
            assert_eq!(document["artifact"]["path_encoding"], "utf-8");
            assert_eq!(document["platform"]["version"], "8.3.27.2325");
            assert!(document["duration_ms"].is_number());
            let artifact = root
                .join("build")
                .join(format!("Demo{extension}.{extension}"));
            assert_eq!(
                document["artifact"]["path"],
                artifact.to_string_lossy().as_ref()
            );
            assert_eq!(fs::read(&artifact).expect("artifact"), b"native-artifact");
        }
    }
}

#[test]
/// Keep an existing artifact and remove all owned temporary data after ibcmd failure.
fn failed_build_preserves_existing_artifact_and_cleans_workspace() {
    let fixture = TestDir::new();
    let ibcmd = fake_ibcmd(&fixture);
    let root = project(&fixture, "configuration", "Billing");
    let build = root.join("build");
    fs::create_dir(&build).expect("build directory");
    let artifact = build.join("Billing.cf");
    fs::write(&artifact, "previous").expect("old artifact");

    for (locale, expected) in [("ru", "завершился ошибкой"), ("en", "failed")] {
        let output = eska(&root, locale, &ibcmd, &["build"], true);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains(expected));
        assert_eq!(
            fs::read_to_string(&artifact).expect("old artifact"),
            "previous"
        );
        let leftovers: Vec<_> = fs::read_dir(&build)
            .expect("build directory")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".eska-"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temporary paths remain: {leftovers:?}"
        );
    }
}

#[test]
/// Reject mismatched versions before creating an output directory or artifact.
fn exact_platform_version_is_required() {
    let fixture = TestDir::new();
    let ibcmd = fake_ibcmd(&fixture);
    let root = project(&fixture, "configuration", "Versioned");
    let config = root.join("eska.toml");
    let value = fs::read_to_string(&config).expect("config");
    fs::write(
        &config,
        format!("{value}\n[build]\nplatform_version = \"8.3.26.1540\"\n"),
    )
    .expect("configure version");

    let output = eska(&root, "en", &ibcmd, &["build"], false);
    assert_eq!(output.status.code(), Some(1));
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("8.3.27.2325"), "{error}");
    assert!(error.contains("8.3.26.1540"), "{error}");
    assert!(!root.join("build").exists());
}

#[test]
/// Terminate the active child and remove the temporary infobase after SIGTERM.
fn interrupted_build_cleans_all_owned_paths() {
    let fixture = TestDir::new();
    let ibcmd = fake_ibcmd(&fixture);
    let root = project(&fixture, "configuration", "Interrupted");
    let child = Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(&root)
        .env("FAKE_IBCMD_SLOW_IMPORT", "1")
        .args(["--lang", "en", "build", "--ibcmd"])
        .arg(&ibcmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start build");
    std::thread::sleep(std::time::Duration::from_millis(300));
    let signal = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("signal build");
    assert!(signal.success());
    let output = child
        .wait_with_output()
        .expect("wait for interrupted build");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Build interrupted"));
    assert!(!root.join("build").exists());
}

#[test]
/// Localize help and successful human output in both supported locales.
fn help_and_human_result_are_localized() {
    let fixture = TestDir::new();
    let ibcmd = fake_ibcmd(&fixture);
    let root = project(&fixture, "extension", "Localized");
    for (locale, help_text, result_text) in [
        ("ru", "Собрать нативный артефакт", "Собран"),
        ("en", "Build a native 1C artifact", "Built"),
    ] {
        let help = Command::new(env!("CARGO_BIN_EXE_eska"))
            .current_dir(&root)
            .args(["--lang", locale, "build", "--help"])
            .output()
            .expect("build help");
        assert!(help.status.success());
        assert!(String::from_utf8_lossy(&help.stdout).contains(help_text));

        let output = eska(&root, locale, &ibcmd, &["build"], false);
        assert!(output.status.success(), "{output:?}");
        assert!(String::from_utf8_lossy(&output.stdout).contains(result_text));
        assert!(String::from_utf8_lossy(&output.stderr).contains("[WARN] fake build warning"));
    }
}
