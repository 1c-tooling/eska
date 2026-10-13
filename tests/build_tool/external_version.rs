use super::*;

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
