//! Table rename uses the existing project protocol, selected dependencies and accepted native identity.

use super::attribute_fixture::{COLUMN, FIELD, OWNER, SHIFTED, STATIONARY, TABLE};
use super::table_fixture::{SERVICES, fixture};
use super::*;
use eska::project_core::source_format::{
    Child, ObjectSource, Property, PropertyName, PropertyValue, Section,
};

/// A single authored Name owns both generated native declaration names; no manifest or service slot is edited.
fn rename(root: &Path, next: &str) -> String {
    let path = root.join("src/Catalogs/Records/object.yaml");
    let mut owner = ObjectSource::from_yaml(&fs::read_to_string(&path).unwrap()).unwrap();
    for section in &mut owner.document.descriptor.object.sections {
        if let Section::Children(children) = section {
            for child in children {
                if let Child::Inline { object } = child
                    && object.uuid.as_str() == TABLE
                {
                    for section in &mut object.sections {
                        if let Section::Properties(properties) = section {
                            for property in properties {
                                if let Property::Typed {
                                    name: PropertyName::Name,
                                    value,
                                } = property
                                {
                                    *value = PropertyValue::Text(next.into());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    let text = format!(
        "# Retain rename annotation\r\n{}",
        owner.to_yaml().unwrap().replace('\n', "\r\n")
    );
    fs::write(path, &text).unwrap();
    text
}

/// Parent/table freeze restores both derived names; unchanged frozen columns and other fields do not block rename.
#[test]
fn prepare_renames_or_restores_tables_with_stable_ids_in_both_layouts_and_locales() {
    for layout in ["family", "readable"] {
        for frozen in [
            None,
            Some(OWNER),
            Some(TABLE),
            Some(COLUMN),
            Some(STATIONARY),
            Some(FIELD),
            Some(SHIFTED),
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
            let imported = success(&test.0, "ru", &args);
            let pin = imported["data"]["accepted_base_sha256"].as_str().unwrap();
            let project = test.0.join("project");
            let settings = fs::read(project.join("eska.toml")).unwrap();
            let manifest = fs::read(project.join("src/source.json")).unwrap();
            let native = fs::read(test.0.join("native/Catalogs/Records.xml")).unwrap();
            let authored = rename(&project, "Новые_Строки");
            let denied = frozen.is_some_and(|uuid| [OWNER, TABLE].contains(&uuid));
            let reports: Vec<Value> = ["ru", "en"]
                .iter()
                .map(|locale| {
                    let output = tool(
                        &project,
                        locale,
                        &["prepare", "--accepted-base-sha256", pin, "--format", "json"],
                    );
                    assert_eq!(output.status.code(), Some(i32::from(denied)), "{output:?}");
                    assert_eq!(output.stderr.is_empty(), !denied);
                    serde_json::from_slice(&output.stdout).unwrap()
                })
                .collect();
            assert_eq!(reports[0], reports[1]);
            let selected = if denied {
                assert!(!project.join(".eska/generations").exists());
                assert_eq!(reports[0]["error"]["code"], "source_changes_excluded");
                let diagnostics = reports[0]["error"]["diagnostics"].as_array().unwrap();
                assert_eq!(diagnostics.len(), 1);
                assert_eq!(diagnostics[0]["uuid"], frozen.unwrap());
                assert_eq!(diagnostics[0]["operation"]["kind"], "rename");
                let args = [
                    "prepare",
                    "--accepted-base-sha256",
                    pin,
                    "--allow-exclusions",
                    "--format",
                    "json",
                ];
                let result = success(&project, "ru", &args);
                assert_eq!(result, success(&project, "en", &args));
                result
            } else {
                reports[0].clone()
            };
            let xml = fs::read_to_string(
                Path::new(selected["data"]["xml"].as_str().unwrap()).join("Catalogs/Records.xml"),
            )
            .unwrap();
            let name = if denied {
                "Rows"
            } else {
                "Новые_Строки"
            };
            assert!(xml.contains(&format!("<Name>{name}</Name>")));
            for role in ["TabularSection", "TabularSectionRow"] {
                assert!(xml.contains(&format!("name=\"Catalog{role}.Records.{name}\"")));
            }
            for uuid in [OWNER, TABLE, COLUMN, STATIONARY, FIELD, SHIFTED]
                .into_iter()
                .chain(SERVICES)
            {
                assert!(xml.contains(uuid));
            }
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

/// Selected XML/BSL old-name mentions refuse before generation; surviving service UUID references remain valid.
#[test]
fn table_rename_checks_native_dependencies_in_both_locales_and_modes() {
    for layout in ["family", "readable"] {
        for kind in ["xml", "bsl", "service"] {
            let test = TestDir::new();
            fixture(&test.0, Some(kind));
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
            let authored = rename(&project, "ChangedRows");
            let mut reports = vec![];
            for locale in ["ru", "en"] {
                for recovery in [false, true] {
                    let mut args =
                        vec!["prepare", "--accepted-base-sha256", pin, "--format", "json"];
                    if recovery {
                        args.push("--allow-exclusions");
                    }
                    let output = tool(&project, locale, &args);
                    let denied = kind != "service";
                    assert_eq!(output.status.code(), Some(i32::from(denied)), "{output:?}");
                    assert_eq!(output.stderr.is_empty(), !denied);
                    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
                    if denied {
                        assert_eq!(
                            report["error"]["code"],
                            "source_structural_reference_preserved"
                        );
                        assert!(
                            report["error"]["location"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .any(|v| v == TABLE)
                        );
                    } else {
                        let xml = fs::read_to_string(
                            Path::new(report["data"]["xml"].as_str().unwrap())
                                .join("Catalogs/Records.xml"),
                        )
                        .unwrap();
                        assert!(xml.contains("CatalogTabularSectionRow.Records.ChangedRows"));
                        for uuid in SERVICES {
                            assert!(xml.contains(uuid));
                        }
                    }
                    reports.push(report);
                }
            }
            assert!(reports.iter().all(|r| r == &reports[0]));
            assert_eq!(
                project.join(".eska/generations").exists(),
                kind == "service"
            );
            assert_eq!(
                fs::read_to_string(project.join("src/Catalogs/Records/object.yaml")).unwrap(),
                authored
            );
        }
    }
}
