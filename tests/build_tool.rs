//! Actual standalone process scenarios without a facade or language-analysis engine.

mod support;

#[path = "support/ibcmd.rs"]
mod ibcmd_fixture;

use eska::project_core::{
    ProjectName, creation::import_project, platform_settings::PlatformVersion,
};
use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use support::TestDir;

/// Run the independent binary with isolated machine config, environment and native fixture log.
fn command(root: &Path, locale: &str, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_eska-build"));
    command
        .current_dir(root)
        .env_remove("ESKA_LANG")
        .env_remove("ESKA_IBCMD")
        .env_remove("ESKA_DISTROBOX")
        .env_remove("ESKA_PLATFORM_ARCH")
        .env_remove("ESKA_ACCEPTED_BASE_SHA256")
        .env("ESKA_CONFIG_DIR", root.join("machine"))
        .env("FAKE_IBCMD_LOG", root.join("platform.log"))
        .args(["--lang", locale])
        .args(args);
    command
}

/// Keep JSON stdout as one envelope; localized diagnostics may be present only on stderr.
fn report(output: &Output, status: &str) -> Value {
    assert_eq!(
        output.status.code(),
        Some(i32::from(status != "success")),
        "{output:?}"
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("one JSON envelope");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["tool"], "eska-build");
    assert_eq!(value["status"], status);
    value
}

/// Create a minimal native configuration with byte-preserved module and empty directory.
fn native(root: &Path) {
    fs::create_dir(root.join("native")).expect("native directory");
    fs::write(
        root.join("native/Configuration.xml"),
        concat!(
            "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\">",
            "<Configuration uuid=\"10000000-0000-4000-8000-000000000001\"><Properties>",
            "<Name>Sample</Name><Comment>Accepted</Comment><Version>1.2.3.4</Version>",
            "</Properties></Configuration></MetaDataObject>"
        ),
    )
    .expect("native configuration");
    fs::write(
        root.join("native/Module.bsl"),
        b"\xef\xbb\xbf// original\r\n",
    )
    .expect("BSL");
    fs::create_dir(root.join("native/Empty")).expect("empty directory");
}

/// Import through the shared offline operation; only the subsequent build uses the native runner.
fn project(root: &Path, frozen: bool) -> String {
    native(root);
    let frozen = if frozen {
        vec![
            eska::project_core::source_format::NativeUuid::parse(
                "10000000-0000-4000-8000-000000000001",
            )
            .expect("UUID"),
        ]
    } else {
        Vec::new()
    };
    let project = import_project(
        &root.join("native"),
        &root.join("project"),
        ProjectName::parse("demo".to_owned()).expect("name"),
        Some(&PlatformVersion::parse("8.3.27.2325").expect("platform")),
        &frozen,
    )
    .expect("import");
    project.accepted.sha256
}

#[test]
/// Help and capabilities work without a project/platform and stay localized or machine-stable.
fn localized_help_and_capabilities_have_no_platform_dependency() {
    let test = TestDir::new();
    let mut capabilities = Vec::new();
    for (locale, phrase) in [
        ("ru", "Сборка нативных артефактов"),
        ("en", "Native artifact builds"),
    ] {
        for args in [
            &["--help"][..],
            &["build", "--help"][..],
            &["capabilities", "--help"][..],
        ] {
            let output = command(&test.0, locale, args).output().expect("help");
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert!(output.stderr.is_empty());
            if args.len() == 1 {
                assert!(String::from_utf8_lossy(&output.stdout).contains(phrase));
            }
        }
        capabilities.push(report(
            &command(&test.0, locale, &["capabilities", "--format", "json"])
                .output()
                .expect("capabilities"),
            "success",
        ));
    }
    assert_eq!(capabilities[0], capabilities[1]);
    assert_eq!(capabilities[0]["data"]["git_required"], false);
    assert_eq!(capabilities[0]["data"]["unpack"], false);
    assert!(!test.0.join("platform.log").exists());
}

#[test]
/// The saved module reaches the actual copied snapshot and the artifact carries protected context.
fn protected_build_preserves_native_bytes_and_publishes_manifest() {
    let test = TestDir::new();
    let pin = project(&test.0, false);
    let root = test.0.join("project");
    let mut cmd = command(
        &root,
        "en",
        &["build", "--format", "json", "--accepted-base-sha256", &pin],
    );
    cmd.arg("--ibcmd")
        .arg(ibcmd_fixture::executable())
        .env("FAKE_IBCMD_ARTIFACT_SOURCE_FILE", "Module.bsl");
    let output = cmd.output().expect("build");
    let value = report(&output, "success");
    assert!(output.stderr.is_empty());
    assert_eq!(
        fs::read(value["data"]["artifact"].as_str().expect("artifact")).expect("bytes"),
        b"\xef\xbb\xbf// original\r\n"
    );
    let manifest: Value = serde_json::from_slice(
        &fs::read(value["data"]["manifest"].as_str().expect("manifest path")).expect("manifest"),
    )
    .expect("manifest JSON");
    assert_eq!(manifest["schema_version"], 2);
    assert_eq!(manifest["source"]["protected"]["baseline"], pin);
    assert_eq!(manifest["source"]["git"]["status"], "unavailable");
    assert_eq!(manifest["project"]["version"]["value"], "1.2.3.4");
    assert_eq!(value["data"]["platform"][0]["stage"], "import_sources");
    assert_eq!(
        value["data"]["platform"][0]["bytes"],
        serde_json::json!(b"[WARN] fake build warning\n")
    );
    assert!(!root.join(".git").exists());
}

#[test]
/// Missing/malformed/wrong pins and invalid saved YAML cannot probe ibcmd or create build resources.
fn invalid_sources_refuse_before_platform_discovery_in_both_locales() {
    let test = TestDir::new();
    let pin = project(&test.0, false);
    let root = test.0.join("project");
    let wrong = "0".repeat(64);
    for (candidate, code) in [
        (None, "accepted_base_pin_required"),
        (Some("oops"), "accepted_base_pin_invalid"),
        (Some(wrong.as_str()), "source_baseline_untrusted"),
    ] {
        let mut reports = Vec::new();
        for locale in ["ru", "en"] {
            let mut cmd = command(&root, locale, &["build", "--format", "json"]);
            cmd.arg("--ibcmd").arg(ibcmd_fixture::executable());
            if let Some(value) = candidate {
                cmd.args(["--accepted-base-sha256", value]);
            }
            let output = cmd.output().expect("refusal");
            let value = report(&output, "error");
            assert_eq!(value["error"]["code"], code);
            assert!(!output.stderr.is_empty());
            reports.push(value);
        }
        assert_eq!(reports[0], reports[1]);
    }
    fs::write(root.join("src/configuration.yaml"), "bad: [\n").expect("bad YAML");
    let output = command(
        &root,
        "en",
        &["build", "--format", "json", "--accepted-base-sha256", &pin],
    )
    .arg("--ibcmd")
    .arg(ibcmd_fixture::executable())
    .output()
    .expect("bad source");
    assert_eq!(
        report(&output, "error")["error"]["code"],
        "source_yaml_invalid"
    );
    assert!(!root.join("platform.log").exists());
    assert!(!root.join("build").exists());
    assert!(!root.join(".eska/generations").exists());
}

#[test]
/// Strict permission refusal retains working edits; opt-in recovery feeds accepted bytes to 1C.
fn forbidden_edits_require_explicit_recovery() {
    let test = TestDir::new();
    let pin = project(&test.0, true);
    let root = test.0.join("project");
    let path = root.join("src/configuration.yaml");
    let original = fs::read_to_string(&path).expect("YAML");
    let working = original.replace("Accepted", "Working");
    assert_ne!(working, original);
    fs::write(&path, &working).expect("direct edit");
    let mut cmd = command(
        &root,
        "ru",
        &["build", "--format", "json", "--accepted-base-sha256", &pin],
    );
    cmd.arg("--ibcmd")
        .arg(ibcmd_fixture::executable())
        .env("FAKE_IBCMD_ARTIFACT_SOURCE_FILE", "Configuration.xml");
    let output = cmd.output().expect("strict refusal");
    let value = report(&output, "error");
    assert_eq!(value["error"]["code"], "source_changes_excluded");
    assert!(
        !value["error"]["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .is_empty()
    );
    assert!(!root.join("platform.log").exists());
    let value = report(
        &cmd.arg("--allow-exclusions")
            .output()
            .expect("recovered build"),
        "success",
    );
    let artifact =
        fs::read_to_string(value["data"]["artifact"].as_str().expect("artifact")).expect("XML");
    assert!(artifact.contains("<Comment>Accepted</Comment>"));
    assert!(!artifact.contains("<Comment>Working</Comment>"));
    assert!(
        !value["data"]["exclusions"]
            .as_array()
            .expect("exclusions")
            .is_empty()
    );
    assert_eq!(fs::read_to_string(path).expect("saved working"), working);
}

#[test]
/// Native success and failure keep exact output bytes and origins without replacing a previous pair.
fn platform_diagnostics_keep_origins_and_failed_build_preserves_previous_pair() {
    let test = TestDir::new();
    let pin = project(&test.0, false);
    let root = test.0.join("project");
    let mut cmd = command(
        &root,
        "en",
        &["build", "--format", "json", "--accepted-base-sha256", &pin],
    );
    cmd.arg("--ibcmd")
        .arg(ibcmd_fixture::executable())
        .env("FAKE_IBCMD_STREAM", "1")
        .env("FAKE_IBCMD_STREAM_LEVEL", "WARN")
        .env("FAKE_IBCMD_STREAM_FILE", "Module.bsl");
    let result = report(&cmd.output().expect("first build"), "success");
    let events = result["data"]["platform"]
        .as_array()
        .expect("platform events");
    let event = events
        .iter()
        .find(|event| !event["origin"].is_null())
        .expect("mapped event");
    assert_eq!(event["origin"]["path"], "Module.bsl");
    assert_eq!(event["origin"]["origin"]["kind"], "payload");
    assert_eq!(event["origin"]["origin"]["authority"], "working");
    let bytes: Vec<u8> = serde_json::from_value(event["bytes"].clone()).expect("raw bytes");
    assert!(String::from_utf8_lossy(&bytes).contains("/Module.bsl, checking\n"));
    let artifact = result["data"]["artifact"].as_str().expect("artifact");
    let manifest = result["data"]["manifest"].as_str().expect("manifest");
    let before = (
        fs::read(artifact).expect("artifact"),
        fs::read(manifest).expect("manifest"),
    );
    let failure = report(
        &cmd.env("FAKE_IBCMD_FAIL_AFTER_STREAM", "1")
            .output()
            .expect("failed import"),
        "error",
    );
    assert_eq!(failure["error"]["code"], "platform_command_failed");
    assert_eq!(
        failure["error"]["diagnostics"]["detail"]["stage"],
        "import_sources"
    );
    let events = failure["error"]["diagnostics"]["platform"]
        .as_array()
        .expect("failed events");
    assert!(
        events
            .iter()
            .any(|event| event["origin"]["path"] == "Module.bsl")
    );
    assert!(events.iter().any(|event| event["stream"] == "stderr"));
    assert_eq!(
        (
            fs::read(artifact).expect("previous artifact"),
            fs::read(manifest).expect("previous manifest")
        ),
        before
    );
}

#[test]
/// A selected member inherits the workspace platform and publishes under the workspace artifact scope.
fn workspace_selection_and_output_scope_are_shared_with_project_core() {
    let test = TestDir::new();
    native(&test.0);
    let imported = import_project(
        &test.0.join("native"),
        &test.0.join("project"),
        ProjectName::parse("demo".to_owned()).expect("name"),
        None,
        &[],
    )
    .expect("inheritable project");
    fs::create_dir_all(test.0.join("other/src")).expect("other member");
    fs::write(
        test.0.join("other/eska.toml"),
        "[project]\nname='other'\ntype='configuration'\n",
    )
    .expect("other config");
    fs::write(
        test.0.join("other/src/Configuration.xml"),
        "broken unselected XML",
    )
    .expect("unselected source");
    fs::write(
        test.0.join("eska.toml"),
        "[workspace]\nmembers=['project','other']\n[build]\nplatform_version='8.3.27.2325'\n",
    )
    .expect("workspace");
    let mut cmd = command(
        &test.0,
        "en",
        &[
            "build",
            "--format",
            "json",
            "--accepted-base-sha256",
            &imported.accepted.sha256,
        ],
    );
    cmd.arg("--ibcmd").arg(ibcmd_fixture::executable());
    assert_eq!(
        report(&cmd.output().expect("missing selector"), "error")["error"]["code"],
        "project_selection_failed"
    );
    assert!(!test.0.join("platform.log").exists());
    let value = report(
        &cmd.args(["--project", "demo"])
            .output()
            .expect("selected build"),
        "success",
    );
    assert_eq!(
        Path::new(value["data"]["artifact"].as_str().expect("artifact")),
        test.0.join("build/demo.cf")
    );
    assert_eq!(value["data"]["platform_version"], "8.3.27.2325");
    assert!(!test.0.join("other/.eska").exists());
    assert!(!test.0.join("project/build").exists());
    assert!(!test.0.join("other/build").exists());
}

#[test]
/// Designer XML keeps its native route and manifest schema; saved sources and human stdout are stable.
fn designer_xml_and_human_output_remain_available() {
    let test = TestDir::new();
    native(&test.0);
    fs::write(test.0.join("eska.toml"),
        "[project]\ntype='configuration'\nsource='native'\n[build]\nplatform_version='8.3.27.2325'\n"
    ).expect("Designer project");
    let before = fs::read(test.0.join("native/Configuration.xml")).expect("native input");
    let mut cmd = command(&test.0, "en", &["build", "--format", "json"]);
    cmd.arg("--ibcmd").arg(ibcmd_fixture::executable());
    let result = report(&cmd.output().expect("Designer build"), "success");
    assert_eq!(result["data"]["source_format"], "designer-xml");
    let manifest: Value = serde_json::from_slice(
        &fs::read(result["data"]["manifest"].as_str().expect("manifest path")).expect("manifest"),
    )
    .expect("JSON");
    assert_eq!(manifest["schema_version"], 1);
    assert!(manifest["source"].get("protected").is_none());
    assert_eq!(
        fs::read(test.0.join("native/Configuration.xml")).expect("native unchanged"),
        before
    );
    assert!(!test.0.join(".eska").exists());
    for (locale, message) in [("ru", "Артефакт собран:"), ("en", "Artifact built:")] {
        let output = command(&test.0, locale, &["build"])
            .arg("--ibcmd")
            .arg(ibcmd_fixture::executable())
            .output()
            .expect("human build");
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(String::from_utf8_lossy(&output.stdout).contains(message));
        assert!(String::from_utf8_lossy(&output.stderr).contains("fake build warning"));
    }
    let output = cmd
        .args(["--accepted-base-sha256", &"0".repeat(64)])
        .output()
        .expect("incompatible source flags");
    assert_eq!(
        report(&output, "error")["error"]["code"],
        "build_source_options_incompatible"
    );
}

#[test]
/// Invalid machine config, missing project platform and exact-version mismatch retain stable codes.
fn runner_preflight_and_machine_config_failures_are_locale_free() {
    let test = TestDir::new();
    let pin = project(&test.0, false);
    let root = test.0.join("project");
    let manifest_path = root.join("eska.toml");
    let manifest = fs::read_to_string(&manifest_path).expect("manifest");
    fs::write(
        &manifest_path,
        manifest.replace(
            "platform_version = \"8.3.27.2325\"",
            "platform_version = \"\"",
        ),
    )
    .expect("missing version");
    let mut cmd = command(
        &root,
        "en",
        &["build", "--format", "json", "--accepted-base-sha256", &pin],
    );
    cmd.arg("--ibcmd").arg(ibcmd_fixture::executable());
    assert_eq!(
        report(&cmd.output().expect("missing version"), "error")["error"]["code"],
        "platform_version_missing"
    );
    assert!(!root.join("platform.log").exists());
    fs::write(&manifest_path, &manifest).expect("restore manifest");
    let output = cmd
        .env("FAKE_IBCMD_VERSION", "8.3.27.9999")
        .output()
        .expect("version mismatch");
    let failure = report(&output, "error");
    assert_eq!(failure["error"]["code"], "platform_version_mismatch");
    assert_eq!(
        failure["error"]["diagnostics"]["actual_version"],
        "8.3.27.9999"
    );
    assert!(!root.join("build").exists());
    fs::create_dir(root.join("machine")).expect("config directory");
    fs::write(
        root.join("machine/config.toml"),
        "[build]\nrunner='distrobox'\n",
    )
    .expect("bad machine config");
    let mut failures = Vec::new();
    for locale in ["ru", "en"] {
        let output = command(
            &root,
            locale,
            &["build", "--format", "json", "--accepted-base-sha256", &pin],
        )
        .arg("--ibcmd")
        .arg(ibcmd_fixture::executable())
        .output()
        .expect("config refusal");
        let failure = report(&output, "error");
        assert_eq!(failure["error"]["code"], "machine_config_container_missing");
        failures.push(failure);
    }
    assert_eq!(failures[0], failures[1]);
    assert!(!root.join("build").exists());
}

#[test]
/// CI can supply its independently approved pin through the environment; an explicit flag overrides it.
fn ci_pin_and_platform_override_do_not_rewrite_project_settings() {
    let test = TestDir::new();
    let pin = project(&test.0, false);
    let root = test.0.join("project");
    let before = fs::read(root.join("eska.toml")).expect("original settings");
    let mut cmd = command(
        &root,
        "en",
        &[
            "build",
            "--format",
            "json",
            "--platform-version",
            "8.3.27.9999",
        ],
    );
    cmd.arg("--ibcmd")
        .arg(ibcmd_fixture::executable())
        .env("ESKA_ACCEPTED_BASE_SHA256", &pin)
        .env("FAKE_IBCMD_VERSION", "8.3.27.9999");
    let value = report(&cmd.output().expect("CI build"), "success");
    assert_eq!(value["data"]["platform_version"], "8.3.27.9999");
    assert_eq!(
        fs::read(root.join("eska.toml")).expect("saved settings"),
        before
    );
    fs::remove_file(root.join("platform.log")).expect("clear previous process evidence");
    let failure = report(
        &cmd.args(["--accepted-base-sha256", &"0".repeat(64)])
            .output()
            .expect("explicit bad pin"),
        "error",
    );
    assert_eq!(failure["error"]["code"], "source_baseline_untrusted");
    assert!(!root.join("platform.log").exists());
}
