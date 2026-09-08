use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use serde_json::Value;

use crate::support::TestDir;

/// Run doctor with isolated machine and Git configuration.
fn doctor(root: &Path, locale: &str, arguments: &[&str], path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(root)
        .env("PATH", path)
        .env("ESKA_CONFIG_DIR", root.join("machine-config"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", root.join("absent-git-config"))
        .args(["--lang", locale, "doctor"])
        .args(arguments)
        .output()
        .expect("run doctor")
}

/// Create the untouched configuration scaffold represented by `eska new`.
fn scaffold() -> (TestDir, PathBuf) {
    let fixture = TestDir::new();
    let root = fixture.0.join("project");
    fs::create_dir_all(root.join("src")).expect("source directory");
    fs::write(
        root.join("eska.toml"),
        concat!(
            "[project]\n",
            "type = 'configuration'\n",
            "source = 'src'\n\n",
            "[build]\n",
            "platform_version = ''\n\n",
            "[vcs.workflow]\n",
            "preset = 'trunk'\n",
        ),
    )
    .expect("project config");
    fs::write(root.join("src/.gitkeep"), "").expect("source marker");
    fs::write(root.join(".gitattributes"), "*.bin filter=lfs\n").expect("attributes");
    (fixture, root)
}

/// Write an executable fake tool used through Rust `Command` without a shell pipeline.
#[cfg(unix)]
fn executable(path: &Path, contents: &str) {
    use std::os::unix::fs::PermissionsExt;

    fs::write(path, contents).expect("fake tool");
    let mut permissions = fs::metadata(path).expect("tool metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("tool permissions");
}

/// Replace an empty scaffold with a build-ready configuration descriptor.
fn configure_project(root: &Path, version: &str) {
    fs::write(
        root.join("eska.toml"),
        format!(
            "[project]\ntype = 'configuration'\nsource = 'src'\n\n[build]\nplatform_version = '{version}'\n\n[vcs.workflow]\npreset = 'trunk'\n"
        ),
    )
    .expect("configured project");
    fs::write(
        root.join("src/Configuration.xml"),
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"><Configuration/></MetaDataObject>"#,
    )
    .expect("root descriptor");
}

#[test]
/// Expose localized help and every read-only selection option.
fn help_is_localized_and_lists_read_only_selection_options() {
    let fixture = TestDir::new();
    for (locale, expected) in [
        ("en", "Diagnose the current project environment"),
        ("ru", "Диагностировать окружение текущего проекта"),
    ] {
        let output = doctor(&fixture.0, locale, &["--help"], Path::new(""));
        assert!(output.status.success(), "{output:?}");
        let stdout = String::from_utf8(output.stdout).expect("UTF-8 help");
        assert!(stdout.contains(expected), "{stdout}");
        assert!(stdout.contains("--project <"), "{stdout}");
        assert!(stdout.contains("--workspace"), "{stdout}");
        assert!(stdout.contains("--format <"), "{stdout}");
    }
}

#[test]
/// Keep JSON locale-independent and explain checks skipped by an empty scaffold.
fn scaffold_json_is_stable_across_locales_and_explains_skipped_checks() {
    let (fixture, root) = scaffold();
    let empty_path = fixture.0.join("empty-path");
    fs::create_dir(&empty_path).expect("empty PATH");
    let mut documents = Vec::new();
    for locale in ["en", "ru"] {
        let output = doctor(&root, locale, &["--format", "json"], &empty_path);
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        documents.push(serde_json::from_slice::<Value>(&output.stdout).expect("doctor JSON"));
    }
    assert_eq!(documents[0], documents[1]);
    let document = &documents[0];
    assert_eq!(document["schema_version"], 1);
    assert_eq!(document["status"], "error");
    assert_eq!(document["scope"]["kind"], "project");
    assert!(document["checks"].as_array().is_some_and(|checks| {
        checks.iter().any(|check| {
            check["id"] == "project.descriptor"
                && check["status"] == "warning"
                && check["code"] == "scaffold"
        }) && checks.iter().any(|check| {
            check["id"] == "build.ibcmd"
                && check["status"] == "skipped"
                && check["code"] == "platform-version-required"
        }) && checks
            .iter()
            .any(|check| check["id"] == "vcs.git" && check["status"] == "fail")
    }));
}

#[test]
/// Continue machine diagnosis when the project manifest cannot be parsed.
fn invalid_manifest_still_reports_machine_config_as_an_independent_check() {
    let fixture = TestDir::new();
    fs::write(fixture.0.join("eska.toml"), "broken = [").expect("invalid config");
    let output = doctor(&fixture.0, "en", &["--format", "json"], Path::new(""));
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let document: Value = serde_json::from_slice(&output.stdout).expect("doctor JSON");
    let checks = document["checks"].as_array().expect("checks");
    assert_eq!(checks[0]["id"], "machine.config");
    assert_eq!(checks[0]["status"], "pass");
    assert_eq!(checks[1]["id"], "project.config");
    assert_eq!(checks[1]["code"], "invalid");
    assert!(checks.iter().any(|check| check["id"] == "vcs.git"));
    assert!(checks.iter().any(|check| check["id"] == "vcs.author"));
    assert!(checks.iter().any(|check| check["id"] == "vcs.git-lfs"));
}

#[test]
/// Limit member checks to an explicit selector and identify aggregate workspace runs.
fn workspace_selector_limits_project_checks_and_reports_its_name() {
    let fixture = TestDir::new();
    let report = fixture.0.join("src/report");
    let processing = fixture.0.join("src/processing");
    fs::create_dir_all(&report).expect("report root");
    fs::create_dir_all(&processing).expect("processing root");
    fs::write(
        fixture.0.join("eska.toml"),
        concat!(
            "[workspace]\n",
            "members = ['src/report', 'src/processing']\n\n",
            "[vcs.workflow]\n",
            "preset = 'trunk'\n",
        ),
    )
    .expect("workspace config");
    fs::write(
        report.join("eska.toml"),
        "[project]\nname = 'report'\ntype = 'report'\nsource = '.'\n",
    )
    .expect("report config");
    fs::write(
        processing.join("eska.toml"),
        "[project]\nname = 'processing'\ntype = 'processing'\nsource = '.'\n",
    )
    .expect("processing config");
    fs::write(
        report.join("Report.xml"),
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"><ExternalReport/></MetaDataObject>"#,
    )
    .expect("report descriptor");
    fs::write(
        processing.join("Processing.xml"),
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"><ExternalDataProcessor/></MetaDataObject>"#,
    )
    .expect("processing descriptor");

    let output = doctor(
        &fixture.0,
        "en",
        &["--project", "report", "--format", "json"],
        Path::new(""),
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let document: Value = serde_json::from_slice(&output.stdout).expect("doctor JSON");
    assert_eq!(document["scope"]["kind"], "project");
    assert_eq!(document["scope"]["projects"], serde_json::json!(["report"]));
    for check in document["checks"].as_array().expect("checks") {
        if let Some(project) = check.get("project") {
            assert_eq!(project, "report");
        }
    }

    let output = doctor(
        &fixture.0,
        "ru",
        &["--workspace", "--format", "json"],
        Path::new(""),
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let document: Value = serde_json::from_slice(&output.stdout).expect("workspace JSON");
    assert_eq!(document["scope"]["kind"], "workspace");
    assert_eq!(
        document["scope"]["projects"],
        serde_json::json!(["report", "processing"])
    );
}

#[test]
/// Continue project and repository diagnosis after an invalid machine config.
fn invalid_machine_config_does_not_hide_project_and_vcs_checks() {
    let (_fixture, root) = scaffold();
    fs::create_dir_all(root.join("machine-config")).expect("machine config directory");
    fs::write(
        root.join("machine-config/config.toml"),
        "[build]\nrunner = 'host'\ncontainer = 'unexpected'\n",
    )
    .expect("invalid machine config");
    let output = doctor(&root, "en", &["--format", "json"], Path::new(""));
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let document: Value = serde_json::from_slice(&output.stdout).expect("doctor JSON");
    let checks = document["checks"].as_array().expect("checks");
    assert!(checks.iter().any(|check| {
        check["id"] == "machine.config" && check["status"] == "fail" && check["code"] == "invalid"
    }));
    assert!(
        checks
            .iter()
            .any(|check| { check["id"] == "project.descriptor" && check["status"] == "warning" })
    );
    assert!(checks.iter().any(|check| check["id"] == "vcs.git"));
}

#[test]
/// Treat missing descriptors in populated sources as failures in both locales.
fn nonempty_sources_without_a_descriptor_are_a_failure_in_both_languages() {
    let (fixture, root) = scaffold();
    fs::write(
        root.join("src/module.bsl"),
        "Процедура Тест()\nКонецПроцедуры\n",
    )
    .expect("source file");
    let empty_path = fixture.0.join("empty-path");
    fs::create_dir(&empty_path).expect("empty PATH");
    for (locale, expected) in [("en", "Root descriptor"), ("ru", "Корневой дескриптор")]
    {
        let output = doctor(&root, locale, &[], &empty_path);
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        let stdout = String::from_utf8(output.stdout).expect("UTF-8 report");
        assert!(stdout.contains(expected), "{stdout}");
        assert!(stdout.contains('✗'), "{stdout}");
        assert!(!stdout.contains("doctor-"), "{stdout}");
    }
}

#[test]
/// Report malformed or type-incompatible root XML with a stable diagnostic code.
fn invalid_root_descriptor_is_reported_without_stopping_other_checks() {
    let (fixture, root) = scaffold();
    fs::write(root.join("src/Configuration.xml"), "<MetaDataObject>").expect("invalid XML");
    let empty_path = fixture.0.join("empty-path");
    fs::create_dir(&empty_path).expect("empty PATH");

    let output = doctor(&root, "en", &["--format", "json"], &empty_path);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let document: Value = serde_json::from_slice(&output.stdout).expect("doctor JSON");
    let checks = document["checks"].as_array().expect("checks");
    assert!(checks.iter().any(|check| {
        check["id"] == "project.descriptor"
            && check["status"] == "fail"
            && check["code"] == "invalid"
    }));
    assert!(checks.iter().any(|check| check["id"] == "vcs.workflow"));
}

#[cfg(unix)]
#[test]
/// Report the discovered incompatible platform version and keep its expected version.
fn incompatible_ibcmd_version_has_stable_expected_and_actual_values() {
    let (fixture, root) = scaffold();
    configure_project(&root, "8.3.27.2325");
    let tools = fixture.0.join("tools");
    fs::create_dir(&tools).expect("tools directory");
    let ibcmd = tools.join("ibcmd");
    executable(&ibcmd, "#!/bin/sh\necho 8.3.26.1498\n");

    let output = doctor(
        &root,
        "ru",
        &[
            "--ibcmd",
            ibcmd.to_str().expect("UTF-8 tool path"),
            "--format",
            "json",
        ],
        &tools,
    );

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let document: Value = serde_json::from_slice(&output.stdout).expect("doctor JSON");
    assert!(document["checks"].as_array().is_some_and(|checks| {
        checks.iter().any(|check| {
            check["id"] == "build.ibcmd"
                && check["code"] == "version-mismatch"
                && check["expected"] == "8.3.27.2325"
                && check["actual"] == "8.3.26.1498"
                && check["source"] == "host"
        })
    }));
}

#[cfg(unix)]
#[test]
/// Accept a local workflow without a remote and skip Git LFS when attributes do not require it.
fn missing_remote_and_unrequired_git_lfs_are_not_failures() {
    let (fixture, root) = scaffold();
    fs::remove_file(root.join(".gitattributes")).expect("remove LFS attributes");
    run_git(&root, &["init", "--initial-branch=main"]);
    run_git(&root, &["config", "user.name", "Doctor Test"]);
    run_git(&root, &["config", "user.email", "doctor@example.invalid"]);
    let tools = fixture.0.join("tools");
    fs::create_dir(&tools).expect("tools directory");
    executable(
        &tools.join("git"),
        "#!/bin/sh\ncase \"$1 $2\" in\n  '--version ') exit 0 ;;\n  'var GIT_AUTHOR_IDENT') exit 0 ;;\nesac\nexit 1\n",
    );

    let output = doctor(&root, "en", &["--format", "json"], &tools);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let document: Value = serde_json::from_slice(&output.stdout).expect("doctor JSON");
    let checks = document["checks"].as_array().expect("checks");
    assert!(checks.iter().any(|check| {
        check["id"] == "vcs.remote"
            && check["status"] == "pass"
            && check["code"] == "not-configured"
    }));
    assert!(checks.iter().any(|check| {
        check["id"] == "vcs.git-lfs"
            && check["status"] == "skipped"
            && check["code"] == "not-required"
    }));
}

#[cfg(unix)]
#[test]
/// Pass a complete environment without writing project or repository state.
fn complete_environment_passes_and_leaves_sources_index_refs_and_artifacts_unchanged() {
    let (fixture, root) = scaffold();
    configure_project(&root, "8.3.27.2325");
    run_git(&root, &["init", "--initial-branch=main"]);
    run_git(&root, &["config", "user.name", "Doctor Test"]);
    run_git(&root, &["config", "user.email", "doctor@example.invalid"]);
    run_git(
        &root,
        &[
            "remote",
            "add",
            "origin",
            "https://example.invalid/project.git",
        ],
    );
    run_git(&root, &["add", "."]);
    run_git(&root, &["commit", "-m", "base"]);

    let tools = fixture.0.join("tools");
    fs::create_dir(&tools).expect("tools directory");
    executable(
        &tools.join("git"),
        "#!/bin/sh\ncase \"$1 $2\" in\n  '--version ') exit 0 ;;\n  'var GIT_AUTHOR_IDENT') exit 0 ;;\n  'lfs version') exit 0 ;;\nesac\nexit 1\n",
    );
    let ibcmd = tools.join("ibcmd");
    executable(&ibcmd, "#!/bin/sh\necho 8.3.27.2325\n");
    executable(&tools.join("1cv8"), "#!/bin/sh\nexit 0\n");

    let protected = [
        root.join("eska.toml"),
        root.join(".gitattributes"),
        root.join("src/Configuration.xml"),
        root.join(".git/HEAD"),
        root.join(".git/index"),
        root.join(".git/config"),
        root.join(".git/refs/heads/main"),
    ];
    let before = protected
        .iter()
        .map(|path| fs::read(path).expect("protected file"))
        .collect::<Vec<_>>();
    let output = doctor(
        &root,
        "en",
        &[
            "--ibcmd",
            ibcmd.to_str().expect("UTF-8 tool path"),
            "--format",
            "json",
        ],
        &tools,
    );
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    let document: Value = serde_json::from_slice(&output.stdout).expect("doctor JSON");
    assert_eq!(document["status"], "ok");
    assert_eq!(document["summary"]["failed"], 0);
    assert_eq!(document["summary"]["warnings"], 0);
    assert!(document["checks"].as_array().is_some_and(|checks| {
        checks.iter().any(|check| {
            check["id"] == "build.ibcmd"
                && check["status"] == "pass"
                && check["actual"] == "8.3.27.2325"
                && check["source"] == "host"
        }) && checks.iter().any(|check| {
            check["id"] == "vcs.remote"
                && check["status"] == "pass"
                && check["code"] == "configured"
        }) && checks
            .iter()
            .any(|check| check["id"] == "vcs.git-lfs" && check["status"] == "pass")
    }));
    let output = doctor(
        &root,
        "ru",
        &["--ibcmd", ibcmd.to_str().expect("UTF-8 tool path")],
        &tools,
    );
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 report");
    assert!(stdout.contains("8.3.27.2325"), "{stdout}");
    assert!(stdout.contains("runner:"), "{stdout}");
    assert!(stdout.contains("host"), "{stdout}");
    assert!(!stdout.contains("doctor-"), "{stdout}");
    let after = protected
        .iter()
        .map(|path| fs::read(path).expect("protected file after doctor"))
        .collect::<Vec<_>>();
    assert_eq!(after, before);
    assert!(!root.join("build").exists());
}

/// Run Git setup commands with deterministic identity inherited from repository config.
#[cfg(unix)]
fn run_git(root: &Path, arguments: &[&str]) {
    let output = Command::new("git")
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", root.join("absent-git-config"))
        .args(arguments)
        .output()
        .expect("run Git");
    assert!(
        output.status.success(),
        "git {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
