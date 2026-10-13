//! Rename keeps the existing prepare protocol, native field UUIDs and selected dependency guard.

use super::attribute_fixture::{COLUMN, FIELD, OWNER, SHIFTED, STATIONARY, TABLE, fixture};
use super::*;
use eska::project_core::source_format::{
    Child, MetadataNode, ObjectSource, Property, PropertyName, PropertyValue, Section,
};

/// Change only two accepted UUIDs; unrelated fields, table identities and positions remain intact.
fn names(node: &mut MetadataNode) {
    for section in &mut node.sections {
        match section {
            Section::Properties(properties) => {
                for property in properties {
                    if let Property::Typed {
                        name: PropertyName::Name,
                        value,
                    } = property
                    {
                        if node.uuid.as_str() == FIELD {
                            *value = PropertyValue::Text("ПереименованноеПоле".into());
                        }
                        if node.uuid.as_str() == COLUMN {
                            *value = PropertyValue::Text("ChangedColumn".into());
                        }
                    }
                }
            }
            Section::Children(children) => {
                for child in children {
                    if let Child::Inline { object } = child {
                        names(object);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Manual authoring preserves annotations and CRLF without changing the accepted manifest or pin.
fn author(project: &Path) -> String {
    let path = project.join("src/Catalogs/Records/object.yaml");
    let mut owner = ObjectSource::from_yaml(&fs::read_to_string(&path).unwrap()).unwrap();
    names(&mut owner.document.descriptor.object);
    let text = format!(
        "# Keep annotation\r\n{}",
        owner.to_yaml().unwrap().replace('\n', "\r\n")
    );
    fs::write(path, &text).unwrap();
    text
}

/// Both locales/layouts select root and table names independently under parent/member/stationary freeze.
#[test]
fn prepare_renames_or_restores_fields_and_columns_by_actual_parent_in_both_locales() {
    for layout in ["family", "readable"] {
        for frozen in [
            None,
            Some(OWNER),
            Some(FIELD),
            Some(TABLE),
            Some(COLUMN),
            Some(SHIFTED),
            Some(STATIONARY),
        ] {
            let test = TestDir::new();
            fixture(&test.0, None);
            let mut args = vec![
                "import", "native", "project", "--name", "sample", "--layout", layout, "--format",
                "json",
            ];
            if let Some(uuid) = frozen {
                args.extend(["--freeze", uuid]);
            }
            let imported = success(&test.0, "en", &args);
            let pin = imported["data"]["accepted_base_sha256"].as_str().unwrap();
            let project = test.0.join("project");
            let settings = fs::read(project.join("eska.toml")).unwrap();
            let manifest = fs::read(project.join("src/source.json")).unwrap();
            let native = fs::read(test.0.join("native/Catalogs/Records.xml")).unwrap();
            let authored = author(&project);
            let refused = frozen.is_some_and(|uuid| [OWNER, FIELD, TABLE, COLUMN].contains(&uuid));
            let args = ["prepare", "--accepted-base-sha256", pin, "--format", "json"];
            let reports: Vec<Value> = ["ru", "en"]
                .iter()
                .map(|locale| {
                    let output = tool(&project, locale, &args);
                    assert_eq!(output.status.code(), Some(i32::from(refused)), "{output:?}");
                    assert_eq!(output.stderr.is_empty(), !refused);
                    serde_json::from_slice(&output.stdout).unwrap()
                })
                .collect();
            assert_eq!(reports[0], reports[1]);
            let selected = if refused {
                assert!(!project.join(".eska/generations").exists());
                assert_eq!(reports[0]["error"]["code"], "source_changes_excluded");
                let diagnostics = reports[0]["error"]["diagnostics"].as_array().unwrap();
                assert!(!diagnostics.is_empty());
                assert!(diagnostics.iter().all(|d|d["operation"]["kind"]=="rename" && d["reason"]=="eska_frozen"));
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
            } else {
                reports[0].clone()
            };
            let xml = fs::read_to_string(
                Path::new(selected["data"]["xml"].as_str().unwrap()).join("Catalogs/Records.xml"),
            )
            .unwrap();
            assert_eq!(
                xml.contains("<Name>ПереименованноеПоле</Name>"),
                !frozen.is_some_and(|uuid| [OWNER, FIELD].contains(&uuid))
            );
            assert_eq!(
                xml.contains("<Name>ChangedColumn</Name>"),
                !frozen.is_some_and(|uuid| [OWNER, TABLE, COLUMN].contains(&uuid))
            );
            for uuid in [OWNER, FIELD, SHIFTED, TABLE, COLUMN, STATIONARY] {
                assert!(xml.contains(uuid));
            }
            assert!(xml.contains("Retained table service bytes"));
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

/// Old field names in selected XML/BSL refuse both strict and recovery modes before publication in either locale.
#[test]
fn rename_field_dependencies_refuse_before_generation_in_both_locales() {
    for layout in ["family", "readable"] {
        for kind in ["xml", "bsl"] {
            let test = TestDir::new();
            fixture(&test.0, Some(kind));
            let imported = success(
                &test.0,
                "ru",
                &[
                    "import", "native", "project", "--name", "sample", "--layout", layout,
                    "--format", "json",
                ],
            );
            let pin = imported["data"]["accepted_base_sha256"].as_str().unwrap();
            let project = test.0.join("project");
            let authored = author(&project);
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
                    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
                    assert_eq!(
                        result["error"]["code"],
                        "source_structural_reference_preserved"
                    );
                    assert!(
                        result["error"]["location"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|v| v == FIELD)
                    );
                    reports.push(result);
                }
            }
            assert!(reports.iter().all(|r| r == &reports[0]));
            assert!(!project.join(".eska/generations").exists());
            assert_eq!(
                fs::read_to_string(project.join("src/Catalogs/Records/object.yaml")).unwrap(),
                authored
            );
        }
    }
}
