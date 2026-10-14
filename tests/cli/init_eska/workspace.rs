//! Existing-directory enrollment preserves root comments, inherited settings and version policy.

use super::*;
use eska::{
    config::WorkspaceConfig,
    project_core::discovery::{DiscoveryContext, discover_context},
};

/// A member retains its actual directory and optional platform override without nested workflow/Git.
#[test]
fn existing_member_inherits_root_policy_or_retains_an_explicit_platform_override() {
    for (locale, platform) in [("ru", None), ("en", Some("8.3.26.1000"))] {
        let fixture = Fixture::new();
        fixture.project_tool();
        let seed = seed(&fixture, ProjectType::Processing, "1.2.3.4");
        let root = unmanaged(&fixture, &seed, "objects/existing member", "working");
        let source = snapshot(&root.join("working"));
        let original = "# Preserve root\r\n[workspace]\r\nmembers=[]\r\n[version]\r\nscheme='x.y.z.build'\r\n[build]\r\nplatform_version='8.3.27.2325'\r\nartifacts_directory='dist'\r\n[vcs.workflow]\r\npreset='trunk'\r\n";
        fs::write(fixture.0.0.join("eska.toml"), original).unwrap();
        let mut command = fixture.command(locale);
        command.args([
            "init",
            "objects/existing member",
            "--source-format",
            "eska",
            "--source",
            "working",
            "--name",
            "member",
            "--format",
            "json",
        ]);
        if let Some(platform) = platform {
            command.args(["--platform-version", platform]);
        }
        let data = success(&command.output().unwrap());
        assert_eq!(data["workspace_member"], true);
        let member = fs::read_to_string(root.join("eska.toml")).unwrap();
        assert!(!member.contains("[vcs"));
        assert_eq!(member.contains("[build"), platform.is_some());
        assert!(!root.join(".git").exists());
        assert_eq!(snapshot(&root.join("working")), source);
        let updated = fs::read_to_string(fixture.0.0.join("eska.toml")).unwrap();
        assert_eq!(updated.lines().next(), Some("# Preserve root"));
        assert_eq!(
            WorkspaceConfig::from_toml(&updated).unwrap().members(),
            &[std::path::PathBuf::from("objects/existing member")]
        );
        let DiscoveryContext::Workspace { workspace, .. } = discover_context(&root).unwrap() else {
            panic!("workspace member");
        };
        let project = workspace.members()[0].project();
        assert_eq!(
            project
                .configuration()
                .build_settings()
                .platform_version()
                .unwrap()
                .as_str(),
            platform.unwrap_or("8.3.27.2325")
        );
        assert_eq!(
            project
                .configuration()
                .build_settings()
                .artifacts_directory(),
            std::path::Path::new("dist")
        );
    }
}

/// Source version policy and duplicate member names refuse before adding project control files.
#[test]
fn member_version_scheme_and_root_owned_workflow_refuse_before_child_publication() {
    let fixture = Fixture::new();
    fixture.project_tool();
    let seed = seed(&fixture, ProjectType::Configuration, "1.2.3");
    let root = unmanaged(&fixture, &seed, "existing", "src");
    let original = "[workspace]\nmembers=[]\n[version]\nscheme='x.y.z.build'\n";
    fs::write(fixture.0.0.join("eska.toml"), original).unwrap();
    let before = snapshot(&root);
    for locale in ["ru", "en"] {
        let output = fixture
            .command(locale)
            .args([
                "init",
                "existing",
                "--source-format",
                "eska",
                "--format",
                "json",
            ])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{output:?}");
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["error"]["code"], "init-eska-version-scheme");
        let output = fixture
            .command(locale)
            .args([
                "init",
                "existing",
                "--source-format",
                "eska",
                "--workflow",
                "trunk",
                "--format",
                "json",
            ])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(snapshot(&root), before);
        assert_eq!(
            fs::read_to_string(fixture.0.0.join("eska.toml")).unwrap(),
            original
        );
    }
}
