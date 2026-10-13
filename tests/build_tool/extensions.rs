//! CFE builds consume protected ESKA inputs and the explicitly selected base configuration.

use super::*;

/// The extension profile keeps its native type and saved module through preparation and packing.
#[test]
fn builds_protected_extensions_with_base_context_in_both_locales() {
    for locale in ["ru", "en"] {
        let test = TestDir::new();
        native(&test.0);
        let descriptor = test.0.join("native/Configuration.xml");
        let contents = fs::read_to_string(&descriptor).unwrap().replace(
            "<Properties>",
            "<Properties><ConfigurationExtensionPurpose>Patch</ConfigurationExtensionPurpose>",
        );
        fs::write(&descriptor, contents).unwrap();
        let imported = import_project(
            &test.0.join("native"),
            &test.0.join("project"),
            ProjectName::parse("extension".to_owned()).unwrap(),
            Some(&PlatformVersion::parse("8.3.27.2325").unwrap()),
            &[],
        )
        .unwrap();
        let root = test.0.join("project");
        fs::write(root.join("base.cf"), b"base").unwrap();
        let output = command(
            &root,
            locale,
            &[
                "build",
                "--format",
                "json",
                "--accepted-base-sha256",
                &imported.accepted.sha256,
                "--base-configuration",
                "base.cf",
            ],
        )
        .arg("--ibcmd")
        .arg(ibcmd_fixture::executable())
        .env("FAKE_IBCMD_ARTIFACT_SOURCE_FILE", "Module.bsl")
        .output()
        .unwrap();
        let value = report(&output, "success");
        assert_eq!(value["data"]["artifact_type"], "extension");
        let artifact = value["data"]["artifact"].as_str().unwrap();
        assert!(
            Path::new(artifact)
                .extension()
                .is_some_and(|extension| extension == "cfe")
        );
        assert_eq!(fs::read(artifact).unwrap(), b"\xef\xbb\xbf// original\r\n");
        let manifest: Value =
            serde_json::from_slice(&fs::read(value["data"]["manifest"].as_str().unwrap()).unwrap())
                .unwrap();
        assert_eq!(manifest["artifact"]["type"], "extension");
        assert_eq!(
            manifest["source"]["protected"]["baseline"],
            imported.accepted.sha256
        );
        assert_eq!(manifest["project"]["version"]["value"], "1.2.3.4");
        let log = fs::read_to_string(root.join("platform.log")).unwrap();
        assert!(
            log.lines()
                .any(|line| line.starts_with("infobase create ") && line.contains("--load="))
        );
    }
}
