#![cfg(unix)]

use std::{
    fs,
    io::{BufRead, BufReader, Read},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    thread,
    time::Duration,
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
    let config_path = root.join("eska.toml");
    let config = fs::read_to_string(&config_path).expect("read project config");
    fs::write(
        &config_path,
        config.replace(
            "platform_version = \"\"",
            "platform_version = \"8.3.27.2325\"",
        ),
    )
    .expect("configure build platform");
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

/// Create a workspace with one report and one processing in manifest order.
fn workspace() -> TestDir {
    let fixture = TestDir::new();
    let report = fixture.0.join("src/sales-report");
    let processing = fixture.0.join("src/import-orders");
    fs::create_dir_all(&report).expect("report member");
    fs::create_dir_all(&processing).expect("processing member");
    fs::write(
        fixture.0.join("eska.toml"),
        concat!(
            "[workspace]\n",
            "members = ['src/sales-report', 'src/import-orders']\n\n",
            "[build]\n",
            "platform_version = '8.3.27.2325'\n",
            "artifacts_directory = 'build'\n",
        ),
    )
    .expect("workspace config");
    fs::write(
        report.join("eska.toml"),
        "[project]\nname = 'sales-report'\ntype = 'report'\nsource = '.'\n",
    )
    .expect("report config");
    fs::write(
        processing.join("eska.toml"),
        "[project]\nname = 'import-orders'\ntype = 'processing'\nsource = '.'\n",
    )
    .expect("processing config");
    fs::write(
        report.join("SalesReport.xml"),
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"><ExternalReport/></MetaDataObject>"#,
    )
    .expect("report descriptor");
    fs::write(
        processing.join("ImportOrders.xml"),
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"><ExternalDataProcessor/></MetaDataObject>"#,
    )
    .expect("processing descriptor");
    fixture
}

/// Install an executable fake that implements the verified ibcmd calls.
fn fake_ibcmd(fixture: &TestDir) -> PathBuf {
    let path = fixture.0.join("ibcmd");
    fs::write(
        &path,
        r#"#!/bin/sh
if [ -n "$FAKE_IBCMD_LOG" ]; then
  printf '%s\n' "$*" >> "$FAKE_IBCMD_LOG"
fi
if [ "$1" = "--version" ]; then
  echo "1C ibcmd version ${FAKE_IBCMD_VERSION:-8.3.27.2325}"
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
  output=
  source=
  for argument in "$@"; do
    case "$argument" in
      --out=*) output="${argument#--out=}";;
      --*) ;;
      *) source="$argument";;
    esac
  done
  if [ "$FAKE_IBCMD_FAIL_IMPORT" = "1" ]; then
    echo "fake import failure" >&2
    exit 7
  fi
  case "$source" in
    *"$FAKE_IBCMD_FAIL_SOURCE_CONTAINS"*)
      if [ -n "$FAKE_IBCMD_FAIL_SOURCE_CONTAINS" ]; then
        echo "fake selective import failure" >&2
        exit 7
      fi;;
  esac
  if [ -n "$FAKE_IBCMD_IMPORT_READY" ]; then
    : > "$FAKE_IBCMD_IMPORT_READY"
    while [ ! -f "$FAKE_IBCMD_IMPORT_CONTINUE" ]; do sleep 0.05; done
  fi
  if [ "$FAKE_IBCMD_STREAM" = "1" ]; then
    echo "[INFO] File: $source/DataProcessors/РаботаСФайлами/Forms/ПрисоединенныйФайл/Ext/Help/ru.html, checking"
    sleep 2
  fi
  if [ -n "$FAKE_IBCMD_ARTIFACT_SOURCE_FILE" ]; then
    source_root="$source"
    if [ -f "$source" ]; then source_root="${source%/*}"; fi
    cat "$source_root/$FAKE_IBCMD_ARTIFACT_SOURCE_FILE" > "$output"
  else
    printf 'native-artifact' > "$output"
  fi
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

/// Capture directory names and file bytes without following links.
fn tree_snapshot(root: &Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
    let mut entries = Vec::new();
    collect_tree_snapshot(root, root, &mut entries);
    entries
}

/// Append one directory subtree in deterministic path order.
fn collect_tree_snapshot(
    root: &Path,
    directory: &Path,
    entries: &mut Vec<(PathBuf, Option<Vec<u8>>)>,
) {
    let mut paths = fs::read_dir(directory)
        .expect("snapshot directory")
        .map(|entry| entry.expect("snapshot entry").path())
        .collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        let relative = path.strip_prefix(root).expect("snapshot path").to_owned();
        let file_type = fs::symlink_metadata(&path)
            .expect("snapshot metadata")
            .file_type();
        if file_type.is_dir() {
            entries.push((relative, None));
            collect_tree_snapshot(root, &path, entries);
        } else {
            entries.push((relative, Some(fs::read(&path).expect("snapshot file"))));
        }
    }
}

#[test]
/// Show a localized single-project plan and preserve every filesystem entry byte-for-byte.
fn dry_run_human_is_localized_and_does_not_change_the_filesystem() {
    for (locale, title, source, artifact_type, replaces, required, found, runner) in [
        (
            "ru",
            "Предварительный просмотр build",
            "Исходники:",
            "Тип артефакта: конфигурация (.cf)",
            "Заменит существующий артефакт: да",
            "Требуемая платформа: 8.3.27.2325",
            "Найденная платформа: 8.3.27.2325",
            "Среда запуска: хост",
        ),
        (
            "en",
            "Build preview",
            "Source:",
            "Artifact type: configuration (.cf)",
            "Replaces existing artifact: yes",
            "Required platform: 8.3.27.2325",
            "Found platform: 8.3.27.2325",
            "Runner: host",
        ),
    ] {
        let fixture = TestDir::new();
        let ibcmd = fake_ibcmd(&fixture);
        let root = project(&fixture, "configuration", "Preview");
        fs::create_dir(root.join("build")).expect("build directory");
        fs::write(root.join("build/Preview.cf"), "existing artifact").expect("existing artifact");
        let before = tree_snapshot(&fixture.0);

        let output = eska(&root, locale, &ibcmd, &["build", "--dry-run"], false);

        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        let stdout = String::from_utf8_lossy(&output.stdout).replace(['\u{2068}', '\u{2069}'], "");
        for expected in [
            title,
            "1.",
            source,
            artifact_type,
            replaces,
            required,
            found,
            runner,
        ] {
            assert!(
                stdout.contains(expected),
                "missing `{expected}` in:\n{stdout}"
            );
        }
        assert!(stdout.contains(root.join("src").to_string_lossy().as_ref()));
        assert!(stdout.contains(root.join("build/Preview.cf").to_string_lossy().as_ref()));
        assert!(!stdout.contains('\x1b'));
        assert_eq!(tree_snapshot(&fixture.0), before);
    }
}

#[test]
/// Keep the dry-run JSON schema locale-independent and preserve manifest execution order.
fn dry_run_workspace_json_is_stable_and_ordered() {
    let fixture = workspace();
    let ibcmd = fake_ibcmd(&fixture);
    let before = tree_snapshot(&fixture.0);
    let mut documents = Vec::new();

    for locale in ["ru", "en"] {
        let output = eska(
            &fixture.0,
            locale,
            &ibcmd,
            &["build", "--dry-run", "--format", "json"],
            false,
        );
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        let document: Value = serde_json::from_slice(&output.stdout).expect("build plan JSON");
        assert_eq!(document["schema_version"], 1);
        assert_eq!(document["kind"], "build-plan");
        assert_eq!(document["scope"], "workspace");
        assert_eq!(document["projects"][0]["name"], "sales-report");
        assert_eq!(document["projects"][0]["artifact"]["type"], "report");
        assert_eq!(document["projects"][1]["name"], "import-orders");
        assert_eq!(document["projects"][1]["artifact"]["type"], "processing");
        for project in document["projects"].as_array().expect("projects") {
            assert_eq!(project["root"]["path_encoding"], "utf-8");
            assert_eq!(project["source"]["path_encoding"], "utf-8");
            assert_eq!(project["artifact"]["path_encoding"], "utf-8");
            assert_eq!(project["artifact"]["replaces_existing"], false);
            assert_eq!(project["platform"]["required_version"], "8.3.27.2325");
            assert_eq!(project["platform"]["found_version"], "8.3.27.2325");
            assert_eq!(project["platform"]["runner"], "host");
        }
        documents.push(document);
    }

    assert_eq!(documents[0], documents[1]);
    assert_eq!(tree_snapshot(&fixture.0), before);
    assert!(!fixture.0.join("build").exists());
}

#[test]
/// Use selectors and a one-run platform override in the preview without changing config.
fn dry_run_honors_selector_and_platform_override() {
    let fixture = workspace();
    let ibcmd = fake_ibcmd(&fixture);
    let manifest = fixture.0.join("eska.toml");
    let config_before = fs::read(&manifest).expect("workspace config");
    let output = Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(&fixture.0)
        .env("FAKE_IBCMD_VERSION", "8.5.4.1234")
        .args([
            "--lang",
            "en",
            "build",
            "--dry-run",
            "-p",
            "import-orders",
            "--platform-version",
            "8.5.4.1234",
            "--format",
            "json",
            "--ibcmd",
        ])
        .arg(&ibcmd)
        .output()
        .expect("preview selected member");

    assert!(output.status.success(), "{output:?}");
    let document: Value = serde_json::from_slice(&output.stdout).expect("selected plan JSON");
    assert_eq!(document["scope"], "project");
    assert_eq!(document["projects"].as_array().map(Vec::len), Some(1));
    assert_eq!(document["projects"][0]["name"], "import-orders");
    assert_eq!(
        document["projects"][0]["platform"]["required_version"],
        "8.5.4.1234"
    );
    assert_eq!(
        document["projects"][0]["platform"]["found_version"],
        "8.5.4.1234"
    );
    assert_eq!(
        fs::read(&manifest).expect("workspace config"),
        config_before
    );
    assert!(!fixture.0.join("build").exists());
}

#[test]
/// Reuse the exact preview artifact, type and platform in the subsequent normal build.
fn dry_run_matches_the_subsequent_build_plan() {
    let fixture = TestDir::new();
    let ibcmd = fake_ibcmd(&fixture);
    let root = project(&fixture, "processing", "Planned");
    let log = fixture.0.join("ibcmd.log");
    let preview = Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(&root)
        .env("FAKE_IBCMD_LOG", &log)
        .args([
            "--lang",
            "en",
            "build",
            "--dry-run",
            "--format",
            "json",
            "--ibcmd",
        ])
        .arg(&ibcmd)
        .output()
        .expect("preview build");
    assert!(preview.status.success(), "{preview:?}");
    let plan: Value = serde_json::from_slice(&preview.stdout).expect("plan JSON");
    assert_eq!(
        fs::read_to_string(&log).expect("version invocation"),
        "--version\n"
    );
    assert!(!root.join("build").exists());

    let built = Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(&root)
        .env("FAKE_IBCMD_LOG", &log)
        .args(["--lang", "en", "build", "--format", "json", "--ibcmd"])
        .arg(&ibcmd)
        .output()
        .expect("execute build");
    assert!(built.status.success(), "{built:?}");
    let result: Value = serde_json::from_slice(&built.stdout).expect("result JSON");
    assert_eq!(
        plan["projects"][0]["artifact"]["type"],
        result["artifact"]["type"]
    );
    assert_eq!(
        plan["projects"][0]["artifact"]["path"],
        result["artifact"]["path"]
    );
    assert_eq!(
        plan["projects"][0]["platform"]["required_version"],
        result["platform"]["version"]
    );
    let invocations = fs::read_to_string(log).expect("build invocations");
    assert_eq!(
        invocations
            .lines()
            .filter(|line| *line == "--version")
            .count(),
        2
    );
    assert_eq!(
        invocations
            .lines()
            .filter(|line| line.starts_with("infobase create"))
            .count(),
        1
    );
    assert_eq!(
        invocations
            .lines()
            .filter(|line| line.starts_with("config import"))
            .count(),
        1
    );
}

#[test]
/// Apply an exact platform override to one build without rewriting project settings.
fn platform_version_override_is_ephemeral() {
    let fixture = TestDir::new();
    let ibcmd = fake_ibcmd(&fixture);
    let root = project(&fixture, "configuration", "Override");
    let config_before = fs::read_to_string(root.join("eska.toml")).expect("project config");
    let output = Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(&root)
        .env("ESKA_CONFIG_DIR", fixture.0.join("settings"))
        .env("FAKE_IBCMD_VERSION", "8.5.4.1234")
        .args([
            "--lang",
            "en",
            "build",
            "--platform-version",
            "8.5.4.1234",
            "--ibcmd",
        ])
        .arg(&ibcmd)
        .output()
        .expect("build with override");
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8(output.stderr).expect("stderr");
    assert!(
        stderr.contains("this selection applies only to this build"),
        "{stderr}"
    );
    assert_eq!(
        fs::read_to_string(root.join("eska.toml")).expect("project config after build"),
        config_before
    );
}

#[test]
/// Require an explicit project version before a normal build starts.
fn unconfigured_platform_version_blocks_only_normal_build() {
    let fixture = TestDir::new();
    let ibcmd = fake_ibcmd(&fixture);
    let root = project(&fixture, "configuration", "Unconfigured");
    let config_path = root.join("eska.toml");
    let config = fs::read_to_string(&config_path).expect("project config");
    fs::write(
        &config_path,
        config.replace(
            "platform_version = \"8.3.27.2325\"",
            "platform_version = \"\"",
        ),
    )
    .expect("clear platform version");

    let mut json_errors = Vec::new();
    for (locale, message) in [
        ("ru", "Заполните build.platform_version"),
        ("en", "Fill in build.platform_version"),
    ] {
        let output = eska(&root, locale, &ibcmd, &["build"], false);
        assert!(!output.status.success(), "{output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(message),
            "{output:?}"
        );
        assert!(!root.join("build").exists());

        let output = eska(&root, locale, &ibcmd, &["build", "--format", "json"], false);
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains(message));
        assert!(!output.stdout.contains(&b'\x1b'));
        let document: Value = serde_json::from_slice(&output.stdout).expect("build error JSON");
        assert_eq!(document["schema_version"], 1);
        assert_eq!(document["status"], "error");
        assert_eq!(document["error"]["code"], "platform-version-missing");
        assert_eq!(document["error"]["stage"], "plan");
        assert!(document["error"].get("project").is_none());
        let preview = eska(
            &root,
            locale,
            &ibcmd,
            &["build", "--dry-run", "--format", "json"],
            false,
        );
        assert_eq!(preview.status.code(), Some(1), "{preview:?}");
        let preview_document: Value =
            serde_json::from_slice(&preview.stdout).expect("preview error JSON");
        assert_eq!(preview_document, document);
        json_errors.push(document);
    }
    assert_eq!(json_errors[0], json_errors[1]);

    let override_build = Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(&root)
        .env("ESKA_CONFIG_DIR", fixture.0.join("settings"))
        .args([
            "--lang",
            "en",
            "build",
            "--platform-version",
            "8.3.27.2325",
            "--ibcmd",
        ])
        .arg(&ibcmd)
        .output()
        .expect("build with one-run version");
    assert!(override_build.status.success(), "{override_build:?}");
}

#[test]
/// Emit an ibcmd line before completion and project its source file to a metadata owner.
fn streams_humanized_diagnostics_while_build_is_running() {
    let fixture = TestDir::new();
    let ibcmd = fake_ibcmd(&fixture);
    let root = project(&fixture, "configuration", "Streaming");
    let help =
        root.join("src/DataProcessors/РаботаСФайлами/Forms/ПрисоединенныйФайл/Ext/Help/ru.html");
    fs::create_dir_all(help.parent().expect("help parent")).expect("create help directory");
    fs::write(&help, "help").expect("write help file");

    let mut child = Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(&root)
        .env("FAKE_IBCMD_STREAM", "1")
        .env_remove("NO_COLOR")
        .args(["--lang", "ru", "build", "--ibcmd"])
        .arg(&ibcmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start streaming build");
    let mut stderr = BufReader::new(child.stderr.take().expect("build stderr"));
    let mut first_line = String::new();
    stderr
        .read_line(&mut first_line)
        .expect("read build heading");
    assert_eq!(
        first_line.replace(['\u{2068}', '\u{2069}'], ""),
        "▶ Начало сборки платформой 1С 8.3.27.2325\n"
    );

    first_line.clear();
    stderr
        .read_line(&mut first_line)
        .expect("read first diagnostic");

    assert_eq!(
        first_line,
        "[INFO] File: Обработка.РаботаСФайлами.Форма.ПрисоединенныйФайл · Ext/Help/ru.html, checking\n"
    );
    assert!(
        child.try_wait().expect("inspect running build").is_none(),
        "build completed before its first diagnostic was observed"
    );
    assert!(!first_line.contains(root.to_string_lossy().as_ref()));

    let mut remaining_stderr = String::new();
    stderr
        .read_to_string(&mut remaining_stderr)
        .expect("read remaining diagnostics");
    let mut stdout = String::new();
    child
        .stdout
        .take()
        .expect("build stdout")
        .read_to_string(&mut stdout)
        .expect("read build result");
    let status = child.wait().expect("wait for streaming build");
    assert!(status.success(), "{remaining_stderr}");
    assert_eq!(remaining_stderr, "[WARN] fake build warning\n");
    assert!(stdout.contains("✓ Собран"), "{stdout}");
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

    let mut json_errors = Vec::new();
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

        let output = eska(&root, locale, &ibcmd, &["build", "--format", "json"], true);
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains(expected));
        assert!(!output.stdout.contains(&b'\x1b'));
        let document: Value = serde_json::from_slice(&output.stdout).expect("build error JSON");
        assert_eq!(document["schema_version"], 1);
        assert_eq!(document["status"], "error");
        assert_eq!(document["error"]["code"], "command-failed");
        assert_eq!(document["error"]["stage"], "import-sources");
        assert!(document["error"].get("project").is_none());
        json_errors.push(document);
    }
    assert_eq!(json_errors[0], json_errors[1]);
}

#[test]
/// Reject mismatched versions before creating an output directory or artifact.
fn exact_platform_version_is_required() {
    let fixture = TestDir::new();
    let ibcmd = fake_ibcmd(&fixture);
    let root = project(&fixture, "configuration", "Versioned");
    let config = root.join("eska.toml");
    let value = fs::read_to_string(&config).expect("config");
    fs::write(&config, value.replace("8.3.27.2325", "8.3.26.1540")).expect("configure version");

    let output = eska(&root, "en", &ibcmd, &["build", "--format", "json"], false);
    assert_eq!(output.status.code(), Some(1));
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("8.3.27.2325"), "{error}");
    assert!(error.contains("8.3.26.1540"), "{error}");
    let document: Value = serde_json::from_slice(&output.stdout).expect("tool error JSON");
    assert_eq!(document["error"]["code"], "ibcmd-version-mismatch");
    assert_eq!(document["error"]["stage"], "tool-discovery");
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
    for (locale, help_text, started_text, result_text, manifest_text) in [
        (
            "ru",
            "Собрать нативный артефакт",
            "Начало сборки платформой 1С",
            "Собран",
            "Паспорт:",
        ),
        (
            "en",
            "Build a native 1C artifact",
            "Starting build with 1C platform",
            "Built",
            "Manifest:",
        ),
    ] {
        let help = Command::new(env!("CARGO_BIN_EXE_eska"))
            .current_dir(&root)
            .args(["--lang", locale, "build", "--help"])
            .output()
            .expect("build help");
        assert!(help.status.success());
        assert!(String::from_utf8_lossy(&help.stdout).contains(help_text));
        assert!(
            String::from_utf8_lossy(&help.stdout).contains(if locale == "ru" {
                "Показать полностью проверенный план сборки"
            } else {
                "Show the fully preflighted build plan"
            })
        );
        assert!(
            String::from_utf8_lossy(&help.stdout).contains(if locale == "ru" {
                "Собрать артефакт из зафиксированного снимка"
            } else {
                "Build from a fixed source snapshot"
            })
        );

        let output = eska(&root, locale, &ibcmd, &["build"], false);
        assert!(output.status.success(), "{output:?}");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stdout.contains(&format!("✓ {result_text}")), "{stdout}");
        assert!(!stdout.contains("8.3.27.2325"), "{stdout}");
        assert!(!stdout.contains('\x1b'), "{stdout:?}");
        assert!(stderr.contains(started_text), "{stderr}");
        assert!(stderr.contains("[WARN] fake build warning"), "{stderr}");
        assert!(!stderr.contains('\x1b'), "{stderr:?}");

        let manifested = eska(&root, locale, &ibcmd, &["build", "--manifest"], false);
        assert!(manifested.status.success(), "{manifested:?}");
        let stdout = String::from_utf8_lossy(&manifested.stdout);
        assert!(stdout.contains(manifest_text), "{stdout}");
        assert!(stdout.contains("Localized.cfe.manifest.json"), "{stdout}");
        assert!(!stdout.contains('\x1b'), "{stdout:?}");
    }
}

#[test]
/// Build a workspace in manifest order and keep single-member JSON backward compatible.
fn workspace_build_uses_shared_outputs_and_distinct_json_shapes() {
    let fixture = workspace();
    let ibcmd = fake_ibcmd(&fixture);
    let output = eska(
        &fixture.0,
        "en",
        &ibcmd,
        &["build", "--format", "json"],
        false,
    );
    assert!(output.status.success(), "{output:?}");
    let document: Value = serde_json::from_slice(&output.stdout).expect("workspace JSON");
    assert_eq!(document["schema_version"], 1);
    assert_eq!(document["projects"][0]["name"], "sales-report");
    assert_eq!(document["projects"][0]["status"], "success");
    assert_eq!(document["projects"][0]["artifact"]["type"], "report");
    assert_eq!(document["projects"][1]["name"], "import-orders");
    assert_eq!(document["projects"][1]["status"], "success");
    assert_eq!(document["projects"][1]["artifact"]["type"], "processing");
    assert_eq!(
        fs::read(fixture.0.join("build/sales-report.erf")).expect("report artifact"),
        b"native-artifact"
    );
    assert_eq!(
        fs::read(fixture.0.join("build/import-orders.epf")).expect("processing artifact"),
        b"native-artifact"
    );

    let member = fixture.0.join("src/sales-report");
    let output = eska(&member, "en", &ibcmd, &["build", "--format", "json"], false);
    assert!(output.status.success(), "{output:?}");
    let document: Value = serde_json::from_slice(&output.stdout).expect("single JSON");
    assert_eq!(document["schema_version"], 1);
    assert_eq!(document["artifact"]["type"], "report");
    assert!(document.get("projects").is_none());
}

#[test]
/// Honor named, repeated and whole-workspace selectors from root and member directories.
fn workspace_build_selectors_choose_the_requested_members() {
    let fixture = workspace();
    let ibcmd = fake_ibcmd(&fixture);
    let named = eska(
        &fixture.0,
        "en",
        &ibcmd,
        &["build", "-p", "import-orders", "--format", "json"],
        false,
    );
    assert!(named.status.success(), "{named:?}");
    let document: Value = serde_json::from_slice(&named.stdout).expect("named JSON");
    assert_eq!(document["artifact"]["type"], "processing");
    assert!(document.get("projects").is_none());
    assert!(!fixture.0.join("build/sales-report.erf").exists());

    let repeated = eska(
        &fixture.0,
        "en",
        &ibcmd,
        &[
            "build",
            "-p",
            "import-orders",
            "-p",
            "sales-report",
            "--format",
            "json",
        ],
        false,
    );
    assert!(repeated.status.success(), "{repeated:?}");
    let document: Value = serde_json::from_slice(&repeated.stdout).expect("repeated JSON");
    assert_eq!(document["projects"][0]["name"], "import-orders");
    assert_eq!(document["projects"][1]["name"], "sales-report");

    let from_member = eska(
        &fixture.0.join("src/import-orders"),
        "en",
        &ibcmd,
        &["build", "--workspace", "--format", "json"],
        false,
    );
    assert!(from_member.status.success(), "{from_member:?}");
    let document: Value = serde_json::from_slice(&from_member.stdout).expect("workspace JSON");
    assert_eq!(document["projects"].as_array().map(Vec::len), Some(2));
}

#[test]
/// Resolve every required platform before the first member build stage starts.
fn workspace_platform_mismatch_blocks_all_build_stages() {
    let fixture = workspace();
    let ibcmd = fake_ibcmd(&fixture);
    let manifest = fixture.0.join("src/import-orders/eska.toml");
    let config = fs::read_to_string(&manifest).expect("member config");
    fs::write(
        &manifest,
        format!("{config}\n[build]\nplatform_version = '8.3.26.1540'\n"),
    )
    .expect("override member platform");
    let log = fixture.0.join("ibcmd.log");
    let output = Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(&fixture.0)
        .env("FAKE_IBCMD_LOG", &log)
        .args(["--lang", "en", "build", "--ibcmd"])
        .arg(&ibcmd)
        .output()
        .expect("run workspace build");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let invocations = fs::read_to_string(log).expect("ibcmd log");
    assert_eq!(
        invocations.lines().collect::<Vec<_>>(),
        ["--version", "--version"]
    );
    assert!(!fixture.0.join("build").exists());
}

#[test]
/// Reject every group during preflight when one member source is invalid.
fn workspace_preflight_blocks_all_ibcmd_invocations() {
    let fixture = workspace();
    let ibcmd = fake_ibcmd(&fixture);
    fs::write(
        fixture.0.join("src/sales-report/SalesReport.xml"),
        "<not-designer-xml/>",
    )
    .expect("break report descriptor");
    let log = fixture.0.join("ibcmd.log");
    let output = Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(&fixture.0)
        .env("FAKE_IBCMD_LOG", &log)
        .args(["--lang", "en", "build", "--format", "json", "--ibcmd"])
        .arg(&ibcmd)
        .output()
        .expect("run workspace build");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("sales-report"),
        "{output:?}"
    );
    let document: Value = serde_json::from_slice(&output.stdout).expect("preflight error JSON");
    assert_eq!(document["error"]["code"], "descriptor-missing");
    assert_eq!(document["error"]["stage"], "preflight");
    assert_eq!(document["error"]["project"], "sales-report");
    assert!(!log.exists(), "ibcmd ran before group preflight completed");
    assert!(!fixture.0.join("build").exists());

    let preview = Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(&fixture.0)
        .env("FAKE_IBCMD_LOG", &log)
        .args([
            "--lang",
            "en",
            "build",
            "--dry-run",
            "--format",
            "json",
            "--ibcmd",
        ])
        .arg(&ibcmd)
        .output()
        .expect("preview invalid workspace");
    assert_eq!(preview.status.code(), Some(1), "{preview:?}");
    let preview_document: Value =
        serde_json::from_slice(&preview.stdout).expect("preview preflight error JSON");
    assert_eq!(preview_document, document);
    assert!(!log.exists(), "ibcmd ran after failed preview preflight");
    assert!(!fixture.0.join("build").exists());
}

#[test]
/// Continue later members after a runtime failure and report stable aggregate error data.
fn workspace_runtime_failure_does_not_stop_later_members() {
    let fixture = workspace();
    let ibcmd = fake_ibcmd(&fixture);
    let output = Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(&fixture.0)
        .env("FAKE_IBCMD_FAIL_IMPORT", "0")
        .env("FAKE_IBCMD_FAIL_SOURCE_CONTAINS", "SalesReport")
        .args(["--lang", "en", "build", "--format", "json", "--ibcmd"])
        .arg(&ibcmd)
        .output()
        .expect("run workspace build");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let document: Value = serde_json::from_slice(&output.stdout).expect("aggregate JSON");
    assert_eq!(document["projects"][0]["status"], "failed");
    assert_eq!(document["projects"][0]["error"]["code"], "command-failed");
    assert_eq!(document["projects"][0]["error"]["stage"], "import-sources");
    assert!(document["projects"][0]["error"].get("project").is_none());
    assert_eq!(document["projects"][1]["status"], "success");
    assert!(!fixture.0.join("build/sales-report.erf").exists());
    assert_eq!(
        fs::read(fixture.0.join("build/import-orders.epf")).expect("later artifact"),
        b"native-artifact"
    );
}

#[test]
/// Require exactly one selected member before accepting a custom output.
fn workspace_output_requires_one_member_before_ibcmd_discovery() {
    let fixture = workspace();
    let ibcmd = fake_ibcmd(&fixture);
    let log = fixture.0.join("ibcmd.log");
    let output = Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(&fixture.0)
        .env("FAKE_IBCMD_LOG", &log)
        .args([
            "--lang",
            "en",
            "build",
            "--output",
            "custom.erf",
            "--format",
            "json",
            "--ibcmd",
        ])
        .arg(&ibcmd)
        .output()
        .expect("run workspace build");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("exactly one selected project"),
        "{output:?}"
    );
    let document: Value = serde_json::from_slice(&output.stdout).expect("selection error JSON");
    assert_eq!(document["error"]["code"], "output-requires-single-project");
    assert_eq!(document["error"]["stage"], "selection");
    assert!(!log.exists());
}

#[test]
/// Return the same discovery error document in both supported locales.
fn json_discovery_error_is_locale_independent() {
    let fixture = TestDir::new();
    let ibcmd = fake_ibcmd(&fixture);
    let mut errors = Vec::new();
    for locale in ["ru", "en"] {
        let output = eska(
            &fixture.0,
            locale,
            &ibcmd,
            &["build", "--format", "json"],
            false,
        );
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        let document: Value = serde_json::from_slice(&output.stdout).expect("discovery JSON");
        assert_eq!(document["error"]["code"], "project-discovery");
        assert_eq!(document["error"]["stage"], "discovery");
        errors.push(document);
    }
    assert_eq!(errors[0], errors[1]);
}

#[test]
/// Write a locale-independent passport while retaining the existing build JSON contract.
fn manifested_build_records_artifact_snapshot_platform_version_and_absent_git() {
    let fixture = TestDir::new();
    let root = project(&fixture, "configuration", "manifest-demo");
    fs::write(
        root.join("src/Configuration.xml"),
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"><Configuration><Properties><Version>1.2.3.4</Version></Properties></Configuration></MetaDataObject>"#,
    )
    .expect("versioned descriptor");
    let ibcmd = fake_ibcmd(&fixture);
    let output = eska(
        &root,
        "en",
        &ibcmd,
        &["build", "--manifest", "--format", "json"],
        false,
    );
    assert!(output.status.success(), "{output:?}");

    let build: Value = serde_json::from_slice(&output.stdout).expect("build JSON");
    assert_eq!(build["schema_version"], 1);
    assert_eq!(build["artifact"]["type"], "configuration");
    assert!(build.get("manifest").is_none(), "build v1 changed: {build}");

    let manifest_path = root.join("build/manifest-demo.cf.manifest.json");
    let bytes = fs::read(&manifest_path).expect("artifact manifest");
    let manifest: Value = serde_json::from_slice(&bytes).expect("manifest JSON");
    assert_eq!(manifest["schema_version"], 1);
    assert_eq!(manifest["kind"], "artifact-manifest");
    assert_eq!(manifest["project"]["name"], "manifest-demo");
    assert_eq!(manifest["project"]["version"]["status"], "available");
    assert_eq!(manifest["project"]["version"]["value"], "1.2.3.4");
    assert_eq!(manifest["artifact"]["type"], "configuration");
    assert_eq!(manifest["artifact"]["checksum"]["algorithm"], "sha256");
    assert_eq!(
        manifest["artifact"]["checksum"]["value"],
        "5e994f33c0e7a8698cd9aa625e8de021fdedcf06cb9bcd174500651882d70f50"
    );
    assert_eq!(manifest["platform"]["version"], "8.3.27.2325");
    assert_eq!(manifest["source"]["git"]["status"], "absent");
    assert_eq!(manifest["source"]["git"]["commit"], Value::Null);
    assert_eq!(manifest["source"]["git"]["dirty"], Value::Null);
    assert!(
        manifest["source"]["snapshot_id"]
            .as_str()
            .is_some_and(|id| id.starts_with("sha256:") && id.len() == 71)
    );
    assert!(
        !String::from_utf8_lossy(&bytes).contains(&root.to_string_lossy().into_owned()),
        "manifest leaked an absolute project path"
    );
    let russian = eska(
        &root,
        "ru",
        &ibcmd,
        &["build", "--manifest", "--format", "json"],
        false,
    );
    assert!(russian.status.success(), "{russian:?}");
    assert_eq!(fs::read(manifest_path).expect("Russian manifest"), bytes);
}

#[test]
/// Build from copied bytes even when the original source changes during ibcmd execution.
fn manifested_build_uses_the_captured_source_snapshot() {
    let fixture = TestDir::new();
    let root = project(&fixture, "processing", "snapshot-demo");
    let ibcmd = fake_ibcmd(&fixture);
    let payload = root.join("src/payload.txt");
    fs::write(&payload, b"captured bytes").expect("initial payload");
    let ready = fixture.0.join("import.ready");
    let proceed = fixture.0.join("import.continue");
    let mut child = Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(&root)
        .env("FAKE_IBCMD_IMPORT_READY", &ready)
        .env("FAKE_IBCMD_IMPORT_CONTINUE", &proceed)
        .env("FAKE_IBCMD_ARTIFACT_SOURCE_FILE", "payload.txt")
        .args(["--lang", "en", "build", "--manifest", "--ibcmd"])
        .arg(&ibcmd)
        .spawn()
        .expect("start manifested build");
    for _ in 0..500 {
        if ready.exists() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(ready.exists(), "fake ibcmd did not reach import");
    fs::write(&payload, b"changed during build").expect("change original source");
    fs::write(&proceed, b"").expect("continue import");
    assert!(child.wait().expect("wait for build").success());
    assert_eq!(
        fs::read(root.join("build/snapshot-demo.epf")).expect("artifact"),
        b"captured bytes"
    );
    let first: Value = serde_json::from_slice(
        &fs::read(root.join("build/snapshot-demo.epf.manifest.json")).expect("manifest"),
    )
    .expect("manifest JSON");
    assert_eq!(first["project"]["version"]["status"], "unavailable");

    let second = Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(&root)
        .env("FAKE_IBCMD_ARTIFACT_SOURCE_FILE", "payload.txt")
        .args(["--lang", "en", "build", "--manifest", "--ibcmd"])
        .arg(&ibcmd)
        .output()
        .expect("repeat manifested build");
    assert!(second.status.success(), "{second:?}");
    assert_eq!(
        fs::read(root.join("build/snapshot-demo.epf")).expect("updated artifact"),
        b"changed during build"
    );
    let second: Value = serde_json::from_slice(
        &fs::read(root.join("build/snapshot-demo.epf.manifest.json")).expect("updated manifest"),
    )
    .expect("updated manifest JSON");
    assert_ne!(
        first["source"]["snapshot_id"],
        second["source"]["snapshot_id"]
    );
}

#[test]
/// Exclude in-source result files and the temporary workspace from the snapshot identity.
fn manifested_build_excludes_its_own_results_from_the_source_snapshot() {
    let fixture = TestDir::new();
    let root = project(&fixture, "configuration", "in-source-demo");
    let ibcmd = fake_ibcmd(&fixture);
    let args = ["build", "--manifest", "--output", "src/result.cf"];
    let first = eska(&root, "en", &ibcmd, &args, false);
    assert!(first.status.success(), "{first:?}");
    let manifest_path = root.join("src/result.cf.manifest.json");
    let first: Value = serde_json::from_slice(&fs::read(&manifest_path).expect("manifest"))
        .expect("manifest JSON");
    let second = eska(&root, "en", &ibcmd, &args, false);
    assert!(second.status.success(), "{second:?}");
    let second: Value =
        serde_json::from_slice(&fs::read(manifest_path).expect("manifest")).expect("manifest JSON");
    assert_eq!(
        first["source"]["snapshot_id"],
        second["source"]["snapshot_id"]
    );
    assert!(
        fs::read_dir(root.join("src"))
            .expect("source directory")
            .all(|entry| !entry
                .expect("source entry")
                .file_name()
                .to_string_lossy()
                .contains("eska-work"))
    );
}

#[test]
/// Record one stable Git commit and distinguish clean from dirty worktrees.
fn manifested_build_records_clean_and_dirty_git_state() {
    let fixture = TestDir::new();
    let root = project(&fixture, "configuration", "git-demo");
    fs::write(
        root.join("src/Configuration.xml"),
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"><Configuration><Properties><Version>1.0.0.1</Version></Properties></Configuration></MetaDataObject>"#,
    )
    .expect("descriptor");
    let git = |arguments: &[&str]| {
        Command::new("git")
            .current_dir(&root)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .args(arguments)
            .output()
            .expect("run git")
    };
    assert!(git(&["init", "-q"]).status.success());
    assert!(git(&["add", "."]).status.success());
    assert!(git(&["commit", "-qm", "fixture"]).status.success());
    let commit = String::from_utf8(git(&["rev-parse", "HEAD"]).stdout)
        .expect("commit UTF-8")
        .trim()
        .to_owned();
    let ibcmd = fake_ibcmd(&fixture);

    let clean = eska(&root, "en", &ibcmd, &["build", "--manifest"], false);
    assert!(clean.status.success(), "{clean:?}");
    let manifest_path = root.join("build/git-demo.cf.manifest.json");
    let clean: Value = serde_json::from_slice(&fs::read(&manifest_path).expect("clean manifest"))
        .expect("clean JSON");
    assert_eq!(clean["source"]["git"]["status"], "available");
    assert_eq!(clean["source"]["git"]["commit"], commit);
    assert_eq!(clean["source"]["git"]["dirty"], false);

    fs::write(root.join("src/dirty.txt"), b"dirty").expect("dirty source");
    let dirty = eska(&root, "en", &ibcmd, &["build", "--manifest"], false);
    assert!(dirty.status.success(), "{dirty:?}");
    let dirty: Value = serde_json::from_slice(&fs::read(manifest_path).expect("dirty manifest"))
        .expect("dirty JSON");
    assert_eq!(dirty["source"]["git"]["commit"], commit);
    assert_eq!(dirty["source"]["git"]["dirty"], true);
}

#[test]
/// Reject links from a manifested snapshot while retaining ordinary build behavior.
fn manifested_build_rejects_source_links_without_changing_ordinary_build() {
    use std::os::unix::fs::symlink;

    let fixture = TestDir::new();
    let root = project(&fixture, "configuration", "linked-demo");
    let ibcmd = fake_ibcmd(&fixture);
    fs::write(root.join("target.txt"), b"outside source").expect("link target");
    symlink("../target.txt", root.join("src/link.txt")).expect("source link");
    let manifested = eska(
        &root,
        "en",
        &ibcmd,
        &["build", "--manifest", "--format", "json"],
        false,
    );
    assert_eq!(manifested.status.code(), Some(1), "{manifested:?}");
    let error: Value = serde_json::from_slice(&manifested.stdout).expect("error JSON");
    assert_eq!(error["error"]["code"], "snapshot-entry-unsupported");
    assert_eq!(error["error"]["stage"], "snapshot");
    assert!(!root.join("build/linked-demo.cf").exists());

    let ordinary = eska(&root, "en", &ibcmd, &["build"], false);
    assert!(ordinary.status.success(), "{ordinary:?}");
}

#[test]
/// Publish successful workspace pairs and preserve the failed member's previous pair.
fn workspace_manifest_keeps_each_member_transaction_independent() {
    let fixture = workspace();
    let ibcmd = fake_ibcmd(&fixture);
    fs::create_dir(fixture.0.join("build")).expect("build directory");
    fs::write(fixture.0.join("build/import-orders.epf"), b"old artifact").expect("old artifact");
    fs::write(
        fixture.0.join("build/import-orders.epf.manifest.json"),
        b"old manifest",
    )
    .expect("old manifest");
    let output = Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(&fixture.0)
        .env("FAKE_IBCMD_FAIL_IMPORT", "0")
        .env("FAKE_IBCMD_FAIL_SOURCE_CONTAINS", "ImportOrders")
        .args([
            "--lang",
            "en",
            "build",
            "--manifest",
            "--format",
            "json",
            "--ibcmd",
        ])
        .arg(&ibcmd)
        .output()
        .expect("workspace manifested build");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let document: Value = serde_json::from_slice(&output.stdout).expect("workspace JSON");
    assert_eq!(document["projects"][0]["status"], "success");
    assert_eq!(document["projects"][1]["status"], "failed");
    let first_manifest: Value = serde_json::from_slice(
        &fs::read(fixture.0.join("build/sales-report.erf.manifest.json")).expect("first manifest"),
    )
    .expect("first manifest JSON");
    assert_eq!(first_manifest["project"]["name"], "sales-report");
    assert_eq!(
        fs::read(fixture.0.join("build/import-orders.epf")).expect("preserved artifact"),
        b"old artifact"
    );
    assert_eq!(
        fs::read(fixture.0.join("build/import-orders.epf.manifest.json"))
            .expect("preserved manifest"),
        b"old manifest"
    );
}
