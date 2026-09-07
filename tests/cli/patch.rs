#![cfg(unix)]

use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, process::Command};

use serde_json::Value;

use crate::{
    support::TestDir,
    vcs::support::{git, repository},
};

/// Create a committed eligible change for dry-run CLI contract checks.
fn project() -> (TestDir, PathBuf) {
    let fixture = repository();
    let root = fixture.0.join("project");
    fs::create_dir_all(root.join("src/CommonModules/Math/Ext")).expect("create source tree");
    fs::write(
        root.join("eska.toml"),
        "[project]\ntype = 'configuration'\n[build]\nplatform_version = '8.3.27.2325'\n[vcs.workflow]\npreset = 'trunk'\n",
    )
    .expect("write project configuration");
    fs::write(root.join("src/Configuration.xml"), "<Configuration/>")
        .expect("write configuration descriptor");
    fs::write(
        root.join("src/CommonModules/Math.xml"),
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"><CommonModule uuid="11111111-1111-1111-1111-111111111111"><Properties><Name>Math</Name><Global>false</Global><Server>true</Server><ClientManagedApplication>false</ClientManagedApplication><ClientOrdinaryApplication>false</ClientOrdinaryApplication><Privileged>false</Privileged><ReturnValuesReuse>DontUse</ReturnValuesReuse></Properties></CommonModule></MetaDataObject>"#,
    )
    .expect("write module descriptor");
    let module = root.join("src/CommonModules/Math/Ext/Module.bsl");
    fs::write(
        &module,
        "Function AddOne(Value) Export\n Return Value + 1;\nEndFunction\n",
    )
    .expect("write base module");
    git(&fixture.0, &["add", "--", "project"]);
    git(&fixture.0, &["commit", "-m", "base"]);
    git(&fixture.0, &["switch", "-c", "feature"]);
    fs::write(
        module,
        "Function AddOne(Value) Export\n Return Value + 2;\nEndFunction\n",
    )
    .expect("change method");
    git(&fixture.0, &["add", "--all"]);
    git(&fixture.0, &["commit", "-m", "feature"]);
    (fixture, root)
}

/// Run a localized patch dry-run and parse its machine result.
fn dry_run(root: &std::path::Path, locale: &str) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(root)
        .args(["--lang", locale, "patch", "--dry-run", "--format", "json"])
        .output()
        .expect("run patch dry-run");
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    serde_json::from_slice(&output.stdout).expect("patch JSON")
}

/// Install a fake paired ibcmd and Designer that exercises every pipeline transition.
fn fake_platform(fixture: &TestDir) -> PathBuf {
    let ibcmd = fixture.0.join("ibcmd");
    fs::write(
        &ibcmd,
        r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "1C ibcmd version 8.3.27.2325"
  exit 0
fi
last=
for argument in "$@"; do
  last="$argument"
  case "$argument" in --data=*) data="${argument#--data=}";; esac
done
if [ "$1 $2" = "infobase create" ]; then
  mkdir -p "$data/db-data"
elif [ "$1 $2" = "config export" ]; then
  mkdir -p "$last"
  printf '%s' '<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"><Configuration><ChildObjects></ChildObjects></Configuration></MetaDataObject>' > "$last/Configuration.xml"
elif [ "$1 $2" = "config save" ]; then
  printf '%s' 'extension-artifact' > "$last"
fi
exit 0
"#,
    )
    .expect("write fake ibcmd");
    let designer = fixture.0.join("1cv8");
    fs::write(
        &designer,
        r#"#!/bin/sh
previous=
for argument in "$@"; do
  if [ "$previous" = "/DumpResult" ]; then result="$argument"; fi
  if [ "$previous" = "/Out" ]; then log="$argument"; fi
  previous="$argument"
done
printf '0' > "$result"
printf 'validated' > "$log"
exit 0
"#,
    )
    .expect("write fake Designer");
    for path in [&ibcmd, &designer] {
        let mut permissions = fs::metadata(path).expect("fake metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("make fake executable");
    }
    ibcmd
}

/// Keep the versioned JSON plan identical across supported UI locales.
#[test]
fn dry_run_json_is_stable_and_does_not_create_artifacts() {
    let (_fixture, root) = project();
    let ru = dry_run(&root, "ru");
    let en = dry_run(&root, "en");

    assert_eq!(ru, en);
    assert_eq!(ru["schema_version"], 1);
    assert_eq!(ru["status"], "planned");
    assert_eq!(ru["plan"]["base"], "refs/heads/main");
    assert_eq!(ru["plan"]["modules"][0]["name"], "Math");
    assert_eq!(ru["plan"]["modules"][0]["methods"][0], "AddOne");
    assert_eq!(ru["runtime_verified"], false);
    assert_eq!(ru["requires_safe_mode_disabled"], true);
    assert!(!root.join("build").exists());
}

/// Return a stable machine error code while localizing the human explanation.
#[test]
fn dirty_error_code_is_locale_independent() {
    let (_fixture, root) = project();
    fs::write(root.join("uncommitted.txt"), "dirty\n").expect("dirty worktree");

    for (locale, expected) in [
        ("ru", "Рабочая копия должна быть чистой"),
        ("en", "Worktree must be clean"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_eska"))
            .current_dir(&root)
            .args(["--lang", locale, "patch", "--dry-run", "--format", "json"])
            .output()
            .expect("run dirty patch");
        assert!(!output.status.success(), "{output:?}");
        let json: Value = serde_json::from_slice(&output.stdout).expect("error JSON");
        assert_eq!(json["error"]["code"], "dirty");
        assert!(String::from_utf8_lossy(&output.stderr).contains(expected));
    }
}

/// Publish only the fully validated candidate and refuse to replace it on a repeated run.
#[test]
fn build_pipeline_publishes_once_without_leaving_workspace() {
    let (_fixture, root) = project();
    let platform = TestDir::new();
    let ibcmd = fake_platform(&platform);
    let output_path = platform.0.join("result.cfe");
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_eska"))
            .current_dir(&root)
            .args(["--lang", "en", "patch", "--format", "json", "--ibcmd"])
            .arg(&ibcmd)
            .arg("--output")
            .arg(&output_path)
            .output()
            .expect("build patch")
    };

    let first = run();
    assert!(first.status.success(), "{first:?}");
    let json: Value = serde_json::from_slice(&first.stdout).expect("build JSON");
    assert_eq!(json["status"], "built");
    assert_eq!(json["validation"], "metadata_bsl_applicability");
    assert_eq!(
        fs::read(&output_path).expect("artifact"),
        b"extension-artifact"
    );
    assert!(fs::read_dir(&root).expect("project entries").all(|entry| {
        !entry
            .expect("project entry")
            .file_name()
            .to_string_lossy()
            .starts_with(".eska-patch-")
    }));

    let second = run();
    assert!(!second.status.success(), "{second:?}");
    let json: Value = serde_json::from_slice(&second.stdout).expect("error JSON");
    assert_eq!(json["error"]["code"], "output");
    assert_eq!(
        fs::read(output_path).expect("unchanged artifact"),
        b"extension-artifact"
    );
}
