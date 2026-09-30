//! Native-artifact CLI contracts exercised through a portable platform process.

use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

use crate::support::TestDir;
use serde_json::Value;

const UUID: &str = "12345678-1234-1234-1234-123456789012";

/// Carry a controlled root descriptor through the fake binary unpacking boundary.
fn artifact(root: &Path, extension: &str, tag: &str, name: &str, uuid: &str) -> std::path::PathBuf {
    let path = root.join(format!("incoming.{extension}"));
    let extra = if extension == "cfe" {
        "<ConfigurationExtensionPurpose>Patch</ConfigurationExtensionPurpose>"
    } else {
        ""
    };
    fs::write(&path, format!(r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"><{tag} uuid="{uuid}"><Properties><Name>{name}</Name>{extra}</Properties></{tag}></MetaDataObject>"#)).unwrap();
    path
}

/// Run an isolated CLI with exact platform and locale choices.
fn run(root: &Path, fixture: &TestDir, locale: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(root)
        .env_remove("ESKA_LANG")
        .args(["--lang", locale])
        .args(args)
        .arg("--ibcmd")
        .arg(super::build::fake_ibcmd(fixture))
        .output()
        .unwrap()
}

/// All four types are detected from the XML content and retain native metadata names.
#[test]
fn creates_all_types_in_both_locales() {
    for locale in ["ru", "en"] {
        for (extension, tag, kind) in [
            ("cf", "Configuration", "configuration"),
            ("cfe", "Configuration", "extension"),
            ("epf", "ExternalDataProcessor", "processing"),
            ("erf", "ExternalReport", "report"),
        ] {
            let fixture = TestDir::new();
            artifact(&fixture.0, extension, tag, "ВнутреннееИмя", UUID);
            let output = run(
                &fixture.0,
                &fixture,
                locale,
                &[
                    "new",
                    "different-directory",
                    "--from",
                    &format!("incoming.{extension}"),
                    "--platform-version",
                    "8.3.27.2325",
                    "--workflow",
                    "trunk",
                    "--no-vcs",
                ],
            );
            assert!(output.status.success(), "{output:?}");
            let root = fixture.0.join("different-directory");
            let config = fs::read_to_string(root.join("eska.toml")).unwrap();
            assert!(config.contains(&format!("type = \"{kind}\"")));
            assert!(config.contains("platform_version = \"8.3.27.2325\""));
            assert!(!root.join("src/.gitkeep").exists());
            assert!(!root.join(".git").exists());
            assert!(
                fs::read_to_string(root.join(if matches!(extension, "epf" | "erf") {
                    "src/incoming.xml"
                } else {
                    "src/Configuration.xml"
                }))
                .unwrap()
                .contains("ВнутреннееИмя")
            );
            assert!(!fs::read_dir(&fixture.0).unwrap().any(|entry| {
                entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".eska-import-")
            }));
        }
    }
}

/// Workspace onboarding inherits the version and preserves the root manifest's comments.
#[test]
fn creates_workspace_member_with_inherited_platform() {
    let fixture = TestDir::new();
    fs::write(
        fixture.0.join("eska.toml"),
        "# preserved\n[workspace]\nmembers = []\n[build]\nplatform_version = '8.3.27.2325'\n",
    )
    .unwrap();
    artifact(&fixture.0, "epf", "ExternalDataProcessor", "Пример", UUID);
    let result = run(
        &fixture.0,
        &fixture,
        "en",
        &["new", "processor", "--from", "incoming.epf"],
    );
    assert!(result.status.success(), "{result:?}");
    let config = fs::read_to_string(fixture.0.join("src/processor/eska.toml")).unwrap();
    assert!(!config.contains("platform_version"));
    assert!(!fixture.0.join("src/processor/.git").exists());
    assert!(
        fs::read_to_string(fixture.0.join("eska.toml"))
            .unwrap()
            .contains("# preserved")
    );
}

/// Invalid XML, destination collisions and missing platform choices leave no project behind.
#[test]
fn rejects_invalid_creation_without_partial_files() {
    let fixture = TestDir::new();
    fs::write(fixture.0.join("incoming.cf"), "not xml").unwrap();
    for extra in [vec![], vec!["--platform-version", "8.3.27.2325"]] {
        let mut args = vec![
            "new",
            "project",
            "--from",
            "incoming.cf",
            "--workflow",
            "trunk",
        ];
        args.extend(extra);
        let output = run(&fixture.0, &fixture, "en", &args);
        assert!(!output.status.success());
        assert!(!fixture.0.join("project").exists());
    }
    fs::create_dir(fixture.0.join("project")).unwrap();
    fs::write(fixture.0.join("project/keep"), "user data").unwrap();
    let output = run(
        &fixture.0,
        &fixture,
        "en",
        &[
            "new",
            "project",
            "--from",
            "incoming.cf",
            "--workflow",
            "trunk",
            "--platform-version",
            "8.3.27.2325",
        ],
    );
    assert!(!output.status.success());
    assert_eq!(
        fs::read_to_string(fixture.0.join("project/keep")).unwrap(),
        "user data"
    );
}

/// Initialize a small real repository without global identity or detached maintenance.
fn git(root: &Path, args: &[&str]) {
    let result = Command::new("git")
        .current_dir(root)
        .args([
            "-c",
            "maintenance.auto=false",
            "-c",
            "user.name=Artifact Test",
            "-c",
            "user.email=artifact@example.invalid",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(result.status.success(), "{result:?}");
}

/// Create the same starting project for source replacement scenarios.
fn imported_project(fixture: &TestDir) -> std::path::PathBuf {
    artifact(&fixture.0, "cf", "Configuration", "Current", UUID);
    let output = run(
        &fixture.0,
        fixture,
        "en",
        &[
            "new",
            "project",
            "--from",
            "incoming.cf",
            "--workflow",
            "trunk",
            "--platform-version",
            "8.3.27.2325",
            "--no-vcs",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    fixture.0.join("project")
}

/// A clean matching project updates without force and preserves config, refs and index bytes.
#[test]
fn clean_matching_import_replaces_sources_and_preserves_git() {
    let fixture = TestDir::new();
    let root = imported_project(&fixture);
    fs::write(root.join("src/old-object.txt"), "old").unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "fixture"]);
    let config = fs::read(root.join("eska.toml")).unwrap();
    let index = fs::read(root.join(".git/index")).unwrap();
    let head = fs::read(root.join(".git/HEAD")).unwrap();
    fs::write(root.join("outside-source.txt"), "preserve unrelated change").unwrap();
    let output = run(
        &root,
        &fixture,
        "en",
        &["import", "../incoming.cf", "--format", "json"],
    );
    assert!(output.status.success(), "{output:?}");
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["applied"], true);
    assert_eq!(result["local_changes"], 0);
    assert_eq!(result["incoming"]["name"], "Current");
    assert!(!root.join("src/old-object.txt").exists());
    assert!(root.join("outside-source.txt").exists());
    assert_eq!(fs::read(root.join("eska.toml")).unwrap(), config);
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(root.join(".git/HEAD")).unwrap(), head);
}

/// UUID changes and name changes each require consent, even for a clean Git working tree.
#[test]
fn identity_differences_require_force_with_locale_independent_json() {
    for (name, uuid) in [
        ("Different", UUID),
        ("Current", "87654321-4321-4321-4321-210987654321"),
    ] {
        let fixture = TestDir::new();
        let root = imported_project(&fixture);
        git(&root, &["init", "-q"]);
        git(&root, &["add", "."]);
        git(&root, &["commit", "-qm", "fixture"]);
        let original = fs::read(root.join("src/Configuration.xml")).unwrap();
        artifact(&fixture.0, "cf", "Configuration", name, uuid);
        let mut documents = Vec::new();
        for locale in ["ru", "en"] {
            let output = run(
                &root,
                &fixture,
                locale,
                &["import", "../incoming.cf", "--format", "json"],
            );
            assert!(!output.status.success(), "{output:?}");
            let document: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(document["error"]["code"], "confirmation-required");
            assert_eq!(document["preview"]["identity_changed"], true);
            documents.push(document);
            assert_eq!(
                fs::read(root.join("src/Configuration.xml")).unwrap(),
                original
            );
        }
        assert_eq!(documents[0], documents[1]);
        let output = run(
            &root,
            &fixture,
            "en",
            &["import", "../incoming.cf", "--force", "--format", "json"],
        );
        assert!(output.status.success(), "{output:?}");
    }
}

/// Local changes and Git-less populated sources cannot be discarded implicitly.
#[test]
fn local_changes_and_dry_run_preserve_sources_until_forced() {
    let fixture = TestDir::new();
    let root = imported_project(&fixture);
    let path = root.join("src/untracked.txt");
    fs::write(&path, "keep until confirmed").unwrap();
    for args in [
        vec!["import", "../incoming.cf", "--format", "json"],
        vec!["import", "../incoming.cf", "--format", "json", "--dry-run"],
    ] {
        let output = run(&root, &fixture, "en", &args);
        assert_eq!(
            output.status.success(),
            args.contains(&"--dry-run"),
            "{output:?}"
        );
        assert!(path.exists());
    }
    git(&root, &["init", "-q"]);
    git(&root, &["add", "eska.toml", "src/Configuration.xml"]);
    git(&root, &["commit", "-qm", "fixture"]);
    let output = run(
        &root,
        &fixture,
        "en",
        &["import", "../incoming.cf", "--format", "json"],
    );
    assert!(!output.status.success());
    assert!(path.exists());
    let output = run(
        &root,
        &fixture,
        "ru",
        &["import", "../incoming.cf", "--force"],
    );
    assert!(output.status.success(), "{output:?}");
    assert!(!path.exists());
}

/// Force never allows changing the project type or replacing a root-level source directory.
#[test]
fn type_mismatch_and_root_sources_are_always_rejected() {
    let fixture = TestDir::new();
    let root = imported_project(&fixture);
    artifact(&fixture.0, "erf", "ExternalReport", "Current", UUID);
    let before = fs::read(root.join("src/Configuration.xml")).unwrap();
    let output = run(
        &root,
        &fixture,
        "en",
        &["import", "../incoming.erf", "--force", "--format", "json"],
    );
    assert!(!output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["error"]["code"],
        "type-mismatch"
    );
    assert_eq!(
        fs::read(root.join("src/Configuration.xml")).unwrap(),
        before
    );
    let config = root.join("eska.toml");
    fs::write(&config, "[project]\ntype = 'configuration'\nsource = '.'\n").unwrap();
    let output = run(
        &root,
        &fixture,
        "en",
        &["import", "../incoming.cf", "--force", "--format", "json"],
    );
    assert!(!output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["error"]["code"],
        "unsafe-path"
    );
}

/// An empty ordinary scaffold can be filled without a nonexistent identity check.
#[test]
fn fills_empty_scaffold_and_selects_exactly_one_workspace_member() {
    let fixture = TestDir::new();
    let created = Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(&fixture.0)
        .args([
            "new",
            "empty",
            "--type",
            "report",
            "--workflow",
            "trunk",
            "--no-vcs",
        ])
        .output()
        .unwrap();
    assert!(created.status.success());
    artifact(&fixture.0, "erf", "ExternalReport", "Report", UUID);
    let output = run(
        &fixture.0.join("empty"),
        &fixture,
        "en",
        &[
            "import",
            "../incoming.erf",
            "--platform-version",
            "8.3.27.2325",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert!(serde_json::from_slice::<Value>(&output.stdout).unwrap()["current"].is_null());
    fs::write(
        fixture.0.join("eska.toml"),
        "[workspace]\nmembers = []\n[build]\nplatform_version = '8.3.27.2325'\n",
    )
    .unwrap();
    let created = run(
        &fixture.0,
        &fixture,
        "en",
        &["new", "report", "--from", "incoming.erf"],
    );
    assert!(created.status.success(), "{created:?}");
    let output = run(
        &fixture.0,
        &fixture,
        "en",
        &["import", "incoming.erf", "--force", "--format", "json"],
    );
    assert!(!output.status.success());
    let output = run(
        &fixture.0,
        &fixture,
        "en",
        &[
            "import",
            "incoming.erf",
            "-p",
            "report",
            "--force",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["project"],
        "report"
    );
}

/// Detect an edit made while the platform is unpacking, including same-length replacements.
#[test]
fn concurrent_source_edit_aborts_even_with_force() {
    let fixture = TestDir::new();
    let root = imported_project(&fixture);
    git(&root, &["init", "-q"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "fixture"]);
    let ignore = fs::read(root.join(".gitignore")).unwrap();
    let ready = fixture.0.join("ready");
    let proceed = fixture.0.join("continue");
    let mut child = Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(&root)
        .args([
            "--lang",
            "en",
            "import",
            "../incoming.cf",
            "--force",
            "--format",
            "json",
            "--ibcmd",
        ])
        .arg(super::build::fake_ibcmd(&fixture))
        .env("FAKE_IBCMD_IMPORT_READY", &ready)
        .env("FAKE_IBCMD_IMPORT_CONTINUE", &proceed)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !ready.exists() {
        assert!(
            child.try_wait().unwrap().is_none(),
            "import exited before readiness"
        );
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            panic!("platform fixture readiness timeout");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let status = Command::new("git")
        .current_dir(&root)
        .args(["status", "--porcelain", "--untracked-files=all"])
        .output()
        .unwrap();
    assert!(status.status.success());
    assert!(status.stdout.is_empty(), "{status:?}");
    assert_eq!(fs::read(root.join(".gitignore")).unwrap(), ignore);
    assert!(fs::read_dir(root.join(".eska/import")).unwrap().count() > 1);
    let descriptor = root.join("src/Configuration.xml");
    let changed = fs::read_to_string(&descriptor)
        .unwrap()
        .replace("Current", "Changed");
    fs::write(&descriptor, &changed).unwrap();
    fs::write(&proceed, "").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success(), "{output:?}");
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["error"]["code"],
        "source-changed"
    );
    assert_eq!(fs::read_to_string(descriptor).unwrap(), changed);
}

/// Platform errors leave the previous source tree and exact settings intact.
#[test]
fn failed_unpack_leaves_existing_project_unchanged() {
    let fixture = TestDir::new();
    let root = imported_project(&fixture);
    let before = fs::read(root.join("src/Configuration.xml")).unwrap();
    let config = fs::read(root.join("eska.toml")).unwrap();
    for failure in ["FAKE_IBCMD_FAIL_LOAD", "FAKE_IBCMD_FAIL_EXPORT"] {
        let output = Command::new(env!("CARGO_BIN_EXE_eska"))
            .current_dir(&root)
            .args([
                "import",
                "../incoming.cf",
                "--force",
                "--format",
                "json",
                "--ibcmd",
            ])
            .arg(super::build::fake_ibcmd(&fixture))
            .env(failure, "1")
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert_eq!(
            fs::read(root.join("src/Configuration.xml")).unwrap(),
            before
        );
        assert_eq!(fs::read(root.join("eska.toml")).unwrap(), config);
        assert_eq!(fs::read_dir(root.join(".eska/import")).unwrap().count(), 1);
    }
}

/// Canonical project discovery must not erase the evidence of an unsafe source symlink.
#[cfg(unix)]
#[test]
fn refuses_linked_source_even_when_it_points_inside_project() {
    let fixture = TestDir::new();
    let root = imported_project(&fixture);
    fs::rename(root.join("src"), root.join("actual")).unwrap();
    std::os::unix::fs::symlink("actual", root.join("src")).unwrap();
    let output = run(
        &root,
        &fixture,
        "en",
        &["import", "../incoming.cf", "--force", "--format", "json"],
    );
    assert!(!output.status.success(), "{output:?}");
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["error"]["code"],
        "unsafe-path"
    );
    assert!(root.join("actual/Configuration.xml").is_file());
}

/// Native exporters can rewrite dotted external filenames while configuration paths stay directories.
#[test]
fn accepts_unicode_dotted_artifact_names_without_guessing_the_descriptor() {
    for (ext, tag) in [("epf", "ExternalDataProcessor"), ("cf", "Configuration")] {
        let fixture = TestDir::new();
        let input = artifact(&fixture.0, ext, tag, "Example", UUID);
        let name = format!("Пример.xml.{ext}");
        fs::rename(input, fixture.0.join(&name)).unwrap();
        let output = run(
            &fixture.0,
            &fixture,
            "en",
            &[
                "new",
                "project",
                "--from",
                &name,
                "--workflow",
                "trunk",
                "--no-vcs",
                "--platform-version",
                "8.3.27.2325",
            ],
        );
        assert!(output.status.success(), "{output:?}");
    }
}
