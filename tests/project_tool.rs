//! The independently compiled project tool requires neither the facade nor native runners.

#[path = "project_tool/creation.rs"]
mod creation;
#[path = "project_tool/deletion.rs"]
mod deletion;
#[path = "project_tool/migration.rs"]
mod migration;
#[path = "project_tool/order.rs"]
mod order;
#[path = "project_tool/readable.rs"]
mod readable;
#[path = "project_tool/reference_clear.rs"]
mod reference_clear;
#[path = "project_tool/rename.rs"]
mod rename;
mod support;

use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use support::TestDir;

/// Run the actual standalone binary with deterministic locale and no inherited approval pin.
fn tool(root: &Path, locale: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_eska-project"))
        .current_dir(root)
        .env_remove("ESKA_LANG")
        .env_remove("ESKA_ACCEPTED_BASE_SHA256")
        .args(["--lang", locale])
        .args(args)
        .output()
        .expect("standalone CLI")
}

/// Require the stable envelope and a successful process before inspecting its command data.
fn success(root: &Path, locale: &str, args: &[&str]) -> Value {
    let output = tool(root, locale, args);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    let result: Value = serde_json::from_slice(&output.stdout).expect("JSON success");
    assert_eq!(result["schema_version"], 1);
    assert_eq!(result["tool"], "eska-project");
    assert_eq!(result["status"], "success");
    result
}

/// Keep fixture bytes native while testing the project tool's actual filesystem boundaries.
fn native(root: &Path) {
    fs::create_dir(root.join("native")).expect("native source");
    fs::write(root.join("native/Configuration.xml"),
        "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Configuration uuid=\"10000000-0000-4000-8000-000000000001\"><Properties><Name>NativeSample</Name><Comment>Accepted</Comment><Version>1.2.3.4</Version></Properties></Configuration></MetaDataObject>"
    ).expect("configuration");
    fs::write(root.join("native/Module.bsl"), b"\xef\xbb\xbf// native\r\n").expect("BSL");
    fs::create_dir(root.join("native/Empty")).expect("empty directory");
}

/// Create one test project through the public command rather than a parallel test-only importer.
fn import(root: &Path, destination: &str, name: &str) -> String {
    let output = success(
        root,
        "ru",
        &[
            "--format",
            "json",
            "import",
            "native",
            destination,
            "--name",
            name,
        ],
    );
    assert_eq!(output["command"], "import");
    output["data"]["accepted_base_sha256"]
        .as_str()
        .expect("candidate pin")
        .to_owned()
}

#[test]
/// Help is localized in both languages, including each command; capabilities are locale-free.
fn help_and_capabilities_work_without_a_project() {
    let test = TestDir::new();
    let mut reports = Vec::new();
    for (locale, phrase) in [
        ("ru", "Операции проекта ESKA"),
        ("en", "Offline ESKA project"),
    ] {
        let output = tool(&test.0, locale, &["--help"]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stderr.is_empty());
        assert!(String::from_utf8_lossy(&output.stdout).contains(phrase));
        for command in [
            "import",
            "migrate",
            "inspect",
            "prepare",
            "transport",
            "capabilities",
        ] {
            let output = tool(&test.0, locale, &[command, "--help"]);
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert!(output.stderr.is_empty());
        }
        reports.push(success(
            &test.0,
            locale,
            &["capabilities", "--format", "json"],
        ));
    }
    assert_eq!(reports[0], reports[1]);
    assert_eq!(reports[0]["data"]["platform_required"], false);
    assert_eq!(reports[0]["data"]["git_required"], false);
    assert_eq!(
        reports[0]["data"]["import_layouts"],
        serde_json::json!(["family", "readable"])
    );
    assert!(!test.0.join("eska.toml").exists());
}

#[test]
/// Offline import preserves native identities; inspect and prepare share the configured source.
fn import_inspect_prepare_and_cache_recreation_use_one_project() {
    let test = TestDir::new();
    native(&test.0);
    let before = fs::read(test.0.join("native/Configuration.xml")).expect("input");
    let pin = import(&test.0, "project", "project-label");
    let root = test.0.join("project");
    let first = success(&root, "ru", &["inspect", "--format", "json"]);
    assert_eq!(
        first,
        success(&root, "en", &["inspect", "--format", "json"])
    );
    assert_eq!(first["data"]["source_format"], "eska");
    assert_eq!(first["data"]["metadata"]["name"], "NativeSample");
    assert_eq!(
        first["data"]["metadata"]["validation"],
        "manifest_and_configuration_only"
    );
    let args = [
        "prepare",
        "--accepted-base-sha256",
        &pin,
        "--format",
        "json",
    ];
    let prepared = success(&root, "ru", &args);
    assert_eq!(prepared, success(&root, "en", &args));
    let xml = Path::new(prepared["data"]["xml"].as_str().expect("prepared XML"));
    assert!(xml.join("Empty").is_dir());
    assert_eq!(
        fs::read(xml.join("Module.bsl")).expect("BSL"),
        b"\xef\xbb\xbf// native\r\n"
    );
    fs::remove_dir_all(root.join(".eska/generations")).expect("delete only own cache");
    assert_eq!(prepared, success(&root, "en", &args));
    assert_eq!(
        fs::read(test.0.join("native/Configuration.xml")).expect("unchanged input"),
        before
    );
    assert!(!root.join(".git").exists());
    assert!(!root.join("build").exists());
}

#[test]
/// Missing/wrong pins and bad YAML refuse materialization; old XML cannot supply a fallback.
fn refusals_are_localized_and_machine_errors_are_locale_free() {
    let test = TestDir::new();
    native(&test.0);
    let pin = import(&test.0, "project", "sample");
    let root = test.0.join("project");
    for (args, code) in [
        (
            vec!["prepare", "--format", "json"],
            "accepted_base_pin_required",
        ),
        (
            vec![
                "prepare",
                "--accepted-base-sha256",
                "invalid",
                "--format",
                "json",
            ],
            "accepted_base_pin_invalid",
        ),
    ] {
        let mut errors = Vec::new();
        for (locale, message) in [("ru", "Операция не выполнена"), ("en", "Operation failed")]
        {
            let output = tool(&root, locale, &args);
            assert_eq!(output.status.code(), Some(1), "{output:?}");
            assert!(String::from_utf8_lossy(&output.stderr).contains(message));
            let value: Value = serde_json::from_slice(&output.stdout).expect("JSON error");
            assert_eq!(value["error"]["code"], code);
            errors.push(value);
        }
        assert_eq!(errors[0], errors[1]);
    }
    let wrong = "0".repeat(64);
    let output = tool(
        &root,
        "ru",
        &[
            "prepare",
            "--accepted-base-sha256",
            &wrong,
            "--format",
            "json",
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    let value: Value = serde_json::from_slice(&output.stdout).expect("wrong pin error");
    assert_eq!(value["error"]["code"], "source_baseline_untrusted");
    fs::copy(
        test.0.join("native/Configuration.xml"),
        root.join("src/Configuration.xml"),
    )
    .expect("leftover XML");
    fs::write(root.join("src/configuration.yaml"), b"broken: [").expect("bad YAML");
    let output = tool(
        &root,
        "en",
        &[
            "prepare",
            "--accepted-base-sha256",
            &pin,
            "--format",
            "json",
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    let value: Value = serde_json::from_slice(&output.stdout).expect("YAML error");
    assert_eq!(value["error"]["code"], "source_yaml_invalid");
    assert!(!root.join(".eska/generations").exists());
}

#[test]
/// Preview never writes; explicit publication keeps unrelated rules and reports its scope.
fn transport_preview_and_write_keep_user_attributes() {
    let test = TestDir::new();
    native(&test.0);
    import(&test.0, "project", "sample");
    let root = test.0.join("project");
    let original = "* text=auto\r\n# user comment\r\n";
    fs::write(root.join(".gitattributes"), original).expect("user attributes");
    let preview = success(&root, "ru", &["transport", "--format", "json"]);
    assert_eq!(preview["data"]["changed"], true);
    assert_eq!(preview["data"]["written"], false);
    assert_eq!(preview["data"]["effective_git_policy_verified"], false);
    assert_eq!(
        fs::read_to_string(root.join(".gitattributes")).expect("unchanged attributes"),
        original
    );
    let published = success(&root, "en", &["transport", "--write", "--format", "json"]);
    assert_eq!(published["data"]["written"], true);
    assert_eq!(preview["data"]["content"], published["data"]["content"]);
    assert_eq!(
        fs::read_to_string(root.join(".gitattributes")).expect("published attributes"),
        published["data"]["content"].as_str().expect("content")
    );
}

#[test]
/// One workspace member can be read even when another member's YAML is invalid.
fn workspace_selection_does_not_open_unselected_yaml() {
    let test = TestDir::new();
    native(&test.0);
    import(&test.0, "first", "first");
    import(&test.0, "other", "other");
    fs::write(
        test.0.join("eska.toml"),
        "[workspace]\nmembers=['first','other']\n[build]\nplatform_version='8.3.27.2325'\n",
    )
    .expect("workspace");
    fs::write(test.0.join("other/src/configuration.yaml"), b"broken: [").expect("unselected YAML");
    let first = success(
        &test.0,
        "ru",
        &["inspect", "--project", "first", "--format", "json"],
    );
    assert_eq!(
        first,
        success(
            &test.0,
            "en",
            &["inspect", "--project", "first", "--format", "json"]
        )
    );
    assert_eq!(first["data"]["platform_version"], "8.3.27.2325");
    assert_eq!(
        Path::new(first["data"]["root"].as_str().expect("selected root")),
        test.0.join("first")
    );
    let output = tool(&test.0, "en", &["inspect", "--format", "json"]);
    assert_eq!(output.status.code(), Some(1));
    let error: Value = serde_json::from_slice(&output.stdout).expect("selection error");
    assert_eq!(error["error"]["code"], "project_selection_failed");
}

#[test]
/// Invalid arguments and an occupied destination have explicit failure envelopes and no writes.
fn argument_and_existing_destination_failures_leave_files_intact() {
    let test = TestDir::new();
    native(&test.0);
    let output = tool(
        &test.0,
        "ru",
        &["import", "native", "project", "--format", "json"],
    );
    assert_eq!(output.status.code(), Some(1));
    let error: Value = serde_json::from_slice(&output.stdout).expect("argument error");
    assert_eq!(error["command"], Value::Null);
    assert_eq!(error["error"]["code"], "arguments_invalid");
    assert!(!test.0.join("project").exists());
    fs::create_dir(test.0.join("project")).expect("existing user directory");
    fs::write(test.0.join("project/sentinel"), b"user bytes").expect("sentinel");
    let output = tool(
        &test.0,
        "en",
        &[
            "import", "native", "project", "--name", "sample", "--format", "json",
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    let error: Value = serde_json::from_slice(&output.stdout).expect("destination error");
    assert_eq!(error["error"]["code"], "source_destination_exists");
    assert_eq!(
        fs::read(test.0.join("project/sentinel")).expect("preserved file"),
        b"user bytes"
    );
}

#[test]
/// Strict CLI export refuses frozen edits; explicit recovery restores accepted bytes and reports it.
fn frozen_edits_require_explicit_exclusion_mode_and_keep_working_bytes() {
    let test = TestDir::new();
    native(&test.0);
    let imported = success(
        &test.0,
        "en",
        &[
            "import",
            "native",
            "project",
            "--name",
            "sample",
            "--freeze",
            "10000000-0000-4000-8000-000000000001",
            "--format",
            "json",
        ],
    );
    let pin = imported["data"]["accepted_base_sha256"]
        .as_str()
        .expect("pin");
    let root = test.0.join("project");
    let owner = root.join("src/configuration.yaml");
    let original = fs::read_to_string(&owner).expect("YAML");
    let working = original.replace("\"Accepted\"", "\"Working\"");
    assert_ne!(original, working);
    fs::write(&owner, &working).expect("direct frozen edit");
    let output = tool(
        &root,
        "ru",
        &["prepare", "--accepted-base-sha256", pin, "--format", "json"],
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let error: Value = serde_json::from_slice(&output.stdout).expect("protected refusal");
    assert_eq!(error["error"]["code"], "source_changes_excluded");
    assert!(
        !error["error"]["diagnostics"]
            .as_array()
            .expect("complete exclusions")
            .is_empty()
    );
    assert!(!root.join(".eska/generations").exists());
    let restored = success(
        &root,
        "en",
        &[
            "prepare",
            "--accepted-base-sha256",
            pin,
            "--allow-exclusions",
            "--format",
            "json",
        ],
    );
    assert!(
        !restored["data"]["exclusions"]
            .as_array()
            .expect("reported exclusions")
            .is_empty()
    );
    let xml = Path::new(restored["data"]["xml"].as_str().expect("XML")).join("Configuration.xml");
    let xml = fs::read_to_string(xml).expect("native XML");
    assert!(xml.contains("<Comment>Accepted</Comment>"));
    assert!(!xml.contains("<Comment>Working</Comment>"));
    assert_eq!(
        fs::read_to_string(owner).expect("unmodified working source"),
        working
    );
}

#[test]
/// A protected environment pin works alone; an explicit flag has precedence and remains checked.
fn independent_environment_pin_and_explicit_precedence() {
    let test = TestDir::new();
    native(&test.0);
    let pin = import(&test.0, "project", "sample");
    let root = test.0.join("project");
    let wrong = "0".repeat(64);
    let output = Command::new(env!("CARGO_BIN_EXE_eska-project"))
        .current_dir(&root)
        .env("ESKA_ACCEPTED_BASE_SHA256", &pin)
        .args([
            "--lang",
            "en",
            "prepare",
            "--accepted-base-sha256",
            &wrong,
            "--format",
            "json",
        ])
        .output()
        .expect("explicit pin");
    assert_eq!(output.status.code(), Some(1));
    let error: Value = serde_json::from_slice(&output.stdout).expect("flag precedence");
    assert_eq!(error["error"]["code"], "source_baseline_untrusted");
    assert!(!root.join(".eska/generations").exists());
    let output = Command::new(env!("CARGO_BIN_EXE_eska-project"))
        .current_dir(&root)
        .env("ESKA_ACCEPTED_BASE_SHA256", &pin)
        .args(["--lang", "ru", "prepare", "--format", "json"])
        .output()
        .expect("environment pin");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let value: Value = serde_json::from_slice(&output.stdout).expect("prepared output");
    assert_eq!(value["data"]["baseline"], pin);
}

#[cfg(unix)]
#[test]
/// A relative path under a non-UTF-8 working directory fails before claiming a project directory.
fn unsupported_root_encoding_is_rejected_before_creation() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let test = TestDir::new();
    let root = test.0.join(OsString::from_vec(vec![0xff]));
    fs::create_dir(&root).expect("non-UTF-8 cwd");
    native(&root);
    let output = tool(
        &root,
        "en",
        &[
            "import", "native", "project", "--name", "sample", "--format", "json",
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    let error: Value = serde_json::from_slice(&output.stdout).expect("encoding refusal");
    assert_eq!(error["error"]["code"], "path_encoding_unsupported");
    assert!(!root.join("project").exists());
}
