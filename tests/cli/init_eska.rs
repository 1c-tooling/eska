//! Main init attaches existing authoring through the real companion with normal shell onboarding.

mod fixture;
mod guarded;
mod workspace;

use eska::{
    config::ProjectConfig,
    project_core::{
        ProjectType,
        discovery::discover,
        metadata_model::ProjectScope,
        source_format::{
            AcceptedBase, AcceptedRef, ImportAncestorRef, prepare_project_source_import,
            project_import_ancestor,
        },
    },
    vcs::workflow::WorkflowPreset,
};
use fixture::{Fixture, seed, snapshot, success, unmanaged};
use serde_json::Value;
use std::fs;

/// Every root type accepts a nested source without platform/Git and returns a stable locale-free schema.
#[test]
fn attaches_four_profiles_without_rewriting_sources_and_keeps_json_locale_free() {
    for kind in [
        ProjectType::Configuration,
        ProjectType::Extension,
        ProjectType::Processing,
        ProjectType::Report,
    ] {
        let fixture = Fixture::new();
        fixture.project_tool();
        let seed = seed(&fixture, kind, "1.2.3");
        let original = snapshot(seed.project.source());
        let mut reports = Vec::new();
        for (locale, name) in [("ru", "project-ru"), ("en", "project-en")] {
            let root = unmanaged(&fixture, &seed, name, "authoring/source with spaces");
            let output = fixture
                .command(locale)
                .args([
                    "init",
                    name,
                    "--source-format",
                    "eska",
                    "--source",
                    "authoring/source with spaces",
                    "--name",
                    "sample",
                    "--workflow",
                    "git-flow",
                    "--no-vcs",
                    "--format",
                    "json",
                ])
                .output()
                .unwrap();
            let mut data = success(&output);
            assert_eq!(data["type"], kind.as_str());
            assert_eq!(data["metadata_name"], "Объект");
            assert_eq!(data["version"], "1.2.3");
            assert_eq!(data["accepted_base_created"], true);
            assert_eq!(data["ancestry_preserved"], false);
            assert_eq!(data["existing_root"], true);
            assert_eq!(data["project_initialized"], true);
            assert_eq!(data["onboarding_complete"], true);
            assert_eq!(data["source_verified"], true);
            assert_eq!(data["support_approved"], false);
            assert_eq!(
                data["source"],
                root.join("authoring/source with spaces").to_str().unwrap()
            );
            assert_eq!(
                snapshot(&root.join("authoring/source with spaces")),
                original
            );
            assert_eq!(fs::read(root.join("user.bin")).unwrap(), [0, 255, 13, 10]);
            assert_eq!(
                fs::read(root.join(".gitignore")).unwrap(),
                b"# User ignore\r\nprivate/\r\n"
            );
            assert!(!root.join(".git").exists());
            let config = ProjectConfig::load(&root.join("eska.toml")).unwrap();
            assert_eq!(
                config.configuration().workflow(),
                Some(WorkflowPreset::GitFlow)
            );
            let project = discover(&root).unwrap();
            let pin = AcceptedRef {
                sha256: data["accepted_base_sha256"].as_str().unwrap().into(),
            };
            AcceptedBase::open(&root.join(".eska/accepted"), &pin).unwrap();
            let ancestor =
                ImportAncestorRef::from_sha256(data["ancestor_sha256"].as_str().unwrap()).unwrap();
            project_import_ancestor(&project, &ProjectScope::Standalone, &ancestor).unwrap();
            data["root"] = Value::Null;
            data["source"] = Value::Null;
            reports.push(data);
        }
        assert_eq!(reports[0], reports[1]);
    }
}

/// Old accepted/initial evidence requires an external pin and survives attachment byte-for-byte.
#[test]
fn preserves_existing_evidence_user_git_and_human_output_in_both_languages() {
    for (locale, phrase) in [
        ("ru", "Исходники ESKA подключены"),
        ("en", "ESKA sources attached"),
    ] {
        let fixture = Fixture::new();
        fixture.project_tool();
        let imported = seed(&fixture, ProjectType::Configuration, "1.2.3");
        let root = imported.project.root();
        let controls = snapshot(&root.join(".eska"));
        fs::remove_file(root.join("eska.toml")).unwrap();
        gix::ThreadSafeRepository::init_opts(
            root,
            gix::create::Kind::WithWorktree,
            gix::create::Options::default(),
            gix::open::Options::isolated(),
        )
        .unwrap();
        fs::write(root.join(".git/user-marker"), "existing Git bytes").unwrap();
        let git = snapshot(&root.join(".git"));
        let output = fixture
            .command(locale)
            .arg("init")
            .arg(root)
            .args([
                "--source-format",
                "eska",
                "--workflow",
                "trunk",
                "--accepted-base-sha256",
                &imported.accepted.sha256,
            ])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(String::from_utf8_lossy(&output.stdout).contains(phrase));
        assert!(String::from_utf8_lossy(&output.stderr).contains("SHA-256"));
        assert_eq!(snapshot(&root.join(".eska")), controls);
        assert_eq!(snapshot(&root.join(".git")), git);
    }
}

/// Published raw history keeps its old independent pin while local authoring bytes survive attachment.
#[test]
fn published_history_and_local_working_bytes_survive_main_attachment() {
    let fixture = Fixture::new();
    fixture.project_tool();
    let imported = seed(&fixture, ProjectType::Configuration, "1.2.3");
    let root = imported.project.root();
    let reference = imported.initial_ancestor.as_ref().unwrap();
    let ancestor =
        project_import_ancestor(&imported.project, &ProjectScope::Standalone, reference).unwrap();
    let owner = imported.project.source().join("configuration.yaml");
    let mut bytes = fs::read(&owner).unwrap();
    bytes.extend_from_slice(b"\n# local authoring bytes\n");
    fs::write(&owner, &bytes).unwrap();
    prepare_project_source_import(
        &imported.project,
        &ProjectScope::Standalone,
        &imported.accepted,
        &ancestor,
        &ancestor,
    )
    .unwrap()
    .publish()
    .unwrap();
    let state: Value =
        serde_json::from_slice(&fs::read(root.join(".eska/source-import/state.json")).unwrap())
            .unwrap();
    assert_eq!(state["schema_version"], 1);
    let controls = snapshot(&root.join(".eska"));
    fs::remove_file(root.join("eska.toml")).unwrap();
    let output = fixture
        .command("en")
        .arg("init")
        .arg(root)
        .args([
            "--source-format",
            "eska",
            "--workflow",
            "trunk",
            "--no-vcs",
            "--accepted-base-sha256",
            &imported.accepted.sha256,
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let data = success(&output);
    assert_eq!(data["accepted_base_created"], false);
    assert_eq!(data["ancestry_preserved"], true);
    assert_eq!(data["ancestor_sha256"], reference.sha256());
    assert_eq!(snapshot(&root.join(".eska")), controls);
    assert_eq!(fs::read(&owner).unwrap(), bytes);
}

/// Invalid options, missing independent support and incompatible flags fail before manifest publication.
#[test]
fn option_help_and_preflight_failures_are_localized_without_touching_inputs() {
    let fixture = Fixture::new();
    fixture.project_tool();
    let imported = seed(&fixture, ProjectType::Report, "1.2.3");
    let root = imported.project.root();
    fs::remove_file(root.join("eska.toml")).unwrap();
    let before = snapshot(root);
    for locale in ["ru", "en"] {
        let help = fixture
            .command(locale)
            .args(["init", "--help"])
            .output()
            .unwrap();
        assert_eq!(help.status.code(), Some(0));
        assert!(String::from_utf8_lossy(&help.stdout).contains("--source-format"));
        assert!(String::from_utf8_lossy(&help.stdout).contains("--accepted-base-sha256"));
        for flags in [
            vec!["--workflow", "trunk"],
            vec!["--workflow", "trunk", "--ibcmd", "unused"],
            vec!["--workflow", "trunk", "--source", "../outside"],
        ] {
            let output = fixture
                .command(locale)
                .arg("init")
                .arg(root)
                .args(["--source-format", "eska", "--format", "json"])
                .args(flags)
                .output()
                .unwrap();
            assert!(!output.status.success(), "{output:?}");
            let value: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(value["status"], "error");
            assert_eq!(snapshot(root), before);
        }
        let output = fixture
            .command(locale)
            .arg("init")
            .arg(root)
            .args(["--format", "json"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            value["error"]["code"],
            "init-eska-source-options-incompatible"
        );
    }
}
