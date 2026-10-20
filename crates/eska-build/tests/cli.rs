//! Independent process tests use the same portable native fixture as the legacy CLI.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use eska_project_core::{
    config::ProjectConfig,
    project::{Project, ProjectType, build::BuildSettings, create},
};
use serde_json::Value;

#[path = "../../../tests/support/ibcmd.rs"]
mod ibcmd_fixture;
#[path = "../../../tests/support/mod.rs"]
mod support;

/// Resolve the checkout from the package instead of hardcoding a workspace or playground path.
fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// Minimal Designer roots keep BOM and CRLF; native compilation is tested separately with 1C.
fn project(root: &Path, kind: ProjectType) -> Project {
    let config = ProjectConfig::new(kind)
        .with_build_settings(BuildSettings::new("8.3.27.2325", "artifacts".into()).unwrap());
    let project = create::create(root, &config).unwrap();
    let (tag, extra, name) = match kind {
        ProjectType::Configuration => ("Configuration", "", "Configuration.xml"),
        ProjectType::Extension => (
            "Configuration",
            "<ConfigurationExtensionPurpose>Customization</ConfigurationExtensionPurpose>",
            "Configuration.xml",
        ),
        ProjectType::Processing => ("ExternalDataProcessor", "", "Обработка.xml"),
        ProjectType::Report => ("ExternalReport", "", "Отчёт.xml"),
    };
    fs::write(project.source().join(name),format!("\u{feff}<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\"><{tag} uuid=\"11111111-1111-1111-1111-111111111111\"><Properties><Name>Пример</Name>{extra}</Properties><ChildObjects/></{tag}></MetaDataObject>\r\n")).unwrap();
    project
}

/// The standalone executable has no wrapper, project server, analyzer or Git on PATH.
fn command(root: &Path, config: &Path, locale: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_eska-build"));
    command
        .current_dir(root)
        .env("PATH", "")
        .env("ESKA_CONFIG_DIR", config)
        .env_remove("ESKA_IBCMD")
        .env_remove("ESKA_DISTROBOX")
        .env_remove("ESKA_PLATFORM_ARCH")
        .env_remove("ESKA_LANG")
        .args(["--lang", locale]);
    command
}

/// Machine stdout is exactly one JSON document; native diagnostics are emitted on stderr.
fn document(output: &Output, success: bool) -> Value {
    assert_eq!(output.status.success(), success, "{output:?}");
    serde_json::from_slice(&output.stdout).unwrap()
}

/// All supported build types and their manifests work directly, without mandatory VCS discovery.
#[test]
fn standalone_builds_all_types_and_clean_preserves_artifacts() {
    let fixture = support::TestDir::new();
    let binary = ibcmd_fixture::executable(&repository());
    for locale in ["ru", "en"] {
        for (index, kind) in [
            ProjectType::Configuration,
            ProjectType::Extension,
            ProjectType::Processing,
            ProjectType::Report,
        ]
        .into_iter()
        .enumerate()
        {
            let project = project(&fixture.0.join(format!("{locale}-{index}")), kind);
            let output = command(project.root(), &fixture.0.join("config"), locale)
                .args(["build", "--format", "json", "--manifest", "--ibcmd"])
                .arg(&binary)
                .output()
                .unwrap();
            let report = document(&output, true);
            assert_eq!(report["schema_version"], 1);
            let plan = eska_build_runtime::build::BuildPlan::new(&project, None).unwrap();
            let artifact = plan.output();
            assert!(artifact.is_file());
            let manifest = eska_build_runtime::build::manifest_path(artifact);
            let data: Value = serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
            assert_eq!(data["source"]["git"]["status"], "unavailable");
            let before = fs::read(artifact).unwrap();
            let output = command(project.root(), &fixture.0.join("config"), locale)
                .arg("clean")
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            assert_eq!(fs::read(artifact).unwrap(), before);
            assert!(manifest.is_file());
            assert!(!project.root().join(".git").exists());
        }
    }
}

/// A selected VCS is optional for standalone, but its broken contract must stop publication.
#[cfg(unix)]
#[test]
fn invalid_vcs_responses_preserve_previous_artifact_and_manifest() {
    let fixture = support::TestDir::new();
    let binary = ibcmd_fixture::executable(&repository());
    let vcs = repository().join("tests/fixtures/component.py");
    for locale in ["ru", "en"] {
        for mode in [
            "bridge-schema",
            "bridge-data",
            "bridge-operation",
            "bridge-extra",
            "bridge-oversized",
            "bridge-failed",
        ] {
            let project = project(
                &fixture.0.join(format!("{locale}-{mode}")),
                ProjectType::Configuration,
            );
            let plan = eska_build_runtime::build::BuildPlan::new(&project, None).unwrap();
            fs::create_dir_all(plan.output().parent().unwrap()).unwrap();
            fs::write(plan.output(), b"previous artifact").unwrap();
            let manifest = eska_build_runtime::build::manifest_path(plan.output());
            fs::write(&manifest, b"previous manifest").unwrap();
            let source = fs::read(project.source().join("Configuration.xml")).unwrap();
            let output = command(project.root(), &fixture.0.join("config"), locale)
                .env("ESKA_COMPONENT_FIXTURE_MODE", mode)
                .arg("--vcs-binary")
                .arg(&vcs)
                .args(["build", "--format", "json", "--manifest", "--ibcmd"])
                .arg(&binary)
                .output()
                .unwrap();
            let report = document(&output, false);
            assert_eq!(report["error"]["code"], "vcs-component", "{report}");
            assert_eq!(report["error"]["stage"], "provenance");
            assert_eq!(fs::read(plan.output()).unwrap(), b"previous artifact");
            assert_eq!(fs::read(manifest).unwrap(), b"previous manifest");
            assert_eq!(
                fs::read(project.source().join("Configuration.xml")).unwrap(),
                source
            );
            assert!(!project.root().join(".eska/build").exists());
        }
    }
}

/// A complete VCS response retains the old manifest schema without linking Git into build.
#[cfg(unix)]
#[test]
fn explicit_vcs_provenance_crosses_the_process_boundary() {
    let fixture = support::TestDir::new();
    let binary = ibcmd_fixture::executable(&repository());
    let vcs = repository().join("tests/fixtures/component.py");
    let project = project(&fixture.0.join("project"), ProjectType::Configuration);
    for locale in ["ru", "en"] {
        let output = command(project.root(), &fixture.0.join("config"), locale)
            .env("ESKA_COMPONENT_FIXTURE_MODE", "bridge-good")
            .arg("--vcs-binary")
            .arg(&vcs)
            .args(["build", "--format", "json", "--manifest", "--ibcmd"])
            .arg(&binary)
            .output()
            .unwrap();
        document(&output, true);
        let plan = eska_build_runtime::build::BuildPlan::new(&project, None).unwrap();
        let manifest: Value = serde_json::from_slice(
            &fs::read(eska_build_runtime::build::manifest_path(plan.output())).unwrap(),
        )
        .unwrap();
        assert_eq!(
            manifest["source"]["git"],
            serde_json::json!({"status":"available","commit":"a".repeat(40),"dirty":false})
        );
    }
}

/// Standalone export creates only XML in a new directory and reports the content's actual type.
#[test]
fn standalone_unpacks_four_types_in_both_languages() {
    let fixture = support::TestDir::new();
    let binary = ibcmd_fixture::executable(&repository());
    for locale in ["ru", "en"] {
        for (extension, tag, kind) in [
            ("cf", "Configuration", "configuration"),
            ("cfe", "Configuration", "extension"),
            ("epf", "ExternalDataProcessor", "processing"),
            ("erf", "ExternalReport", "report"),
        ] {
            let input = fixture.0.join(format!("{locale}.{extension}"));
            let extra = if kind == "extension" {
                "<ConfigurationExtensionPurpose>Customization</ConfigurationExtensionPurpose>"
            } else {
                ""
            };
            fs::write(&input,format!("<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\"><{tag} uuid=\"11111111-1111-1111-1111-111111111111\"><Properties><Name>ИмяИзXML</Name>{extra}</Properties></{tag}></MetaDataObject>")).unwrap();
            let destination = fixture.0.join(format!("{locale}-{extension} XML"));
            let output = command(&fixture.0, &fixture.0.join("config"), locale)
                .arg("unpack")
                .arg(&input)
                .arg(&destination)
                .args([
                    "--format",
                    "json",
                    "--platform-version",
                    "8.3.27.2325",
                    "--ibcmd",
                ])
                .arg(&binary)
                .output()
                .unwrap();
            let report = document(&output, true);
            assert_eq!(report["status"], "unpacked");
            assert_eq!(report["identity"]["project_type"], kind);
            assert_eq!(report["identity"]["name"], "ИмяИзXML");
            assert!(!destination.join("eska.toml").exists());
            assert!(!destination.join(".git").exists());
        }
    }
}

/// Existing output is rejected before discovering or spawning any platform process.
#[test]
fn unpack_does_not_replace_existing_directories() {
    let fixture = support::TestDir::new();
    let destination = fixture.0.join("existing");
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("keep"), b"user bytes").unwrap();
    let output = command(&fixture.0, &fixture.0.join("config"), "en")
        .args(["unpack", "missing.cf"])
        .arg(&destination)
        .args(["--format", "json", "--ibcmd", "missing-platform"])
        .output()
        .unwrap();
    let report = document(&output, false);
    assert_eq!(report["error"]["code"], "unsafe-path");
    assert_eq!(fs::read(destination.join("keep")).unwrap(), b"user bytes");
    assert!(!fixture.0.join(".eska").exists());
}

/// Help reflects standalone command paths and remains available before runtime discovery.
#[test]
fn help_uses_the_standalone_name_and_both_locales() {
    let fixture = support::TestDir::new();
    for locale in ["ru", "en"] {
        for action in ["build", "unpack", "clean", "platform", "config"] {
            let output = command(&fixture.0, &fixture.0.join("config"), locale)
                .args([action, "--help"])
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            let help = String::from_utf8(output.stdout).unwrap();
            assert!(help.contains(&format!("eska-build {action}")));
            assert!(help.contains(if locale == "ru" {
                "Использование"
            } else {
                "Usage"
            }));
            assert!(!help.contains(&format!("eska {action}")));
        }
    }
}
