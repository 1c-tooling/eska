//! Tabular-section creation uses the existing prepare protocol and independent accepted parent authority.

use super::*;
use eska::project_core::source_format::{Child, Descriptor, ObjectSource, Section};

const OWNER: &str = "20000000-0000-4000-8000-000000000001";
const TABLE: &str = "30000000-0000-4000-8000-000000000001";
const CREATED: &str = "40000000-0000-4000-8000-000000000001";

/// A table-local type namespace is accepted independently from the empty working column list.
fn fixture(root: &Path) {
    fs::create_dir_all(root.join("native/Catalogs")).unwrap();
    fs::write(root.join("native/Configuration.xml"), "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Configuration uuid=\"10000000-0000-4000-8000-000000000001\"><Properties><Name>Sample</Name></Properties><ChildObjects><Catalog>Records</Catalog></ChildObjects></Configuration></MetaDataObject>").unwrap();
    fs::write(root.join("native/Catalogs/Records.xml"), format!("<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" xmlns:v8=\"http://v8.1c.ru/8.1/data/core\" xmlns:cfg=\"http://v8.1c.ru/8.1/data/enterprise/current-config\" version=\"2.20\"><Catalog uuid=\"{OWNER}\"><Properties><Name>Records</Name></Properties><ChildObjects><TabularSection xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" uuid=\"{TABLE}\"><Properties><Name>Rows</Name></Properties><ChildObjects/></TabularSection></ChildObjects></Catalog></MetaDataObject>")).unwrap();
}

/// Fresh fields retain the actual table's `QName` scope without duplicating it in the new node.
fn append(root: &Path, missing: bool) -> String {
    let file = root.join("src/Catalogs/Records/object.yaml");
    let mut owner = ObjectSource::from_yaml(&fs::read_to_string(&file).unwrap()).unwrap();
    let name = if missing {
        "cfg:EnumRef.Missing"
    } else {
        "xs:boolean"
    };
    let descriptor = Descriptor::from_xml(&format!("<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" xmlns:v8=\"http://v8.1c.ru/8.1/data/core\" xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" xmlns:cfg=\"http://v8.1c.ru/8.1/data/enterprise/current-config\" version=\"2.20\"><Attribute uuid=\"{CREATED}\"><Properties><Name>Новая_Колонка</Name><Synonym/><Comment/><Type><v8:Type>{name}</v8:Type></Type></Properties></Attribute></MetaDataObject>")).unwrap();
    let Section::Children(children) = &mut owner.document.descriptor.object.sections[1] else {
        unreachable!()
    };
    let Child::Inline { object } = &mut children[0] else {
        unreachable!()
    };
    let Section::Children(columns) = &mut object.sections[1] else {
        unreachable!()
    };
    columns.push(Child::Inline {
        object: Box::new(descriptor.object),
    });
    let text = format!(
        "# Preserve table annotation\r\n{}",
        owner.to_yaml().unwrap().replace('\n', "\r\n")
    );
    fs::write(file, &text).unwrap();
    text
}

/// Both layouts and locales create under the true parent or report table/ancestor freezes consistently.
#[test]
fn prepare_creates_columns_or_restores_table_freeze_in_both_layouts_and_locales() {
    for layout in ["family", "readable"] {
        for freeze in [None, Some(OWNER), Some(TABLE)] {
            let test = TestDir::new();
            fixture(&test.0);
            let mut args = vec![
                "import", "native", "project", "--name", "sample", "--layout", layout, "--format",
                "json",
            ];
            if let Some(freeze) = freeze {
                args.extend(["--freeze", freeze]);
            }
            let imported = success(&test.0, "ru", &args);
            let pin = imported["data"]["accepted_base_sha256"].as_str().unwrap();
            let project = test.0.join("project");
            let manifest = fs::read(project.join("src/source.json")).unwrap();
            let native = fs::read(test.0.join("native/Catalogs/Records.xml")).unwrap();
            let authored = append(&project, false);
            let mut reports = Vec::new();
            for locale in ["ru", "en"] {
                let output = tool(
                    &project,
                    locale,
                    &["prepare", "--accepted-base-sha256", pin, "--format", "json"],
                );
                assert_eq!(
                    output.status.code(),
                    Some(i32::from(freeze.is_some())),
                    "{output:?}"
                );
                assert_eq!(output.stderr.is_empty(), freeze.is_none());
                reports.push(serde_json::from_slice::<Value>(&output.stdout).unwrap());
            }
            assert_eq!(reports[0], reports[1]);
            let selected = if freeze.is_some() {
                assert!(!project.join(".eska/generations").exists());
                assert_eq!(reports[0]["error"]["code"], "source_changes_excluded");
                let diagnostics = reports[0]["error"]["diagnostics"].as_array().unwrap();
                assert_eq!(diagnostics.len(), 1);
                assert_eq!(diagnostics[0]["operation"]["kind"], "create");
                assert_eq!(diagnostics[0]["uuid"], TABLE);
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
            assert_eq!(xml.contains(CREATED), freeze.is_none());
            assert_eq!(xml.contains("Новая_Колонка"), freeze.is_none());
            assert_eq!(
                xml.contains("<v8:Type>xs:boolean</v8:Type>"),
                freeze.is_none()
            );
            assert!(xml.contains(TABLE));
            assert_eq!(
                fs::read_to_string(project.join("src/Catalogs/Records/object.yaml")).unwrap(),
                authored
            );
            assert_eq!(fs::read(project.join("src/source.json")).unwrap(), manifest);
            assert_eq!(
                fs::read(test.0.join("native/Catalogs/Records.xml")).unwrap(),
                native
            );
        }
    }
}

/// Missing declared type targets refuse before publication even when exclusions are explicitly allowed.
#[test]
fn missing_column_type_target_refuses_without_generation_in_both_locales() {
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
        let authored = append(&project, true);
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
