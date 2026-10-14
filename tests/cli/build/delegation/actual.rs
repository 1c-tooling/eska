//! Real facade/companion builds consume the same saved authority and portable native process fixture.

#[path = "../../../fixtures/eska-format/query_creation.rs"]
mod native_fixture;

use super::{super::ibcmd_fixture, fixture::Fixture};
use eska::project_core::{
    ProjectName,
    creation::{ImportedProject, import_query_project},
    platform_settings::PlatformVersion,
    source_format::NativeUuid,
};
use serde_json::Value;
use std::{fs, path::Path, process::Command};

/// Install an actual compatible companion in the fixture's isolated executable directory.
fn companion(fixture: &Fixture) {
    let destination = fixture
        .0
        .0
        .join(format!("bin/eska-build{}", std::env::consts::EXE_SUFFIX));
    fs::hard_link(env!("CARGO_BIN_EXE_eska-build"), &destination)
        .or_else(|_| fs::copy(env!("CARGO_BIN_EXE_eska-build"), &destination).map(|_| ()))
        .unwrap();
}

/// Shared native profiles retain all query/form inputs and make the metadata root explicit.
fn project(
    fixture: &Fixture,
    kind: &str,
    platform: bool,
) -> (ImportedProject, native_fixture::Native) {
    let native = native_fixture::native(&fixture.0.0, kind);
    for path in [
        &native.root,
        &native.form,
        &native.dialog,
        &native.schema,
        &native.module,
    ] {
        assert!(native.input.join(path).is_file());
    }
    let version = platform.then(|| PlatformVersion::parse("8.3.27.2325").unwrap());
    let imported = import_query_project(
        &native.input,
        &fixture.0.0.join("project"),
        ProjectName::parse("demo".to_owned()).unwrap(),
        version.as_ref(),
        &[NativeUuid::parse(native_fixture::FORM_UUID).unwrap()],
    )
    .unwrap();
    let source = imported.project.source();
    assert!(
        fs::read_to_string(source.join(native.root_source))
            .unwrap()
            .contains(native_fixture::ROOT_UUID)
    );
    assert!(source.join(&native.form_source).is_file());
    (imported, native)
}

/// The same native runner and machine-local inputs are visible to both invocation paths.
fn platform(command: &mut Command, fixture: &Fixture, module: &Path) {
    command
        .env_remove("ESKA_IBCMD")
        .env_remove("ESKA_DISTROBOX")
        .env_remove("ESKA_PLATFORM_ARCH")
        .env("FAKE_IBCMD_LOG", fixture.0.0.join("platform.log"))
        .env("FAKE_IBCMD_ARTIFACT_SOURCE_FILE", module)
        .arg("--ibcmd")
        .arg(ibcmd_fixture::executable());
}

/// Platform execution returns one stable tool envelope; no facade-specific success document is added.
fn success(output: &std::process::Output) -> Value {
    assert!(output.status.success(), "{output:?}");
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["schema_version"], 1);
    assert_eq!(result["tool"], "eska-build");
    assert_eq!(result["command"], "build");
    assert_eq!(result["status"], "success");
    result
}

#[test]
/// All four native types build saved BOM/CRLF bytes in both locales through the real companion.
fn four_native_profiles_preserve_saved_bytes_and_direct_result_contract() {
    for kind in native_fixture::TYPES {
        let fixture = Fixture::new();
        companion(&fixture);
        let (imported, native) = project(&fixture, kind, true);
        let root = imported.project.root();
        let mut saved = fs::read(root.join("src").join(native.module_source)).unwrap();
        saved.extend_from_slice("// saved 😀\r\n".as_bytes());
        fs::write(root.join("src").join(native.module_source), &saved).unwrap();
        let source_before = native_fixture::snapshot(&root.join("src"));
        let base_before = native_fixture::snapshot(&root.join(".eska/accepted"));
        fs::write(root.join("base.cf"), b"base context").unwrap();
        for (locale, human_text) in [("ru", "Артефакт собран:"), ("en", "Artifact built:")]
        {
            let mut shell = fixture.command(locale);
            shell.arg("--project-dir").arg(root).args([
                "build",
                "--format",
                "json",
                "--accepted-base-sha256",
                &imported.accepted.sha256,
            ]);
            if kind != "configuration" {
                shell.args(["--base-configuration", "base.cf"]);
            }
            platform(&mut shell, &fixture, &native.module);
            let value = success(&shell.output().unwrap());
            assert_eq!(value["data"]["artifact_type"], kind);
            assert_eq!(value["data"]["source_format"], "eska");
            assert_eq!(
                fs::read(value["data"]["artifact"].as_str().unwrap()).unwrap(),
                saved
            );
            let manifest: Value = serde_json::from_slice(
                &fs::read(value["data"]["manifest"].as_str().unwrap()).unwrap(),
            )
            .unwrap();
            assert_eq!(
                manifest["source"]["protected"]["baseline"],
                imported.accepted.sha256
            );

            let mut direct = Command::new(env!("CARGO_BIN_EXE_eska-build"));
            direct
                .current_dir(&fixture.0.0)
                .env("ESKA_CONFIG_DIR", fixture.0.0.join("machine"))
                .args(["--lang", locale, "--project-dir"])
                .arg(root)
                .args([
                    "build",
                    "--format",
                    "json",
                    "--accepted-base-sha256",
                    &imported.accepted.sha256,
                ]);
            if kind != "configuration" {
                direct.args(["--base-configuration", "base.cf"]);
            }
            platform(&mut direct, &fixture, &native.module);
            let direct_value = success(&direct.output().unwrap());
            for key in [
                "artifact",
                "manifest",
                "artifact_type",
                "platform_version",
                "runner",
                "source_format",
                "git_provenance",
                "exclusions",
            ] {
                assert_eq!(value["data"][key], direct_value["data"][key], "{key}");
            }
            let mut human = fixture.command(locale);
            human.arg("--project-dir").arg(root).args([
                "build",
                "--accepted-base-sha256",
                &imported.accepted.sha256,
            ]);
            if kind != "configuration" {
                human.args(["--base-configuration", "base.cf"]);
            }
            platform(&mut human, &fixture, &native.module);
            let output = human.output().unwrap();
            assert!(output.status.success(), "{output:?}");
            assert!(String::from_utf8_lossy(&output.stdout).contains(human_text));
            assert!(String::from_utf8_lossy(&output.stderr).contains("fake build warning"));
        }
        assert_eq!(native_fixture::snapshot(&root.join("src")), source_before);
        assert_eq!(
            native_fixture::snapshot(&root.join(".eska/accepted")),
            base_before
        );
    }
}

#[test]
/// A selected member inherits platform/output scope while the shell retains its workspace base path.
fn member_inherits_platform_and_workspace_output_and_base_configuration() {
    let fixture = Fixture::new();
    companion(&fixture);
    let (imported, native) = project(&fixture, "extension", false);
    fs::write(
        fixture.0.0.join("eska.toml"),
        "[workspace]\nmembers=['project']\n[build]\nplatform_version='8.3.27.2325'\n",
    )
    .unwrap();
    fs::write(fixture.0.0.join("host.cf"), b"workspace context").unwrap();
    for locale in ["ru", "en"] {
        let mut shell = fixture.command(locale);
        shell
            .env("ESKA_ACCEPTED_BASE_SHA256", &imported.accepted.sha256)
            .args([
                "build",
                "-p",
                "demo",
                "--format",
                "json",
                "--base-configuration",
                "host.cf",
            ]);
        platform(&mut shell, &fixture, &native.module);
        let value = success(&shell.output().unwrap());
        assert_eq!(
            Path::new(value["data"]["artifact"].as_str().unwrap()),
            fixture.0.0.join("build/demo.cfe")
        );
        assert_eq!(value["data"]["platform_version"], "8.3.27.2325");
    }
    assert!(!imported.project.root().join("build").exists());
}
