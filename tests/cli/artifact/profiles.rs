//! Registered export must preserve the file's identity regardless of its suffix.

use super::*;

/// Both configuration profiles choose their loading mode from the proven descriptor.
#[test]
fn imports_configuration_profiles_under_misleading_suffixes_in_both_locales() {
    for locale in ["ru", "en"] {
        for (original, suffix, expected) in [
            ("cf", "cfe", "configuration"),
            ("cfe", "cf", "extension"),
            ("cfe", "erf", "extension"),
        ] {
            let fixture = TestDir::new();
            let input = artifact(&fixture.0, original, "Configuration", "Проверка", UUID);
            let filename = format!("Проверка.v1.{suffix}");
            fs::rename(input, fixture.0.join(&filename)).unwrap();
            let log = fixture.0.join("commands.log");
            let output = Command::new(env!("CARGO_BIN_EXE_eska"))
                .current_dir(&fixture.0)
                .args([
                    "--lang",
                    locale,
                    "new",
                    "project",
                    "--from",
                    &filename,
                    "--workflow",
                    "trunk",
                    "--no-vcs",
                    "--platform-version",
                    "8.3.27.2325",
                    "--ibcmd",
                ])
                .arg(super::super::build::fake_ibcmd(&fixture))
                .env("FAKE_IBCMD_LOG", &log)
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            let root = fixture.0.join("project");
            assert!(
                fs::read_to_string(root.join("eska.toml"))
                    .unwrap()
                    .contains(&format!("type = \"{expected}\""))
            );
            assert!(root.join("src/ConfigDumpInfo.xml").is_file());
            assert!(!root.join("src/probe-only.txt").exists());
            let commands = fs::read_to_string(log).unwrap();
            let load = commands
                .lines()
                .rfind(|line| line.starts_with("infobase config load "))
                .unwrap();
            assert_eq!(
                load.contains("--extension=Проверка"),
                expected == "extension"
            );
            assert!(!load.contains("--force"));
            if suffix != "erf" {
                assert_eq!(
                    commands
                        .lines()
                        .filter(|line| line.starts_with("infobase create "))
                        .count(),
                    2
                );
            }
            assert!(
                !String::from_utf8_lossy(&output.stderr).contains("incomplete fixture file export")
            );
        }
    }
}

/// Failed loading, export, identity verification and missing dump state publish no partial files.
#[test]
fn refuses_incomplete_registered_extensions_without_changing_project_or_input() {
    for locale in ["ru", "en"] {
        let fixture = TestDir::new();
        let input = artifact(&fixture.0, "cfe", "Configuration", "Проверка", UUID);
        let contents = fs::read(&input).unwrap();
        let created = run(
            &fixture.0,
            &fixture,
            locale,
            &[
                "new",
                "project",
                "--from",
                "incoming.cfe",
                "--workflow",
                "trunk",
                "--no-vcs",
                "--platform-version",
                "8.3.27.2325",
            ],
        );
        assert!(created.status.success(), "{created:?}");
        let root = fixture.0.join("project");
        let config = fs::read(root.join("eska.toml")).unwrap();
        let descriptor = fs::read(root.join("src/Configuration.xml")).unwrap();
        for (flag, code) in [
            ("FAKE_IBCMD_FAIL_LOAD", "platform-failed"),
            ("FAKE_IBCMD_FAIL_EXPORT", "platform-failed"),
            ("FAKE_IBCMD_WRONG_EXPORTED_IDENTITY", "invalid-descriptor"),
            ("FAKE_IBCMD_MISSING_DUMP_INFO", "missing-descriptor"),
            ("FAKE_IBCMD_NO_PROBE_DESCRIPTOR", "platform-failed"),
            (
                "FAKE_IBCMD_TRUNCATED_PROBE_DESCRIPTOR",
                "invalid-descriptor",
            ),
        ] {
            let output = Command::new(env!("CARGO_BIN_EXE_eska"))
                .current_dir(&root)
                .args([
                    "--lang",
                    locale,
                    "import",
                    "../incoming.cfe",
                    "--force",
                    "--format",
                    "json",
                    "--ibcmd",
                ])
                .arg(super::super::build::fake_ibcmd(&fixture))
                .env(flag, "1")
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(1), "{output:?}");
            let document: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(document["error"]["code"], code);
            if flag == "FAKE_IBCMD_TRUNCATED_PROBE_DESCRIPTOR" {
                assert!(
                    String::from_utf8_lossy(&output.stderr)
                        .contains("incomplete fixture file export"),
                    "{output:?}"
                );
            }
            assert_eq!(
                fs::read(root.join("src/Configuration.xml")).unwrap(),
                descriptor
            );
            assert_eq!(fs::read(root.join("eska.toml")).unwrap(), config);
            assert_eq!(fs::read(&input).unwrap(), contents);
            assert_eq!(fs::read_dir(root.join(".eska/import")).unwrap().count(), 1);
            assert!(!root.join("src/probe-only.txt").exists());
        }
    }
}
