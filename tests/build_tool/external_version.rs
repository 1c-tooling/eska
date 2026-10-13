use super::*;
use eska::project_core::source_format::{NativeUuid, accept_source, import_designer_queries};

/// Convert only an owned native fixture, preserving imported module bytes and external identity.
fn eska_source(root: &Path, frozen: bool) -> String {
    let config = fs::read_to_string(root.join("eska.toml")).unwrap();
    let descriptor = root.join("src/External.xml");
    let xml = fs::read_to_string(&descriptor).unwrap().replace(
        "<MetaDataObject xmlns=",
        "<MetaDataObject version=\"2.20\" xmlns=",
    );
    let tag = if xml.contains("<ExternalReport>") {
        "ExternalReport"
    } else {
        "ExternalDataProcessor"
    };
    fs::write(
        &descriptor,
        xml.replace(
            &format!("<{tag}>"),
            &format!("<{tag} uuid=\"10000000-0000-4000-8000-000000000001\">"),
        ),
    )
    .unwrap();
    fs::rename(root.join("src"), root.join("native")).unwrap();
    import_designer_queries(&root.join("native"), &root.join("src")).unwrap();
    fs::create_dir(root.join(".eska")).unwrap();
    let freeze = if frozen {
        vec![NativeUuid::parse("10000000-0000-4000-8000-000000000001").unwrap()]
    } else {
        vec![]
    };
    let pin = accept_source(&root.join("src"), &root.join(".eska/accepted"), &freeze).unwrap();
    fs::write(
        root.join("eska.toml"),
        config.replace(
            "[build]",
            "source_format='eska'\n[eska]\naccepted_base='.eska/accepted'\n[build]",
        ),
    )
    .unwrap();
    pin.sha256
}

/// Protected EPF/ERF snapshots select their explicit root and record the exact BSL version.
#[test]
fn eska_external_build_uses_pinned_module_bytes_in_both_profiles_and_locales() {
    for (kind, tag, extension) in [
        ("processing", "ExternalDataProcessor", "epf"),
        ("report", "ExternalReport", "erf"),
    ] {
        for (value, locale) in [("1.2.3", "ru"), ("01.02.003.0004", "en")] {
            let fixture = TestDir::new();
            let module = format!(
                "\u{feff}Функция Версия() Экспорт\r\nВозврат \"{value}\";\r\nКонецФункции\r\n"
            );
            external(&fixture.0, kind, tag, &module);
            let pin = eska_source(&fixture.0, false);
            let output = command(
                &fixture.0,
                locale,
                &["build", "--format", "json", "--accepted-base-sha256", &pin],
            )
            .arg("--ibcmd")
            .arg(ibcmd_fixture::executable())
            .env(
                "FAKE_IBCMD_ARTIFACT_SOURCE_FILE",
                "External/Ext/ObjectModule.bsl",
            )
            .output()
            .unwrap();
            let document = report(&output, "success");
            let artifact = document["data"]["artifact"].as_str().unwrap();
            assert!(artifact.ends_with(extension));
            assert_eq!(fs::read(artifact).unwrap(), module.as_bytes());
            let manifest: Value = serde_json::from_slice(
                &fs::read(document["data"]["manifest"].as_str().unwrap()).unwrap(),
            )
            .unwrap();
            assert_eq!(manifest["project"]["version"]["value"], value);
            assert_eq!(manifest["source"]["protected"]["baseline"], pin);
            assert_eq!(
                fs::read_to_string(fixture.0.join("src/ObjectModule.bsl")).unwrap(),
                module
            );
        }
    }
}

/// A frozen edit refuses before launch; explicit exclusions build the accepted version.
#[test]
fn eska_external_build_cannot_hide_restricted_version_changes() {
    let fixture = TestDir::new();
    let accepted = "Функция Версия() Экспорт Возврат \"1.2.3\"; КонецФункции\r\n";
    external(&fixture.0, "report", "ExternalReport", accepted);
    let pin = eska_source(&fixture.0, true);
    let edited = accepted.replace("1.2.3", "1.2.4");
    fs::write(fixture.0.join("src/ObjectModule.bsl"), &edited).unwrap();
    for locale in ["ru", "en"] {
        let output = command(
            &fixture.0,
            locale,
            &["build", "--format", "json", "--accepted-base-sha256", &pin],
        )
        .arg("--ibcmd")
        .arg(ibcmd_fixture::executable())
        .output()
        .unwrap();
        let document = report(&output, "error");
        assert_eq!(document["error"]["code"], "source_changes_excluded");
        assert!(!fixture.0.join("platform.log").exists());
    }
    let output = command(
        &fixture.0,
        "en",
        &[
            "build",
            "--format",
            "json",
            "--accepted-base-sha256",
            &pin,
            "--allow-exclusions",
        ],
    )
    .arg("--ibcmd")
    .arg(ibcmd_fixture::executable())
    .env(
        "FAKE_IBCMD_ARTIFACT_SOURCE_FILE",
        "External/Ext/ObjectModule.bsl",
    )
    .output()
    .unwrap();
    let document = report(&output, "success");
    assert_eq!(
        fs::read(document["data"]["artifact"].as_str().unwrap()).unwrap(),
        accepted.as_bytes()
    );
    assert_eq!(
        fs::read_to_string(fixture.0.join("src/ObjectModule.bsl")).unwrap(),
        edited
    );
}

/// Create an actual native external profile; its source version is exclusively in BSL.
fn external(root: &Path, kind: &str, tag: &str, module: &str) {
    fs::create_dir_all(root.join("src/External/Ext")).unwrap();
    fs::write(
        root.join("eska.toml"),
        format!(
            "[project]\nname='external'\ntype='{kind}'\n[build]\nplatform_version='8.3.27.2325'\n"
        ),
    )
    .unwrap();
    fs::write(root.join("src/External.xml"), format!("<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\"><{tag}><Properties><Name>External</Name></Properties></{tag}></MetaDataObject>")).unwrap();
    fs::write(root.join("src/External/Ext/ObjectModule.bsl"), module).unwrap();
}

/// Invalid external contracts fail without a runner probe and preserve previous artifact/manifest pairs.
#[test]
fn external_contract_refuses_before_platform_with_localized_hint_and_structured_coordinates() {
    for (kind, tag, extension) in [
        ("processing", "ExternalDataProcessor", "epf"),
        ("report", "ExternalReport", "erf"),
    ] {
        let fixture = TestDir::new();
        let module = "\u{feff}Функция Версия() Возврат \"1.2.3\"; КонецФункции\r\n";
        external(&fixture.0, kind, tag, module);
        let output_name = format!("result.{extension}");
        fs::write(fixture.0.join(&output_name), b"previous artifact").unwrap();
        let manifest_name = format!("{output_name}.manifest.json");
        fs::write(fixture.0.join(&manifest_name), b"previous manifest").unwrap();
        let mut documents = Vec::new();
        for locale in ["ru", "en"] {
            let output = command(
                &fixture.0,
                locale,
                &["build", "--format", "json", "--output", &output_name],
            )
            .arg("--ibcmd")
            .arg(ibcmd_fixture::executable())
            .output()
            .unwrap();
            let document = report(&output, "error");
            assert_eq!(document["error"]["code"], "project_version_policy_invalid");
            assert_eq!(
                document["error"]["diagnostics"]["code"],
                "version_function_not_exported"
            );
            assert_eq!(document["error"]["diagnostics"]["range"]["byte_start"], 3);
            assert_eq!(
                document["error"]["diagnostics"]["remediation"],
                "version_migration"
            );
            assert!(
                document["error"]["location"][0]
                    .as_str()
                    .unwrap()
                    .ends_with("ObjectModule.bsl")
            );
            assert!(String::from_utf8_lossy(&output.stderr).contains("eska version migrate"));
            assert!(!fixture.0.join("platform.log").exists());
            assert_eq!(
                fs::read(fixture.0.join(&output_name)).unwrap(),
                b"previous artifact"
            );
            assert_eq!(
                fs::read(fixture.0.join(&manifest_name)).unwrap(),
                b"previous manifest"
            );
            assert_eq!(
                fs::read_to_string(fixture.0.join("src/External/Ext/ObjectModule.bsl")).unwrap(),
                module
            );
            documents.push(document);
        }
        assert_eq!(documents[0], documents[1]);
    }
}

/// The immutable build snapshot records the exact function literal in both numeric schemes.
#[test]
fn external_manifest_records_the_object_module_version_without_an_xml_fallback() {
    for (kind, tag, extension) in [
        ("processing", "ExternalDataProcessor", "epf"),
        ("report", "ExternalReport", "erf"),
    ] {
        for value in ["1.2.3", "01.02.003.0004"] {
            let fixture = TestDir::new();
            let module = format!("Функция Версия() Экспорт Возврат \"{value}\"; КонецФункции");
            external(&fixture.0, kind, tag, &module);
            let output = command(&fixture.0, "en", &["build", "--format", "json"])
                .arg("--ibcmd")
                .arg(ibcmd_fixture::executable())
                .output()
                .unwrap();
            let document = report(&output, "success");
            let manifest: Value = serde_json::from_slice(
                &fs::read(document["data"]["manifest"].as_str().unwrap()).unwrap(),
            )
            .unwrap();
            assert_eq!(manifest["schema_version"], 1);
            assert_eq!(manifest["project"]["version"]["value"], value);
            assert!(
                document["data"]["artifact"]
                    .as_str()
                    .unwrap()
                    .ends_with(extension)
            );
            assert_eq!(
                fs::read_to_string(fixture.0.join("src/External/Ext/ObjectModule.bsl")).unwrap(),
                module
            );
        }
    }
}
