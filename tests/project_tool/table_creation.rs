//! Fresh tabular sections use explicit UUIDs and existing preparation, policy and diagnostic contracts.

use super::*;
use eska::project_core::source_format::{
    Child, Descriptor, GeneratedIdentity, MetadataNode, NativeUuid, ObjectSource, Property,
    PropertyName, PropertyValue, Section, TabularSectionIdentities,
};

const OWNER: &str = "20000000-0000-4000-8000-000000000001";
const EXISTING: &str = "40000000-0000-4000-8000-000000000001";
const CREATED: &str = "90000000-0000-4000-8000-000000000001";
const OPAQUE: &str = "70000000-0000-4000-8000-000000000001";

/// The accepted fixture has an opaque generated UUID and a table that insertions may displace.
fn fixture(root: &Path) {
    fs::create_dir_all(root.join("native/Catalogs")).unwrap();
    fs::write(root.join("native/Configuration.xml"), "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Configuration uuid=\"10000000-0000-4000-8000-000000000001\"><Properties><Name>Sample</Name><Comment/></Properties><ChildObjects><Catalog>Records</Catalog></ChildObjects></Configuration></MetaDataObject>").unwrap();
    fs::write(root.join("native/Catalogs/Records.xml"), format!("<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" xmlns:v8=\"http://v8.1c.ru/8.1/data/core\" xmlns:xr=\"http://v8.1c.ru/8.3/xcf/readable\" xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" xmlns:cfg=\"http://v8.1c.ru/8.1/data/enterprise/current-config\" version=\"2.20\"><Catalog uuid=\"{OWNER}\"><InternalInfo><xr:GeneratedType name=\"CatalogObject.Records\" category=\"Object\"><xr:TypeId>{OPAQUE}</xr:TypeId><xr:ValueId>80000000-0000-4000-8000-000000000001</xr:ValueId></xr:GeneratedType></InternalInfo><Properties><Name>Records</Name></Properties><ChildObjects><Attribute uuid=\"30000000-0000-4000-8000-000000000001\"><Properties><Name>Flag</Name><Comment/></Properties></Attribute><TabularSection uuid=\"{EXISTING}\"><Properties><Name>Rows</Name></Properties><ChildObjects/></TabularSection></ChildObjects></Catalog></MetaDataObject>")).unwrap();
}

/// Deterministic new identities keep test assertions separate from the four generated role slots.
fn uuid(prefix: char) -> NativeUuid {
    NativeUuid::parse(&format!("{prefix}0000000-0000-4000-8000-000000000001")).unwrap()
}

/// Build the entire new subtree without assigning native supplier authority to its fresh identities.
fn append(root: &Path, displaced: bool, missing: bool, collision: bool) -> String {
    let file = root.join("src/Catalogs/Records/object.yaml");
    let mut source = ObjectSource::from_yaml(&fs::read_to_string(&file).unwrap()).unwrap();
    let name = if missing {
        "cfg:EnumRef.Missing"
    } else {
        "xs:boolean"
    };
    let column = Descriptor::from_xml(&format!("<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" xmlns:v8=\"http://v8.1c.ru/8.1/data/core\" xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" xmlns:cfg=\"http://v8.1c.ru/8.1/data/enterprise/current-config\" version=\"2.20\"><Attribute uuid=\"{}\"><Properties><Name>Новая_Колонка</Name><Synonym/><Comment/><Type><v8:Type>{name}</v8:Type></Type></Properties></Attribute></MetaDataObject>", uuid('e').as_str())).unwrap();
    let table = MetadataNode {
        kind: eska::project_core::MetadataKind::TabularSection,
        uuid: NativeUuid::parse(CREATED).unwrap(),
        namespaces: Vec::new(),
        attributes: Vec::new(),
        sections: vec![
            Section::TabularSectionIdentities(TabularSectionIdentities {
                prefix: "xr".into(),
                table: GeneratedIdentity {
                    type_id: if collision {
                        NativeUuid::parse(OPAQUE).unwrap()
                    } else {
                        uuid('a')
                    },
                    value_id: uuid('b'),
                },
                row: GeneratedIdentity {
                    type_id: uuid('c'),
                    value_id: uuid('d'),
                },
            }),
            Section::Properties(vec![
                Property::Typed {
                    name: PropertyName::Name,
                    value: PropertyValue::Text("Новые_Строки".into()),
                },
                Property::Typed {
                    name: PropertyName::Synonym,
                    value: PropertyValue::Empty,
                },
                Property::Typed {
                    name: PropertyName::Comment,
                    value: PropertyValue::Empty,
                },
            ]),
            Section::Children(vec![Child::Inline {
                object: Box::new(column.object),
            }]),
        ],
    };
    for section in &mut source.document.descriptor.object.sections {
        if let Section::Children(children) = section {
            children.insert(
                usize::from(!displaced) + 1,
                Child::Inline {
                    object: Box::new(table),
                },
            );
            break;
        }
    }
    let text = format!(
        "# Preserve table annotation\r\n{}",
        source.to_yaml().unwrap().replace('\n', "\r\n")
    );
    fs::write(file, &text).unwrap();
    let file = root.join("src/configuration.yaml");
    let mut configuration = ObjectSource::from_yaml(&fs::read_to_string(&file).unwrap()).unwrap();
    for section in &mut configuration.document.descriptor.object.sections {
        if let Section::Properties(properties) = section {
            for property in properties {
                if let Property::Typed {
                    name: PropertyName::Comment,
                    value,
                } = property
                {
                    *value = PropertyValue::Text("Independent configuration edit".into());
                }
            }
        }
    }
    fs::write(file, configuration.to_yaml().unwrap()).unwrap();
    text
}

/// Layout and locale do not affect creation authority, displaced freezes or independent selected properties.
#[test]
fn prepare_creates_tables_or_restores_owner_and_displaced_freezes_in_both_locales() {
    for layout in ["family", "readable"] {
        for freeze in [None, Some(OWNER), Some(EXISTING)] {
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
            let settings = fs::read(project.join("eska.toml")).unwrap();
            let native = fs::read(test.0.join("native/Catalogs/Records.xml")).unwrap();
            let authored = append(&project, freeze == Some(EXISTING), false, false);
            let reports: Vec<Value> = ["ru", "en"]
                .iter()
                .map(|locale| {
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
                    serde_json::from_slice(&output.stdout).unwrap()
                })
                .collect();
            assert_eq!(reports[0], reports[1]);
            let selected = freeze.map_or_else(
                || {
                    assert_eq!(reports[0]["data"]["exclusions"], serde_json::json!([]));
                    reports[0].clone()
                },
                |freeze| {
                    assert!(!project.join(".eska/generations").exists());
                    assert_eq!(reports[0]["error"]["code"], "source_changes_excluded");
                    let diagnostics = reports[0]["error"]["diagnostics"].as_array().unwrap();
                    assert_eq!(diagnostics.len(), 1);
                    assert_eq!(diagnostics[0]["operation"]["kind"], "create");
                    assert_eq!(diagnostics[0]["uuid"], freeze);
                    assert_eq!(diagnostics[0]["reason"], "eska_frozen");
                    let args = [
                        "prepare",
                        "--accepted-base-sha256",
                        pin,
                        "--allow-exclusions",
                        "--format",
                        "json",
                    ];
                    let selected = success(&project, "ru", &args);
                    assert_eq!(selected, success(&project, "en", &args));
                    selected
                },
            );
            let xml_root = Path::new(selected["data"]["xml"].as_str().unwrap());
            let xml = fs::read_to_string(xml_root.join("Catalogs/Records.xml")).unwrap();
            assert_eq!(xml.contains(CREATED), freeze.is_none());
            assert_eq!(xml.contains("Новая_Колонка"), freeze.is_none());
            assert_eq!(
                xml.contains("CatalogTabularSection.Records.Новые_Строки"),
                freeze.is_none()
            );
            assert!(xml.contains(OPAQUE));
            assert!(xml.contains(EXISTING));
            assert!(
                fs::read_to_string(xml_root.join("Configuration.xml"))
                    .unwrap()
                    .contains("Independent configuration edit")
            );
            assert_eq!(
                fs::read_to_string(project.join("src/Catalogs/Records/object.yaml")).unwrap(),
                authored
            );
            assert_eq!(fs::read(project.join("src/source.json")).unwrap(), manifest);
            assert_eq!(fs::read(project.join("eska.toml")).unwrap(), settings);
            assert_eq!(
                fs::read(test.0.join("native/Catalogs/Records.xml")).unwrap(),
                native
            );
        }
    }
}

/// Missing column types and legacy service UUID collisions cannot be bypassed through recovery mode.
#[test]
fn invalid_table_dependencies_and_service_ids_refuse_before_generation_in_both_locales() {
    for layout in ["family", "readable"] {
        for collision in [false, true] {
            let test = TestDir::new();
            fixture(&test.0);
            let imported = success(
                &test.0,
                "en",
                &[
                    "import", "native", "project", "--name", "sample", "--layout", layout,
                    "--format", "json",
                ],
            );
            let pin = imported["data"]["accepted_base_sha256"].as_str().unwrap();
            let project = test.0.join("project");
            let authored = append(&project, false, !collision, collision);
            let mut reports = Vec::new();
            for locale in ["ru", "en"] {
                for recovery in [false, true] {
                    let mut args =
                        vec!["prepare", "--accepted-base-sha256", pin, "--format", "json"];
                    if recovery {
                        args.push("--allow-exclusions");
                    }
                    let output = tool(&project, locale, &args);
                    assert_eq!(output.status.code(), Some(1), "{output:?}");
                    assert!(!output.stderr.is_empty());
                    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
                    assert_eq!(
                        report["error"]["code"],
                        if collision {
                            "native_uuid_duplicate"
                        } else {
                            "source_type_target_missing"
                        }
                    );
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
}
