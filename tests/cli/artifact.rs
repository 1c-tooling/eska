//! Native-artifact CLI contracts exercised through a portable platform process.

use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

use crate::support::TestDir;

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
