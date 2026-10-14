//! Failure acknowledgment retains old inputs and newly published controls without recursive root cleanup.

use super::*;

/// The real source child finishes before each reproducible receipt, data or parent-policy failure.
fn command(fixture: &Fixture, locale: &str, mode: &str) -> std::process::Command {
    let mut command = fixture.command(locale);
    command
        .env(
            "INIT_FIXTURE_REAL_PROJECT",
            env!("CARGO_BIN_EXE_eska-project"),
        )
        .env("INIT_FIXTURE_ROOT", &fixture.0.0)
        .env("INIT_FIXTURE_MODE", mode);
    command
}

/// A failed or invalid receipt never permits deleting an existing directory or its completed init data.
#[test]
fn post_child_failures_keep_existing_roots_evidence_and_user_files() {
    for mode in [
        "manifest",
        "source",
        "accepted",
        "state",
        "git",
        "malformed",
        "failed",
    ] {
        let fixture = Fixture::new();
        let seed = seed(&fixture, ProjectType::Configuration, "1.2.3");
        let root = unmanaged(&fixture, &seed, "existing", "src");
        gix::ThreadSafeRepository::init_opts(
            &fixture.0.0,
            gix::create::Kind::WithWorktree,
            gix::create::Options::default(),
            gix::open::Options::isolated(),
        )
        .unwrap();
        fixture.guarded_tool_from(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/cli/init_eska/companion.rs"),
        );
        let output = command(&fixture, "en", mode)
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
        assert_eq!(output.status.code(), Some(1), "{mode}: {output:?}");
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["status"], "error");
        assert_eq!(
            value["diagnostics"]["destination_removed"], false,
            "{mode}: {value}"
        );
        assert_eq!(value["diagnostics"]["existing_root"], true);
        assert_eq!(value["diagnostics"]["support_approved"], false);
        assert!(root.join("eska.toml").is_file());
        assert!(root.join(".eska/accepted/accepted.json").is_file());
        assert!(
            root.join(".eska/source-import/initial/ancestor.json")
                .is_file()
        );
        assert_eq!(fs::read(root.join("user.bin")).unwrap(), [0, 255, 13, 10]);
        if mode == "git" {
            assert_eq!(
                fs::read(root.join(".git/user-marker")).unwrap(),
                b"raced Git"
            );
        }
    }
}

/// Changed root bytes remain intact and the initialized existing member remains unlisted and retained.
#[test]
fn raced_workspace_policy_keeps_the_existing_initialized_member_without_enrollment() {
    let fixture = Fixture::new();
    let seed = seed(&fixture, ProjectType::Configuration, "1.2.3");
    let root = unmanaged(&fixture, &seed, "existing", "src");
    let original = "# Root\n[workspace]\nmembers=[]\n";
    fs::write(fixture.0.0.join("eska.toml"), original).unwrap();
    fixture.guarded_tool_from(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cli/init_eska/companion.rs"),
    );
    let output = command(&fixture, "ru", "workspace")
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
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["error"]["code"], "init-eska-policy-changed");
    assert!(root.join("eska.toml").is_file());
    assert!(root.join("src/configuration.yaml").is_file());
    assert_eq!(
        fs::read_to_string(fixture.0.0.join("eska.toml")).unwrap(),
        format!("{original}\n# intervening root\n")
    );
}
