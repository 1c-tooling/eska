//! Whole-workspace CLI keeps member approvals separate and never publishes around a conflict.

use super::*;
use eska::project_core::{ProjectName, config::ProjectConfig};

/// Use the actual CLI for both layouts and preserve inherited rather than generated build settings.
fn fixture(test: &TestDir) {
    native(&test.0);
    fs::create_dir(test.0.join("native/Ext")).unwrap();
    fs::rename(
        test.0.join("native/Module.bsl"),
        test.0.join("native/Ext/ManagedApplicationModule.bsl"),
    )
    .unwrap();
    for (label, incoming) in [("old", false), ("incoming", true)] {
        let root = test.0.join(label);
        fs::create_dir_all(root.join("projects")).unwrap();
        if incoming {
            let path = test.0.join("native/Configuration.xml");
            fs::write(
                &path,
                fs::read_to_string(&path)
                    .unwrap()
                    .replace("Accepted", "Supplier update"),
            )
            .unwrap();
        }
        let mut pins = serde_json::Map::new();
        for (name, layout) in [("one", "family"), ("two", "readable")] {
            let destination = format!("{label}/projects/{name}");
            let result = success(
                &test.0,
                "en",
                &[
                    "--format",
                    "json",
                    "import",
                    "native",
                    &destination,
                    "--name",
                    name,
                    "--layout",
                    layout,
                ],
            );
            pins.insert(name.into(), result["data"]["accepted_base_sha256"].clone());
            let manifest = root.join("projects").join(name).join("eska.toml");
            let config = ProjectConfig::load(&manifest)
                .unwrap()
                .with_name(ProjectName::parse(name.into()).unwrap());
            fs::write(
                manifest,
                format!(
                    "# Retain member\r\n{}",
                    config.to_workspace_member_toml().unwrap()
                ),
            )
            .unwrap();
        }
        fs::write(root.join("eska.toml"), "# Workspace settings\r\n[workspace]\r\nmembers = ['projects/one', 'projects/two']\r\n[build]\r\nplatform_version = '8.3.27.2325'\r\nartifacts_directory = 'dist'\r\n[vcs.workflow]\r\npreset = 'trunk'\r\n").unwrap();
        fs::write(
            test.0.join(format!("{label}-pins.json")),
            serde_json::json!({"schema_version":1,"members":pins}).to_string(),
        )
        .unwrap();
    }
}

/// The same envelope and member pins are shown on RU/EN before a whole exact copy is published.
#[test]
fn workspace_base_update_preview_write_and_repeat_preserve_inheritance() {
    let test = TestDir::new();
    fixture(&test);
    let old = test.0.join("old");
    let local = b"\xef\xbb\xbf// Local workspace code\r\n";
    fs::write(
        old.join("projects/one/src/Ext/ManagedApplicationModule.bsl"),
        local,
    )
    .unwrap();
    fs::write(
        test.0
            .join("incoming/projects/one/src/Ext/ManagedApplicationModule.bsl"),
        "// Unapproved working edit\n",
    )
    .unwrap();
    let args = [
        "--format",
        "json",
        "update-base",
        "../incoming",
        "../updated",
        "--workspace",
        "--accepted-bases",
        "../old-pins.json",
        "--incoming-bases",
        "../incoming-pins.json",
    ];
    let ru = success(&old, "ru", &args);
    assert_eq!(ru, success(&old, "en", &args));
    assert_eq!(ru["data"]["scope"], "workspace");
    assert_eq!(ru["data"]["can_write"], true);
    assert_eq!(ru["data"]["members"].as_array().unwrap().len(), 2);
    assert!(!test.0.join("updated").exists());
    let write: Vec<_> = args.into_iter().chain(["--write"]).collect();
    assert_eq!(
        success(&old, "ru", &write)["data"]["publication"],
        "created"
    );
    assert_eq!(
        success(&old, "en", &write)["data"]["publication"],
        "unchanged"
    );
    let updated = test.0.join("updated");
    assert_eq!(
        fs::read(old.join("eska.toml")).unwrap(),
        fs::read(updated.join("eska.toml")).unwrap()
    );
    assert_eq!(
        fs::read(updated.join("projects/one/src/Ext/ManagedApplicationModule.bsl")).unwrap(),
        local
    );
    assert_eq!(
        fs::read(old.join("projects/one/src/Ext/ManagedApplicationModule.bsl")).unwrap(),
        local
    );
    let pins: Value =
        serde_json::from_slice(&fs::read(test.0.join("incoming-pins.json")).unwrap()).unwrap();
    for name in ["one", "two"] {
        assert_eq!(
            fs::read(old.join("projects").join(name).join("eska.toml")).unwrap(),
            fs::read(updated.join("projects").join(name).join("eska.toml")).unwrap()
        );
        let prepared = success(
            &updated,
            "en",
            &[
                "--format",
                "json",
                "prepare",
                "--project",
                name,
                "--accepted-base-sha256",
                pins["members"][name].as_str().unwrap(),
            ],
        );
        assert!(
            prepared["data"]["exclusions"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            success(
                &updated,
                "ru",
                &["--format", "json", "inspect", "--project", name]
            )["data"]["platform_version"],
            "8.3.27.2325"
        );
    }
}

/// Invalid approval and another member's overlap retain all current files and create no first output.
#[test]
fn workspace_base_update_rejects_wrong_pins_and_blocks_all_members_on_conflict() {
    let test = TestDir::new();
    fixture(&test);
    let old = test.0.join("old");
    let mut pins: Value =
        serde_json::from_slice(&fs::read(test.0.join("incoming-pins.json")).unwrap()).unwrap();
    pins["members"]["two"] = Value::String("0".repeat(64));
    fs::write(test.0.join("wrong-pins.json"), pins.to_string()).unwrap();
    for locale in ["ru", "en"] {
        let out = tool(
            &old,
            locale,
            &[
                "--format",
                "json",
                "update-base",
                "../incoming",
                "../refused",
                "--workspace",
                "--accepted-bases",
                "../old-pins.json",
                "--incoming-bases",
                "../wrong-pins.json",
                "--write",
            ],
        );
        assert_eq!(out.status.code(), Some(1));
        assert_eq!(
            serde_json::from_slice::<Value>(&out.stdout).unwrap()["error"]["code"],
            "source_baseline_untrusted"
        );
        assert!(!test.0.join("refused").exists());
    }
    let root = old.join("projects/two/src/configuration.yaml");
    fs::write(
        &root,
        fs::read_to_string(&root)
            .unwrap()
            .replace("Accepted", "Local conflict"),
    )
    .unwrap();
    let args = [
        "--format",
        "json",
        "update-base",
        "../incoming",
        "../refused",
        "--workspace",
        "--accepted-bases",
        "../old-pins.json",
        "--incoming-bases",
        "../incoming-pins.json",
    ];
    let preview = success(&old, "ru", &args);
    assert_eq!(preview, success(&old, "en", &args));
    assert_eq!(preview["data"]["can_write"], false);
    let write: Vec<_> = args.into_iter().chain(["--write"]).collect();
    for locale in ["ru", "en"] {
        let output = tool(&old, locale, &write);
        assert_eq!(output.status.code(), Some(1));
        let doc: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(doc["error"]["code"], "source_update_conflicts");
        assert_eq!(
            doc["error"]["diagnostics"]["members"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(!test.0.join("refused").exists());
    }
}
