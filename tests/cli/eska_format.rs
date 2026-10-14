use std::{fs, path::Path, process::Command};

use eska::{
    build_core::{self, BuildError, BuildPlan, BuildProvenance, GitProvenance, Ibcmd, ToolOptions},
    project_core::{config::ProjectConfig, discovery::discover},
};
use serde_json::Value;

use crate::support::TestDir;

/// Select ESKA explicitly while keeping an adversarial leftover XML in the same source directory.
fn project() -> TestDir {
    let fixture = TestDir::new();
    fs::create_dir(fixture.0.join("src")).expect("source directory");
    fs::write(
        fixture.0.join("eska.toml"),
        concat!(
            "[project]\ntype='configuration'\nsource_format='eska'\n",
            "[eska]\naccepted_base='accepted'\n[build]\nplatform_version='8.3.27.2325'\n",
        ),
    )
    .expect("configured source");
    fs::write(fixture.0.join("src/Configuration.xml"),
        "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\"><Configuration><Properties><Name>Stale</Name><Version>1.2.3.4</Version></Properties></Configuration></MetaDataObject>"
    ).expect("leftover XML");
    fs::write(fixture.0.join("src/configuration.yaml"), b"broken: [").expect("bad YAML");
    fixture
}

/// Invoke the existing application with isolated machine settings and an explicit UI locale.
fn eska(root: &Path, locale: &str, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(root)
        .env("ESKA_CONFIG_DIR", root.join("settings"))
        .env_remove("ESKA_LANG")
        .env_remove("ESKA_ACCEPTED_BASE_SHA256")
        .args(["--lang", locale])
        .args(args)
        .output()
        .expect("CLI result")
}

#[test]
/// Unsupported ESKA preview fails before tool discovery; JSON remains identical in both locales.
fn native_build_cli_refuses_eska_preview_before_platform_discovery() {
    let fixture = project();
    let mut documents = Vec::new();
    for (locale, message) in [
        ("ru", "Предпросмотр сборки ESKA недоступен."),
        ("en", "ESKA build preview is unavailable."),
    ] {
        let output = eska(
            &fixture.0,
            locale,
            &[
                "build",
                "--dry-run",
                "--format",
                "json",
                "--ibcmd",
                "missing-platform-tool",
            ],
        );
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains(message));
        let document: Value = serde_json::from_slice(&output.stdout).expect("JSON failure");
        assert_eq!(document["schema_version"], 1);
        assert_eq!(document["error"]["code"], "build-eska-dry-run-unsupported");
        documents.push(document);
    }
    assert_eq!(documents[0], documents[1]);
    assert!(!fixture.0.join("build").exists());
}

#[test]
/// Legacy bump stays unsupported; source import refuses force or absent approval before tool reads.
fn version_and_import_preflight_preserve_eska_sources_and_ignore_leftover_xml() {
    let fixture = project();
    let xml = fixture.0.join("src/Configuration.xml");
    let yaml = fixture.0.join("src/configuration.yaml");
    let before = (fs::read(&xml).expect("XML"), fs::read(&yaml).expect("YAML"));
    let mut imports = Vec::new();
    for (locale, message) in [
        ("ru", "Эта XML-операция недоступна для исходников ESKA."),
        ("en", "This XML operation is unavailable for ESKA sources."),
    ] {
        let output = eska(&fixture.0, locale, &["version", "bump", "patch"]);
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains(message));
        for (args, code) in [
            (
                &["import", "missing.cf", "--force", "--format", "json"][..],
                "import-eska-force-incompatible",
            ),
            (
                &["import", "missing.cf", "--format", "json"][..],
                "import-accepted-required",
            ),
        ] {
            let output = eska(&fixture.0, locale, args);
            assert_eq!(output.status.code(), Some(1), "{output:?}");
            let document: Value = serde_json::from_slice(&output.stdout).expect("JSON failure");
            assert_eq!(document["error"]["code"], code);
            imports.push(document);
        }
    }
    assert_eq!(imports[0], imports[2]);
    assert_eq!(imports[1], imports[3]);
    assert_eq!(fs::read(xml).expect("unchanged XML"), before.0);
    assert_eq!(fs::read(yaml).expect("unchanged YAML"), before.1);
    assert!(!fixture.0.join("build").exists());
}

#[test]
/// An editable manifest never supplies the approval digest used by protected export.
fn invalid_eska_config_is_localized_and_json_errors_do_not_depend_on_locale() {
    let fixture = project();
    fs::write(
        fixture.0.join("eska.toml"),
        "[project]\ntype='configuration'\nsource_format='eska'\n",
    )
    .expect("missing locator");
    let mut documents = Vec::new();
    for locale in ["ru", "en"] {
        let output = eska(&fixture.0, locale, &["build", "--format", "json"]);
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("[eska].accepted_base"));
        documents.push(serde_json::from_slice::<Value>(&output.stdout).expect("JSON failure"));
    }
    assert_eq!(documents[0], documents[1]);
    assert!(!fixture.0.join("build").exists());
    assert!(
        ProjectConfig::from_toml(concat!(
            "[project]\ntype='configuration'\nsource_format='eska'\n",
            "[eska]\naccepted_base='accepted'\nsha256='self-approved'\n",
        ))
        .is_err()
    );
}

#[test]
/// Changing only the plan's path cannot turn an ESKA build into an ordinary native build.
fn native_library_build_requires_preparation_even_with_a_leftover_xml_path() {
    let fixture = project();
    let project = discover(&fixture.0).expect("project discovery");
    let tool = Ibcmd::discover(
        project
            .configuration()
            .build_settings()
            .platform_version()
            .expect("platform"),
        &ToolOptions::new(Some(super::build::fake_ibcmd(&fixture)), None, None),
    )
    .expect("portable tool version only");
    let artifact = fixture.0.join("previous.cf");
    let manifest = build_core::manifest_path(&artifact);
    fs::write(&artifact, b"previous artifact sentinel").expect("artifact");
    fs::write(&manifest, b"previous manifest sentinel").expect("manifest");
    let plan = BuildPlan::new(&project, Some(&artifact)).expect("build plan");
    let plan = plan.with_snapshot_source(project.source().to_owned());
    let mut called = false;
    for result in [
        build_core::execute(&plan, &tool),
        build_core::execute_streaming_with_manifest(
            &plan,
            &project,
            BuildProvenance::new(None, GitProvenance::Unavailable),
            &tool,
            |_, _, _| {
                called = true;
            },
        ),
    ] {
        let Err(BuildError::SourceValidation(error)) = result else {
            panic!("expected unprepared source refusal: {result:?}");
        };
        assert_eq!(error.code, "source_preparation_required");
    }
    assert!(!called);
    assert_eq!(
        fs::read(artifact).expect("preserved artifact"),
        b"previous artifact sentinel"
    );
    assert_eq!(
        fs::read(manifest).expect("preserved manifest"),
        b"previous manifest sentinel"
    );
    assert!(!fixture.0.join("build").exists());
}
