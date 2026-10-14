//! Compound new consumes real offline/native companions and never removes published child roots.

pub(super) mod fixture;
mod guarded;
#[cfg(feature = "build-cli")]
mod native;

use eska::{
    config::{ProjectConfig, WorkspaceConfig},
    project_core::{discovery, native_version},
    vcs::workflow::WorkflowPreset,
};
use fixture::{Fixture, success};
use std::fs;

#[test]
/// All four empty profiles expose candidates and retain normal standalone workflow policy.
fn empty_profiles_in_both_locales_need_no_platform_and_preserve_workflow() {
    for locale in ["ru", "en"] {
        for kind in ["configuration", "extension", "processing", "report"] {
            let fixture = Fixture::new();
            fixture.project_tool();
            let output = fixture
                .command(locale)
                .args([
                    "new",
                    "project",
                    "--source-format",
                    "eska",
                    "--type",
                    kind,
                    "--workflow",
                    "git-flow",
                    "--no-vcs",
                    "--metadata-name",
                    "Объект",
                    "--format",
                    "json",
                ])
                .output()
                .unwrap();
            let data = success(&output);
            let root = fixture.0.0.join("project");
            assert_eq!(data["type"], kind);
            assert_eq!(data["metadata_name"], "Объект");
            assert_eq!(data["version"], "0.1.0");
            assert_eq!(data["workspace_member"], false);
            assert_eq!(data["source_verified"], true);
            assert_eq!(data["onboarding_complete"], true);
            assert_eq!(data["support_approved"], false);
            assert_eq!(data["accepted_base_sha256"].as_str().unwrap().len(), 64);
            assert_eq!(data["ancestor_sha256"].as_str().unwrap().len(), 64);
            let config = ProjectConfig::load(&root.join("eska.toml")).unwrap();
            assert_eq!(
                config.configuration().workflow(),
                Some(WorkflowPreset::GitFlow)
            );
            assert!(root.join(".gitattributes").is_file());
            assert!(!root.join(".git").exists());
            assert!(!root.join("src/.gitkeep").exists());
            let project = discovery::discover(&root).unwrap();
            assert_eq!(
                native_version::project_source_version(&project)
                    .unwrap()
                    .to_string(),
                "0.1.0"
            );
        }
    }
}

#[test]
/// Member defaults honor the root scheme/platform without nested workflow/Git initialization.
fn workspace_members_inherit_policy_and_choose_the_initial_version_scheme() {
    for (scheme, version) in [("x.y.z", "0.1.0"), ("x.y.z.build", "0.1.0.0")] {
        let fixture = Fixture::new();
        fixture.project_tool();
        let manifest = format!(
            "# untouched root comment\n[workspace]\nmembers=[]\n[version]\nscheme='{scheme}'\n[build]\nplatform_version='8.3.27.2325'\n[vcs.workflow]\npreset='trunk'\n"
        );
        fs::write(fixture.0.0.join("eska.toml"), &manifest).unwrap();
        let output = fixture
            .command("en")
            .args([
                "new",
                "member",
                "--source-format",
                "eska",
                "--type",
                "processing",
                "--format",
                "json",
            ])
            .output()
            .unwrap();
        let data = success(&output);
        assert_eq!(data["version"], version);
        assert_eq!(data["workspace_member"], true);
        let member = fixture.0.0.join("src/member");
        let text = fs::read_to_string(member.join("eska.toml")).unwrap();
        assert!(!text.contains("[vcs"), "{text}");
        assert!(!text.contains("[build"), "{text}");
        assert!(!member.join(".git").exists());
        let root_text = fs::read_to_string(fixture.0.0.join("eska.toml")).unwrap();
        assert!(root_text.starts_with("# untouched root comment"));
        assert_eq!(
            WorkspaceConfig::from_toml(&root_text).unwrap().members(),
            &[std::path::PathBuf::from("src/member")]
        );
        let discovery::DiscoveryContext::Workspace { workspace, .. } =
            discovery::discover_context(&member).unwrap()
        else {
            panic!("expected member context");
        };
        let project = workspace.members()[0].project();
        assert_eq!(
            project
                .configuration()
                .build_settings()
                .platform_version()
                .unwrap()
                .as_str(),
            "8.3.27.2325"
        );
        assert_eq!(
            native_version::project_source_version(project)
                .unwrap()
                .to_string(),
            version
        );
    }
}

#[test]
/// Invalid explicit member versions and source flags are refused before any output directory claim.
fn preflight_refuses_incompatible_flags_and_nonterminal_policy() {
    let fixture = Fixture::new();
    fixture.project_tool();
    for locale in ["ru", "en"] {
        for arguments in [
            vec![
                "new",
                "project",
                "--source-format",
                "eska",
                "--type",
                "report",
                "--format",
                "json",
            ],
            vec![
                "new",
                "project",
                "--source-format",
                "eska",
                "--workflow",
                "trunk",
                "--format",
                "json",
            ],
            vec![
                "new",
                "project",
                "--source-format",
                "eska",
                "--type",
                "report",
                "--workflow",
                "trunk",
                "--initial-version",
                "bad",
                "--format",
                "json",
            ],
            vec![
                "new",
                "project",
                "--source-format",
                "eska",
                "--type",
                "report",
                "--workflow",
                "trunk",
                "--metadata-name",
                "bad/name",
                "--format",
                "json",
            ],
            vec![
                "new",
                "project",
                "--source-format",
                "eska",
                "--from",
                "incoming.cf",
                "--workflow",
                "trunk",
                "--initial-version",
                "1.2.3",
                "--format",
                "json",
            ],
            vec![
                "new",
                "project",
                "--type",
                "report",
                "--workflow",
                "trunk",
                "--initial-version",
                "1.2.3",
                "--format",
                "json",
            ],
        ] {
            let output = fixture.command(locale).args(arguments).output().unwrap();
            assert_eq!(output.status.code(), Some(2), "{output:?}");
            let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(value["status"], "error");
            assert!(output.stderr.is_empty(), "{output:?}");
            assert!(!fixture.0.0.join("project").exists());
        }
    }
}

#[test]
/// An explicit member version cannot change the root scheme or claim its source parent.
fn explicit_member_version_mismatch_refuses_before_publication() {
    let fixture = Fixture::new();
    fixture.project_tool();
    fs::write(
        fixture.0.0.join("eska.toml"),
        "[workspace]\nmembers=[]\n[version]\nscheme='x.y.z.build'\n",
    )
    .unwrap();
    let output = fixture
        .command("en")
        .args([
            "new",
            "member",
            "--source-format",
            "eska",
            "--type",
            "report",
            "--initial-version",
            "1.2.3",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(!fixture.0.0.join("src").exists());
}

#[test]
/// Standalone Git owns an exclusive reservation and ignores redirecting Git environment settings.
fn standalone_git_is_isolated_and_human_result_returns_candidate_pins() {
    for (locale, human) in [("ru", "ESKA-проект создан"), ("en", "ESKA project created")]
    {
        let fixture = Fixture::new();
        fixture.project_tool();
        let sentinel = fixture.0.0.join("foreign");
        fs::create_dir(&sentinel).unwrap();
        let output = fixture
            .command(locale)
            .env("GIT_DIR", &sentinel)
            .env("GIT_WORK_TREE", &sentinel)
            .args([
                "new",
                "project",
                "--source-format",
                "eska",
                "--type",
                "configuration",
                "--workflow",
                "trunk",
            ])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert!(
            String::from_utf8_lossy(&output.stdout).contains(human),
            "{output:?}"
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("SHA-256"));
        assert_eq!(sentinel.read_dir().unwrap().count(), 0);
        let repo =
            gix::open_opts(fixture.0.0.join("project"), gix::open::Options::isolated()).unwrap();
        assert!(repo.head().unwrap().is_unborn());
    }
}
