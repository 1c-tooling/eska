//! Queries reports describe mixed actual member versions and preserve independently held history pins.

use super::*;
use eska::project_core::{
    config::ProjectConfig,
    discovery::{DiscoveryContext, discover_context},
};
use std::collections::BTreeMap;

/// Every fixture project is created by the real import command in this test's isolated playground.
fn workspace(test: &TestDir) -> (std::path::PathBuf, BTreeMap<String, ImportAncestorRef>) {
    let root = test.0.join("old");
    fs::create_dir_all(root.join("projects")).unwrap();
    let mut pins = BTreeMap::new();
    let mut ancestors = BTreeMap::new();
    for kind in fixture::TYPES {
        let parent = test.0.join(format!("input {kind}"));
        fs::create_dir(&parent).unwrap();
        let native = fixture::native(&parent, kind);
        let destination = root.join("projects").join(kind);
        let imported = success(
            &test.0,
            "en",
            &[
                "import",
                native.input.to_str().unwrap(),
                destination.to_str().unwrap(),
                "--name",
                kind,
                "--layout",
                "queries",
                "--format",
                "json",
            ],
        );
        pins.insert(
            kind,
            imported["data"]["accepted_base_sha256"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
        ancestors.insert(
            kind.to_owned(),
            ImportAncestorRef::from_sha256(imported["data"]["ancestor_sha256"].as_str().unwrap())
                .unwrap(),
        );
        let path = destination.join("eska.toml");
        let config = ProjectConfig::load(&path).unwrap();
        fs::write(
            path,
            format!(
                "# Member {kind}\r\n{}",
                config
                    .to_workspace_member_toml()
                    .unwrap()
                    .replace("\n[build]\nplatform_version = \"\"\n", "\n")
            ),
        )
        .unwrap();
    }
    fs::write(root.join("eska.toml"),
        "# Exact root\r\n[workspace]\r\nmembers = ['projects/configuration', 'projects/extension', 'projects/processing', 'projects/report']\r\n[build]\r\nplatform_version = '8.3.27.2325'\r\nartifacts_directory = 'dist'\r\n"
    ).unwrap();
    fs::write(
        test.0.join("pins.json"),
        serde_json::json!({"schema_version":1,"members":pins}).to_string(),
    )
    .unwrap();
    (root, ancestors)
}

/// A mixed map 5/6 workspace retains every member's exact history, inherited settings and digest.
#[test]
fn queries_workspace_migration_reports_actual_versions_and_keeps_raw_history() {
    let test = TestDir::new();
    let (old, ancestors) = workspace(&test);
    let before = fixture::snapshot(&old);
    let args = [
        "migrate",
        "../new",
        "--workspace",
        "--accepted-bases",
        "../pins.json",
        "--layout",
        "queries",
        "--format",
        "json",
    ];
    let ru = success(&old, "ru", &args);
    assert_eq!(ru, success(&old, "en", &args));
    for (locale, phrase) in [
        ("ru", "План workspace Queries"),
        ("en", "Queries workspace preview"),
    ] {
        let human = tool(&old, locale, &args[..7]);
        assert_eq!(human.status.code(), Some(0));
        assert!(human.stderr.is_empty());
        assert!(String::from_utf8_lossy(&human.stdout).contains(phrase));
    }
    assert_eq!(ru["data"]["source_layout"], "queries");
    assert_eq!(
        ru["data"]["source_layout_versions"],
        serde_json::json!([5, 6])
    );
    assert!(ru["data"].get("source_layout_version").is_none());
    assert_eq!(ru["data"]["layout_changed"], false);
    let members = ru["data"]["members"].as_array().unwrap();
    assert_eq!(members.len(), 4);
    for member in members {
        let external = matches!(member["name"].as_str().unwrap(), "processing" | "report");
        assert_eq!(
            member["source_layout_version"],
            if external { 6 } else { 5 }
        );
        assert_eq!(member["ancestry_preserved"], true);
        assert_eq!(member["baseline"], member["previous_baseline"]);
    }
    assert!(!test.0.join("new").exists());
    let written = success(&old, "en", &[&args[..], &["--write"]].concat());
    assert_eq!(written["data"]["publication"], "created");
    let new = test.0.join("new");
    assert_eq!(fixture::snapshot(&new), before);
    let DiscoveryContext::Workspace { workspace, .. } = discover_context(&new).unwrap() else {
        panic!("workspace fixture");
    };
    for member in workspace.members() {
        project_import_ancestor(
            member.project(),
            &ProjectScope::Member(member.name().clone()),
            &ancestors[member.name().as_str()],
        )
        .unwrap();
        assert_eq!(
            member
                .project()
                .configuration()
                .build_settings()
                .platform_version()
                .unwrap()
                .as_str(),
            "8.3.27.2325"
        );
    }
    let repeated = success(&old, "ru", &[&args[..], &["--write"]].concat());
    assert_eq!(repeated["data"]["publication"], "unchanged");
    for (locale, phrase) in [
        ("ru", "Workspace Queries записан"),
        ("en", "Queries workspace published"),
    ] {
        let human = tool(&old, locale, &[&args[..7], &["--write"]].concat());
        assert_eq!(human.status.code(), Some(0));
        assert!(human.stderr.is_empty());
        assert!(String::from_utf8_lossy(&human.stdout).contains(phrase));
    }
    assert_eq!(fixture::snapshot(&old), before);
}
