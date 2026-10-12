//! The complete workspace uses explicit per-member approval and retained manifest inheritance.

use super::*;
use eska::project_core::{
    discovery::{DiscoveryContext, discover_context},
    source_format::{AcceptedBase, AcceptedRef, WORKSPACE_LAYOUT_RECEIPT},
};

/// Create two independently pinned members with different native origins and layout versions.
fn fixture(test: &TestDir) -> (std::path::PathBuf, std::path::PathBuf, Vec<String>) {
    native(&test.0);
    let old = test.0.join("old");
    fs::create_dir_all(old.join("projects")).unwrap();
    let first = import(&test.0, "old/projects/one", "one");
    let path = test.0.join("native/Configuration.xml");
    let content = fs::read_to_string(&path)
        .unwrap()
        .replace("NativeSample", "SecondSample");
    fs::write(path, content).unwrap();
    let second = success(
        &test.0,
        "en",
        &[
            "import",
            "native",
            "old/projects/two",
            "--name",
            "two",
            "--layout",
            "readable",
            "--platform-version",
            "8.3.26.1000",
            "--format",
            "json",
        ],
    )["data"]["accepted_base_sha256"]
        .as_str()
        .unwrap()
        .to_owned();
    let path = old.join("projects/one/eska.toml");
    let content = fs::read_to_string(&path)
        .unwrap()
        .replace("\n[build]\nplatform_version = \"\"\n", "\n");
    fs::write(path, format!("# Inherit root\r\n{content}")).unwrap();
    fs::write(old.join("eska.toml"),
        "# Exact root settings\r\n[workspace]\r\nmembers = ['projects/one', 'projects/two']\r\n[build]\r\nplatform_version = '8.3.27.2325'\r\nartifacts_directory = 'dist'\r\n[vcs.workflow]\r\npreset = 'trunk'\r\n"
    ).unwrap();
    let approved = test.0.join("approved.json");
    fs::write(
        &approved,
        serde_json::json!({"schema_version":1, "members":{"one":first,"two":second}}).to_string(),
    )
    .unwrap();
    (old, approved, vec![first, second])
}

/// Preview/write/repeat work from root and nested paths without detaching members or approving CI.
#[test]
fn workspace_migration_preview_and_write_preserve_settings_and_working_values() {
    let test = TestDir::new();
    let (old, approved, pins) = fixture(&test);
    let authored = old.join("projects/one/src/configuration.yaml");
    let content = fs::read_to_string(&authored)
        .unwrap()
        .replace("Accepted", "Working workspace");
    fs::write(&authored, content).unwrap();
    fs::write(old.join("notes.md"), "User notes").unwrap();
    let settings = fs::read(old.join("eska.toml")).unwrap();
    let output = test.0.join("new");
    let args = [
        "migrate",
        output.to_str().unwrap(),
        "--workspace",
        "--accepted-bases",
        approved.to_str().unwrap(),
        "--format",
        "json",
    ];
    let previews = [
        success(&old, "ru", &args),
        success(&old.join("projects/one/src"), "en", &args),
    ];
    assert_eq!(previews[0], previews[1]);
    assert!(!output.exists());
    assert_eq!(previews[0]["data"]["scope"], "workspace");
    assert_eq!(previews[0]["data"]["members"].as_array().unwrap().len(), 2);
    assert_eq!(previews[0]["data"]["publication"], Value::Null);
    let mut write = args.to_vec();
    write.push("--write");
    let report = success(&old, "ru", &write);
    assert_eq!(report["data"]["publication"], "created");
    assert_eq!(report["data"]["members"], previews[0]["data"]["members"]);
    assert_eq!(
        success(&old, "en", &write)["data"]["publication"],
        "unchanged"
    );
    assert_eq!(fs::read(output.join("eska.toml")).unwrap(), settings);
    assert_eq!(fs::read(old.join("eska.toml")).unwrap(), settings);
    assert!(!output.join("notes.md").exists());
    let DiscoveryContext::Workspace { workspace, .. } = discover_context(&output).unwrap() else {
        panic!("copied workspace");
    };
    assert_member_outputs(&test, &old, &workspace, &report, &pins);
    assert!(
        fs::read_to_string(output.join("projects/one/src/configuration.yaml"))
            .unwrap()
            .contains("Working workspace")
    );
    assert!(
        fs::read_to_string(authored)
            .unwrap()
            .contains("Working workspace")
    );
}

/// Check member preparation separately from copy publication, including inherited platform settings.
fn assert_member_outputs(
    test: &TestDir,
    old: &Path,
    workspace: &eska::project_core::Workspace,
    report: &Value,
    pins: &[String],
) {
    let output = workspace.root();
    let versions = ["8.3.27.2325", "8.3.26.1000"];
    for (index, member) in workspace.members().iter().enumerate() {
        assert_eq!(
            member
                .project()
                .configuration()
                .build_settings()
                .platform_version()
                .unwrap()
                .as_str(),
            versions[index]
        );
        assert_eq!(
            fs::read(member.root().join("eska.toml")).unwrap(),
            fs::read(
                old.join("projects")
                    .join(member.name().as_str())
                    .join("eska.toml")
            )
            .unwrap()
        );
        let planned = &report["data"]["members"][index];
        assert_eq!(planned["previous_baseline"], pins[index]);
        let pin = planned["baseline"].as_str().unwrap();
        let generation = success(
            output,
            "en",
            &[
                "prepare",
                "--project",
                member.name().as_str(),
                "--accepted-base-sha256",
                pin,
                "--format",
                "json",
            ],
        );
        assert_eq!(generation["data"]["exclusions"], serde_json::json!([]));
        let base = AcceptedBase::open(
            &member.root().join(".eska/accepted"),
            &AcceptedRef { sha256: pin.into() },
        )
        .unwrap();
        let restored = test.0.join(format!("restored-{index}"));
        base.restore(&restored).unwrap();
        assert!(
            !fs::read_to_string(restored.join("configuration.yaml"))
                .unwrap()
                .contains("Working workspace")
        );
    }
}

/// A missing, duplicate, extra or mismatched member pin refuses all output in both locales.
#[test]
fn workspace_migration_invalid_approval_never_creates_output() {
    let test = TestDir::new();
    let (old, approved, pins) = fixture(&test);
    let output = test.0.join("refused");
    for (content, code) in [
        ("{}".to_owned(), "source_workspace_pins_invalid"),
        (format!("{{\"schema_version\":1,\"members\":{{\"one\":\"{}\",\"one\":\"{}\"}}}}",pins[0],pins[0]),"source_workspace_pins_invalid"),
        (serde_json::json!({"schema_version":1,"members":{"one":pins[0]}}).to_string(),"source_workspace_pins_mismatch"),
        (serde_json::json!({"schema_version":1,"members":{"one":pins[0],"two":pins[1],"extra":pins[0]}}).to_string(),"source_workspace_pins_mismatch"),
        (serde_json::json!({"schema_version":1,"members":{"one":pins[0],"two":pins[0]}}).to_string(),"source_baseline_untrusted"),
    ] {
        fs::write(&approved, content).unwrap();
        let mut errors = Vec::new();
        for locale in ["ru", "en"] {
            let result = tool(&old, locale, &["migrate", output.to_str().unwrap(), "--workspace", "--accepted-bases", approved.to_str().unwrap(), "--write", "--format", "json"]);
            assert_eq!(result.status.code(), Some(1));
            assert!(!result.stderr.is_empty());
            let value: Value = serde_json::from_slice(&result.stdout).unwrap();
            assert_eq!(value["error"]["code"], code);
            errors.push(value);
        }
        assert_eq!(errors[0], errors[1]);
        assert!(!output.exists());
    }
    for args in [
        vec![
            "migrate",
            output.to_str().unwrap(),
            "--workspace",
            "--write",
            "--format",
            "json",
        ],
        vec![
            "migrate",
            output.to_str().unwrap(),
            "--workspace",
            "--project",
            "one",
            "--format",
            "json",
        ],
        vec![
            "migrate",
            output.to_str().unwrap(),
            "--accepted-bases",
            approved.to_str().unwrap(),
            "--format",
            "json",
        ],
    ] {
        let result = tool(&old, "en", &args);
        assert_eq!(result.status.code(), Some(1));
        assert!(!output.exists());
    }
}

/// Both presentation locales expose the workspace boundary and block members under an outer receipt.
#[test]
fn workspace_migration_help_human_and_readiness_cover_both_locales() {
    let test = TestDir::new();
    let (old, approved, _) = fixture(&test);
    let output = test.0.join("new");
    let args = [
        "migrate",
        output.to_str().unwrap(),
        "--workspace",
        "--accepted-bases",
        approved.to_str().unwrap(),
    ];
    for (locale, phrase) in [
        ("ru", "План читаемого workspace"),
        ("en", "Readable workspace preview"),
    ] {
        let help = tool(&old, locale, &["migrate", "--help"]);
        assert_eq!(help.status.code(), Some(0));
        let content = String::from_utf8(help.stdout).unwrap();
        assert!(content.contains("--workspace") && content.contains("--accepted-bases"));
        let preview = tool(&old, locale, &args);
        assert_eq!(preview.status.code(), Some(0));
        assert!(preview.stderr.is_empty());
        assert!(String::from_utf8(preview.stdout).unwrap().contains(phrase));
        assert!(!output.exists());
    }
    let mut write = args.to_vec();
    write.extend(["--write", "--format", "json"]);
    let report = success(&old, "en", &write);
    fs::write(output.join(WORKSPACE_LAYOUT_RECEIPT), "incomplete").unwrap();
    for locale in ["ru", "en"] {
        for member in ["one", "two"] {
            let result = tool(
                &output.join("projects").join(member),
                locale,
                &["inspect", "--format", "json"],
            );
            assert_eq!(result.status.code(), Some(1));
            let value: Value = serde_json::from_slice(&result.stdout).unwrap();
            assert_eq!(value["error"]["code"], "source_migration_incomplete");
        }
    }
    let baseline = report["data"]["members"][0]["baseline"].as_str().unwrap();
    let result = tool(
        &output,
        "en",
        &[
            "prepare",
            "--project",
            "one",
            "--accepted-base-sha256",
            baseline,
            "--format",
            "json",
        ],
    );
    assert_eq!(result.status.code(), Some(1));
    let value: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["error"]["code"], "source_migration_incomplete");
}
