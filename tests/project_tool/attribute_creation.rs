//! Offline attribute creation uses accepted parent authority and the existing locale-independent prepare protocol.

use super::*;
use eska::project_core::source_format::{
    Child, Descriptor, ObjectSource, Property, PropertyName, PropertyValue, Section,
};

const OWNER: &str = "20000000-0000-4000-8000-000000000001";
const CREATED: &str = "40000000-0000-4000-8000-000000000001";

/// The accepted parent declares the namespaces required by a new typed field without any native runner.
fn fixture(root: &Path) {
    fs::create_dir_all(root.join("native/Catalogs")).unwrap();
    fs::write(root.join("native/Configuration.xml"), "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Configuration uuid=\"10000000-0000-4000-8000-000000000001\"><Properties><Name>Sample</Name></Properties><ChildObjects><Catalog>Records</Catalog></ChildObjects></Configuration></MetaDataObject>").unwrap();
    fs::write(root.join("native/Catalogs/Records.xml"), format!("<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" xmlns:v8=\"http://v8.1c.ru/8.1/data/core\" xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" xmlns:cfg=\"http://v8.1c.ru/8.1/data/enterprise/current-config\" version=\"2.20\"><Catalog uuid=\"{OWNER}\"><Properties><Name>Records</Name></Properties><ChildObjects/></Catalog></MetaDataObject>")).unwrap();
}

/// A fresh node owns one type value; native layout carries only ordering and namespace coordinates.
fn append(root: &Path, name: &str, missing_target: bool) -> String {
    let file = root.join("src/Catalogs/Records/object.yaml");
    let mut owner = ObjectSource::from_yaml(&fs::read_to_string(&file).unwrap()).unwrap();
    let descriptor = Descriptor::from_xml(&format!("<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" xmlns:v8=\"http://v8.1c.ru/8.1/data/core\" xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" version=\"2.20\"><Attribute uuid=\"{CREATED}\"><Properties><Name>{name}</Name><Synonym/><Comment/><Type><v8:Type>xs:boolean</v8:Type></Type></Properties></Attribute></MetaDataObject>")).unwrap();
    let mut object = descriptor.object;
    if missing_target {
        let Section::Properties(properties) = &mut object.sections[0] else {
            unreachable!()
        };
        let Property::Typed {
            name: PropertyName::Type,
            value: PropertyValue::TypeDescription(value),
        } = &mut properties[3]
        else {
            unreachable!()
        };
        value.data.alternatives[0].name = "cfg:CatalogRef.Missing".into();
    }
    for section in &mut owner.document.descriptor.object.sections {
        if let Section::Children(children) = section {
            children.push(Child::Inline {
                object: Box::new(object.clone()),
            });
        }
    }
    let text = format!(
        "# Preserve annotation\r\n{}",
        owner.to_yaml().unwrap().replace('\n', "\r\n")
    );
    fs::write(file, &text).unwrap();
    text
}

/// Both layouts and locales select the same native bytes or restore a denied creation without changing YAML.
#[test]
fn prepare_creates_attribute_or_reports_parent_freeze_in_both_locales_and_layouts() {
    for layout in ["family", "readable"] {
        for frozen in [false, true] {
            let test = TestDir::new();
            fixture(&test.0);
            let mut args = vec![
                "import", "native", "project", "--name", "sample", "--layout", layout, "--format",
                "json",
            ];
            if frozen {
                args.extend(["--freeze", OWNER]);
            }
            let imported = success(&test.0, "ru", &args);
            let pin = imported["data"]["accepted_base_sha256"].as_str().unwrap();
            let project = test.0.join("project");
            let manifest = fs::read(project.join("src/source.json")).unwrap();
            let authored = append(&project, "Новое_Поле", false);
            let mut reports = Vec::new();
            for locale in ["ru", "en"] {
                let output = tool(
                    &project,
                    locale,
                    &["prepare", "--accepted-base-sha256", pin, "--format", "json"],
                );
                assert_eq!(output.status.code(), Some(i32::from(frozen)), "{output:?}");
                assert_eq!(output.stderr.is_empty(), !frozen);
                reports.push(serde_json::from_slice::<Value>(&output.stdout).unwrap());
            }
            assert_eq!(reports[0], reports[1]);
            let selected = if frozen {
                assert!(!project.join(".eska/generations").exists());
                assert_eq!(reports[0]["error"]["code"], "source_changes_excluded");
                let diagnostics = reports[0]["error"]["diagnostics"].as_array().unwrap();
                assert_eq!(diagnostics.len(), 1);
                assert_eq!(diagnostics[0]["operation"]["kind"], "create");
                assert_eq!(diagnostics[0]["uuid"], OWNER);
                assert_eq!(diagnostics[0]["reason"], "eska_frozen");
                let recovery = [
                    "prepare",
                    "--accepted-base-sha256",
                    pin,
                    "--allow-exclusions",
                    "--format",
                    "json",
                ];
                let restored = success(&project, "ru", &recovery);
                assert_eq!(restored, success(&project, "en", &recovery));
                restored
            } else {
                assert_eq!(reports[0]["data"]["exclusions"], serde_json::json!([]));
                assert_eq!(
                    reports[0],
                    success(
                        &project,
                        "en",
                        &["prepare", "--accepted-base-sha256", pin, "--format", "json"]
                    )
                );
                reports[0].clone()
            };
            let xml = fs::read_to_string(
                Path::new(selected["data"]["xml"].as_str().unwrap()).join("Catalogs/Records.xml"),
            )
            .unwrap();
            assert_eq!(xml.contains(CREATED), !frozen);
            assert_eq!(xml.contains("Новое_Поле"), !frozen);
            assert_eq!(xml.contains("<v8:Type>xs:boolean</v8:Type>"), !frozen);
            assert_eq!(
                fs::read_to_string(project.join("src/Catalogs/Records/object.yaml")).unwrap(),
                authored
            );
            assert_eq!(fs::read(project.join("src/source.json")).unwrap(), manifest);
        }
    }
}

/// A new field's reference cannot evade the configuration type index through allow-exclusions or locale.
#[test]
fn missing_type_target_refuses_without_partial_generation_in_both_locales() {
    for layout in ["family", "readable"] {
        let test = TestDir::new();
        fixture(&test.0);
        let imported = success(
            &test.0,
            "en",
            &[
                "import", "native", "project", "--name", "sample", "--layout", layout, "--format",
                "json",
            ],
        );
        let pin = imported["data"]["accepted_base_sha256"].as_str().unwrap();
        let project = test.0.join("project");
        let authored = append(&project, "NewField", true);
        let mut reports = Vec::new();
        for locale in ["ru", "en"] {
            for recovery in [false, true] {
                let mut args = vec!["prepare", "--accepted-base-sha256", pin, "--format", "json"];
                if recovery {
                    args.push("--allow-exclusions");
                }
                let output = tool(&project, locale, &args);
                assert_eq!(output.status.code(), Some(1));
                assert!(!output.stderr.is_empty());
                let report: Value = serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(report["error"]["code"], "source_type_target_missing");
                reports.push(report);
            }
        }
        assert!(reports.iter().all(|report| report == &reports[0]));
        assert!(!project.join(".eska/generations").exists());
        assert_eq!(
            fs::read_to_string(project.join("src/Catalogs/Records/object.yaml")).unwrap(),
            authored
        );
    }
}
