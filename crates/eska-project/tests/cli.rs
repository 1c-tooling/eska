//! Standalone process tests run without a wrapper, Git, a 1C runner or an analyzer on PATH.

use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

use serde_json::Value;

#[path = "../../../tests/support/mod.rs"]
mod support;
use support::TestDir;

/// Protocol failures before and after directory claiming cannot publish incomplete onboarding.
#[cfg(unix)]
#[test]
fn native_onboarding_refuses_invalid_component_replies_and_rolls_back() {
    for locale in ["ru", "en"] {
        for mode in [
            "missing",
            "bridge-schema",
            "bridge-onboarding-identity",
            "bridge-onboarding-failed",
        ] {
            let fixture = TestDir::new();
            let root = fixture.0.join("project");
            fs::create_dir(fixture.0.join(".eska")).unwrap();
            fs::write(fixture.0.join(".eska/keep"), b"user bytes").unwrap();
            let input = fixture.0.join("input.epf");
            let bytes = b"<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\"><ExternalDataProcessor uuid=\"11111111-1111-1111-1111-111111111111\"><Properties><Name>Existing</Name></Properties></ExternalDataProcessor></MetaDataObject>\r\n";
            fs::write(&input, bytes).unwrap();
            let component =
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/component.py");
            let output = command(&fixture.0, locale)
                .env(
                    "ESKA_BUILD_BINARY",
                    if mode == "missing" {
                        fixture.0.join("missing")
                    } else {
                        component
                    },
                )
                .env("ESKA_COMPONENT_FIXTURE_MODE", mode)
                .args([
                    "__shell-new",
                    "project",
                    "--from",
                    "input.epf",
                    "--workflow",
                    "trunk",
                    "--no-vcs",
                    "--platform-version",
                    "8.3.27.2325",
                ])
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(1), "{output:?}");
            assert_eq!(output.stdout, [] as [u8; 0]);
            assert_ne!(output.stderr, [] as [u8; 0]);
            assert!(!root.exists());
            assert_eq!(fs::read(&input).unwrap(), bytes);
            assert_eq!(
                fs::read(fixture.0.join(".eska/keep")).unwrap(),
                b"user bytes"
            );
            assert_eq!(fs::read_dir(fixture.0.join(".eska")).unwrap().count(), 1);
        }
    }
}

/// Invoke the real component using only its absolute executable path.
fn command(root: &Path, locale: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_eska-project"));
    command
        .current_dir(root)
        .env("PATH", "")
        .env("NO_COLOR", "1")
        .env_remove("ESKA_LANG")
        .args(["--lang", locale, "--project-dir"])
        .arg(root);
    command
}

/// Successful machine output has no human diagnostics or terminal control sequences.
fn success(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!output.stdout.contains(&0x1b));
    serde_json::from_slice(&output.stdout).expect("JSON document")
}

/// Reject an operation with a localized stderr and a stable machine category.
fn failure(output: Output, code: &str) -> Vec<u8> {
    assert_eq!(output.status.code(), Some(1));
    assert_ne!(output.stderr, [] as [u8; 0]);
    assert!(!output.stderr.contains(&0x1b));
    let document: Value = serde_json::from_slice(&output.stdout).expect("failure JSON");
    assert_eq!(document["error"]["code"], code);
    output.stdout
}

/// Supply root identity with BOM/CRLF and opaque payloads whose exact bytes must survive init.
fn export(root: &Path, kind: &str) -> Vec<(String, Vec<u8>)> {
    fs::create_dir_all(root).unwrap();
    let (tag, extra, name) = match kind {
        "configuration" => (
            "Configuration",
            "<CompatibilityMode>Version8_3_27</CompatibilityMode>",
            "Configuration.xml",
        ),
        "extension" => (
            "Configuration",
            "<ConfigurationExtensionPurpose>Customization</ConfigurationExtensionPurpose><ConfigurationExtensionCompatibilityMode>Version8_3_24</ConfigurationExtensionCompatibilityMode>",
            "Configuration.xml",
        ),
        "processing" => ("ExternalDataProcessor", "", "Обработка.xml"),
        "report" => ("ExternalReport", "", "Отчёт.xml"),
        _ => panic!("test type"),
    };
    let files = vec![(name.into(), format!("\u{feff}<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.17\"><{tag} uuid=\"12345678-1234-1234-1234-123456789012\"><Properties><Name>Пример</Name><Version>1.2.3.4</Version>{extra}</Properties><ChildObjects/></{tag}></MetaDataObject>\r\n").into_bytes()), ("Модуль.bsl".into(), b"\xef\xbb\xbf// unchanged\r\n".to_vec()), ("opaque.bin".into(), vec![0,255,13,10])];
    for (path, bytes) in &files {
        fs::write(root.join(path), bytes).unwrap();
    }
    files
}

/// All supported project kinds work without any executable dependency or implicit repository.
#[test]
fn new_and_root_validation_are_independent_of_native_tools() {
    let fixture = TestDir::new();
    for (index, kind) in ["configuration", "extension", "processing", "report"]
        .iter()
        .enumerate()
    {
        let name = format!("Проект с пробелами {index}");
        let source = if index == 0 {
            "."
        } else {
            "Выгрузка/xml"
        };
        let created = success(
            &command(&fixture.0, "en")
                .args([
                    "new", &name, "--type", kind, "--source", source, "--format", "json",
                ])
                .output()
                .unwrap(),
        );
        assert_eq!(created["status"], "created");
        assert_eq!(created["vcs_initialized"], false);
        let root = fixture.0.join(name);
        assert!(!root.join(".git").exists());
        let config = fs::read_to_string(root.join("eska.toml")).unwrap();
        assert!(config.contains("platform_version = \"\""));
        assert!(!config.contains("[vcs"));
        export(&root.join(source), kind);
        let en = success(
            &command(&root, "en")
                .args(["validate", "--format", "json"])
                .output()
                .unwrap(),
        );
        let ru = success(
            &command(&root, "ru")
                .args(["validate", "--format", "json"])
                .output()
                .unwrap(),
        );
        assert_eq!(en, ru);
        assert_eq!(en["projects"][0]["project_type"], *kind);
        let context = success(
            &command(&root, "en")
                .args(["context", "--format", "json"])
                .output()
                .unwrap(),
        );
        assert_eq!(
            context["context"]["limited_context"],
            *kind != "configuration"
        );
        assert!(!root.join(".eska").exists());
    }
}

/// Repeated initialization preserves every export byte and refuses an existing manifest.
#[test]
fn init_keeps_bom_crlf_and_opaque_payloads() {
    let fixture = TestDir::new();
    for (index, kind) in ["configuration", "extension", "processing", "report"]
        .iter()
        .enumerate()
    {
        let root = fixture.0.join(format!("existing-{index}"));
        let source = root.join("Исходники XML");
        let files = export(&source, kind);
        success(
            &command(&root, "ru")
                .args([
                    "init",
                    "--source",
                    "Исходники XML",
                    "--platform-version",
                    "8.3.27.2325",
                    "--format",
                    "json",
                ])
                .output()
                .unwrap(),
        );
        assert!(!root.join(".git").exists());
        for (path, bytes) in files {
            assert_eq!(fs::read(source.join(path)).unwrap(), bytes);
        }
        let manifest = fs::read(root.join("eska.toml")).unwrap();
        let en = failure(
            command(&root, "en")
                .args(["init", "--format", "json"])
                .output()
                .unwrap(),
            "manifest_exists",
        );
        let ru = failure(
            command(&root, "ru")
                .args(["init", "--format", "json"])
                .output()
                .unwrap(),
            "manifest_exists",
        );
        assert_eq!(en, ru);
        assert_eq!(fs::read(root.join("eska.toml")).unwrap(), manifest);
    }
}

/// Workspace creation and enrollment retain comments, inherit settings and omit nested controls.
#[test]
fn workspace_new_init_and_application_selection_share_core_rules() {
    let fixture = TestDir::new();
    fs::write(fixture.0.join("eska.toml"),"# root preserved\n[workspace]\nmembers=[] # members preserved\n[build]\nplatform_version=\"8.3.27.2325\"\nartifacts_directory=\"artifacts\"\n").unwrap();
    success(
        &command(&fixture.0, "en")
            .args(["new", "base", "--type", "configuration", "--format", "json"])
            .output()
            .unwrap(),
    );
    export(&fixture.0.join("src/base/src"), "configuration");
    let extension = fixture.0.join("src/ext");
    let files = export(&extension.join("src"), "extension");
    success(
        &command(&extension, "ru")
            .args(["init", "--format", "json"])
            .output()
            .unwrap(),
    );
    for (path, bytes) in files {
        assert_eq!(fs::read(extension.join("src").join(path)).unwrap(), bytes);
    }
    for root in [fixture.0.join("src/base"), extension] {
        for name in [".git", ".gitignore", ".gitattributes"] {
            assert!(!root.join(name).exists());
        }
        let config = fs::read_to_string(root.join("eska.toml")).unwrap();
        assert!(!config.contains("artifacts_directory"));
        assert!(!config.contains("platform_version"));
    }
    let mut root = fs::read_to_string(fixture.0.join("eska.toml")).unwrap();
    assert!(root.contains("# root preserved") && root.contains("# members preserved"));
    let en = failure(
        command(&fixture.0, "en")
            .args(["context", "--format", "json"])
            .output()
            .unwrap(),
        "application_declarations_missing",
    );
    let ru = failure(
        command(&fixture.0, "ru")
            .args(["context", "--format", "json"])
            .output()
            .unwrap(),
        "application_declarations_missing",
    );
    assert_eq!(en, ru);
    root.push_str("\n[applications.main]\nconfiguration=\"base\"\nextensions=[\"ext\"]\n");
    fs::write(fixture.0.join("eska.toml"), root).unwrap();
    let en = success(
        &command(&fixture.0, "en")
            .args(["context", "--format", "json"])
            .output()
            .unwrap(),
    );
    let ru = success(
        &command(&fixture.0, "ru")
            .args(["context", "--format", "json"])
            .output()
            .unwrap(),
    );
    assert_eq!(en, ru);
    assert_eq!(en["context"]["target_platform_version"], "8.3.27.2325");
    assert_eq!(en["context"]["sources"][1]["member"], "ext");
    assert_eq!(
        en["context"]["sources"][1]["compatibility_mode"],
        "Version8_3_24"
    );
    assert!(!fixture.0.join("bsl-analyzer.toml").exists());
}

/// Invalid options and collisions leave user files and even the destination namespace untouched.
#[test]
fn preflight_rejects_invalid_input_before_writing() {
    let fixture = TestDir::new();
    for (args, code) in [
        (
            vec!["new", "bad", "--type", "other", "--format", "json"],
            "project_type",
        ),
        (
            vec![
                "new",
                "bad",
                "--type",
                "report",
                "--source",
                "../escape",
                "--format",
                "json",
            ],
            "project_config",
        ),
        (
            vec![
                "new",
                "bad",
                "--type",
                "report",
                "--platform-version",
                "8.3.27",
                "--format",
                "json",
            ],
            "platform_version",
        ),
    ] {
        failure(command(&fixture.0, "en").args(args).output().unwrap(), code);
        assert!(!fixture.0.join("bad").exists());
    }
    let existing = fixture.0.join("existing");
    fs::create_dir(&existing).unwrap();
    fs::write(existing.join("mine"), b"keep").unwrap();
    failure(
        command(&fixture.0, "ru")
            .args(["new", "existing", "--type", "report", "--format", "json"])
            .output()
            .unwrap(),
        "destination_exists",
    );
    assert_eq!(fs::read(existing.join("mine")).unwrap(), b"keep");
    assert_eq!(fs::read_dir(existing).unwrap().count(), 1);
}

/// Empty workspaces and malformed root XML never produce a successful validation envelope.
#[test]
fn invalid_root_and_empty_workspace_fail_in_both_languages() {
    let fixture = TestDir::new();
    fs::write(fixture.0.join("eska.toml"), "[workspace]\nmembers=[]\n").unwrap();
    for lang in ["en", "ru"] {
        failure(
            command(&fixture.0, lang)
                .args(["validate", "--workspace", "--format", "json"])
                .output()
                .unwrap(),
            "no_projects",
        );
    }
    fs::remove_file(fixture.0.join("eska.toml")).unwrap();
    success(
        &command(&fixture.0, "en")
            .args([
                "new",
                "broken",
                "--type",
                "configuration",
                "--format",
                "json",
            ])
            .output()
            .unwrap(),
    );
    let root = fixture.0.join("broken");
    fs::write(root.join("src/Configuration.xml"), b"<broken>").unwrap();
    for lang in ["en", "ru"] {
        failure(
            command(&root, lang)
                .args(["validate", "--format", "json"])
                .output()
                .unwrap(),
            "project_source",
        );
    }
}

/// A symlinked export cannot escape the project during initialization.
#[cfg(unix)]
#[test]
fn source_containment_is_checked_before_any_write() {
    let fixture = TestDir::new();
    let outside = fixture.0.join("outside");
    export(&outside, "configuration");
    let root = fixture.0.join("root");
    fs::create_dir(&root).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("src")).unwrap();
    failure(
        command(&root, "en")
            .args(["init", "--format", "json"])
            .output()
            .unwrap(),
        "invalid_source",
    );
    assert!(!root.join("eska.toml").exists());
}

/// Shared metadata and version commands have correct standalone help and work with no full CLI.
#[test]
fn metadata_version_and_help_are_available_without_the_wrapper() {
    let fixture = TestDir::new();
    export(&fixture.0.join("src"), "configuration");
    success(
        &command(&fixture.0, "en")
            .args(["init", "--format", "json"])
            .output()
            .unwrap(),
    );
    for locale in ["en", "ru"] {
        let version = success(
            &command(&fixture.0, locale)
                .args(["version", "--format", "json"])
                .output()
                .unwrap(),
        );
        assert!(version.to_string().contains("1.2.3.4"));
        let metadata = success(
            &command(&fixture.0, locale)
                .args(["metadata", "inspect"])
                .output()
                .unwrap(),
        );
        assert!(metadata["result"].is_object());
        for args in [
            vec!["--help"],
            vec!["new", "--help"],
            vec!["metadata", "inspect", "--help"],
            vec!["version", "--help"],
        ] {
            let output = command(&fixture.0, locale).args(args).output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let help = String::from_utf8(output.stdout).unwrap();
            assert!(help.contains("eska-project"));
            assert!(!help.contains("eska metadata"));
        }
    }
}

/// Freshness requires no executable, does not create state and yields locale-independent JSON.
#[test]
fn analyzer_configuration_status_is_read_only_and_independent_of_locale() {
    let fixture = TestDir::new();
    export(&fixture.0.join("src"), "configuration");
    success(
        &command(&fixture.0, "en")
            .args(["init", "--format", "json"])
            .output()
            .unwrap(),
    );
    for expected in ["missing", "migration_required"] {
        if expected == "migration_required" {
            fs::write(
                fixture.0.join("bsl-analyzer.toml"),
                "[source]\nroot='src'\n[diagnostics.parameters]\nLineLength=false\n",
            )
            .unwrap();
        }
        let en = command(&fixture.0, "en")
            .args(["analyzer-config", "status", "--format", "json"])
            .output()
            .unwrap();
        let ru = command(&fixture.0, "ru")
            .args(["analyzer-config", "status", "--format", "json"])
            .output()
            .unwrap();
        assert_eq!(en.status.code(), Some(1));
        assert_eq!(ru.status.code(), Some(1));
        assert_eq!(en.stdout, ru.stdout);
        let result: Value = serde_json::from_slice(&en.stdout).unwrap();
        assert_eq!(result["status"], expected);
        assert!(!fixture.0.join(".eska").exists());
    }
}

/// The real published analyzer validates preview and migration through the standalone process CLI.
#[test]
#[ignore = "requires ESKA_BSL_ANALYZER pointing to published bsl-analyzer-app 0.4.0"]
fn analyzer_migration_process_uses_real_check_config_and_stable_json() {
    let fixture = TestDir::new();
    export(&fixture.0.join("src"), "configuration");
    success(
        &command(&fixture.0, "en")
            .args(["init", "--format", "json"])
            .output()
            .unwrap(),
    );
    let original =
        b"# retain original\r\n[source]\r\nroot='src'\r\n[formatting]\r\nindent_size=2\r\n";
    fs::write(fixture.0.join("bsl-analyzer.toml"), original).unwrap();
    let args = ["analyzer-config", "migrate", "--format", "json"];
    let en = success(&command(&fixture.0, "en").args(args).output().unwrap());
    let ru = success(&command(&fixture.0, "ru").args(args).output().unwrap());
    assert_eq!(en, ru);
    let token = en["preview"]["token"].as_str().unwrap();
    let applied = success(
        &command(&fixture.0, "ru")
            .args([
                "analyzer-config",
                "migrate",
                "--apply",
                "--preview-token",
                token,
                "--format",
                "json",
            ])
            .output()
            .unwrap(),
    );
    assert_eq!(applied["status"], "migrated");
    let backup = applied["backup"]["value"].as_str().unwrap();
    assert_eq!(fs::read(backup).unwrap(), original);
    let en = success(
        &command(&fixture.0, "en")
            .args(["analyzer-config", "prepare", "--format", "json"])
            .output()
            .unwrap(),
    );
    let ru = success(
        &command(&fixture.0, "ru")
            .args(["analyzer-config", "prepare", "--format", "json"])
            .output()
            .unwrap(),
    );
    assert_eq!(en, ru);
    assert_eq!(en["changed"], false);
}
