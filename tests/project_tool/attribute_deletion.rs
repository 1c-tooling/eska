//! Field and column removal uses the existing prepare protocol, native authority and selected dependency bytes.

use super::attribute_fixture::{
    COLUMN, FIELD as DELETED, OWNER, SHIFTED, STATIONARY, TABLE, fixture,
};
use super::*;
use eska::project_core::source_format::{Child, ObjectSource, Section};

/// Authoring removes complete children while retaining annotation, CRLF, native service slots and reverse map.
fn remove(root: &Path) -> String {
    let path = root.join("src/Catalogs/Records/object.yaml");
    let mut owner = ObjectSource::from_yaml(&fs::read_to_string(&path).unwrap()).unwrap();
    for section in &mut owner.document.descriptor.object.sections {
        if let Section::Children(children) = section {
            children.retain(|child| !matches!(child, Child::Inline { object } if object.uuid.as_str() == DELETED));
            for child in children {
                if let Child::Inline { object } = child
                    && object.uuid.as_str() == TABLE
                {
                    for section in &mut object.sections {
                        if let Section::Children(columns) = section {
                            columns.retain(|child| !matches!(child, Child::Inline { object } if object.uuid.as_str() == COLUMN));
                        }
                    }
                }
            }
        }
    }
    let text = format!(
        "# Retain annotation\r\n{}",
        owner.to_yaml().unwrap().replace('\n', "\r\n")
    );
    fs::write(path, &text).unwrap();
    text
}

/// Owner/member/displaced/stationary freezes select the same collections in both layouts and locales.
#[test]
fn prepare_removes_or_restores_fields_and_columns_by_accepted_policy_in_both_locales() {
    for layout in ["family", "readable"] {
        for frozen in [
            None,
            Some(OWNER),
            Some(DELETED),
            Some(SHIFTED),
            Some(TABLE),
            Some(COLUMN),
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
            let authored = remove(&project);
            let refused = frozen.is_some_and(|uuid| uuid != STATIONARY);
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
                assert!(diagnostics.iter().all(|d| d["operation"]["kind"] == "delete" && d["reason"] == "eska_frozen"));
                let recovery = [
                    "prepare",
                    "--accepted-base-sha256",
                    pin,
                    "--allow-exclusions",
                    "--format",
                    "json",
                ];
                let selected = success(&project, "ru", &recovery);
                assert_eq!(selected, success(&project, "en", &recovery));
                selected
            } else {
                reports[0].clone()
            };
            let xml = fs::read_to_string(
                Path::new(selected["data"]["xml"].as_str().unwrap()).join("Catalogs/Records.xml"),
            )
            .unwrap();
            assert_eq!(
                xml.contains(DELETED),
                frozen.is_some_and(|uuid| [OWNER, DELETED, SHIFTED, TABLE].contains(&uuid))
            );
            assert_eq!(
                xml.contains(COLUMN),
                frozen.is_some_and(|uuid| [OWNER, TABLE, COLUMN].contains(&uuid))
            );
            for retained in [OWNER, SHIFTED, TABLE, STATIONARY] {
                assert!(xml.contains(retained));
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

/// A remaining field mention in XML/BSL cannot be bypassed through locale or explicit recovery mode.
#[test]
fn field_dependencies_in_selected_xml_and_bsl_refuse_before_generation_in_both_locales() {
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
            let authored = remove(&project);
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
                            .any(|value| value == DELETED)
                    );
                    reports.push(result);
                }
            }
            assert!(reports.iter().all(|result| result == &reports[0]));
            assert!(!project.join(".eska/generations").exists());
            assert_eq!(
                fs::read_to_string(project.join("src/Catalogs/Records/object.yaml")).unwrap(),
                authored
            );
        }
    }
}
