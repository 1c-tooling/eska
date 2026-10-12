//! The standalone CLI reports namespace authority and invalidated-name dependencies without rewriting YAML.

use super::*;
use eska::project_core::source_format::{
    Child, ObjectSource, Property, PropertyName, PropertyValue, Section,
};

const OWNER: &str = "20000000-0000-4000-8000-000000000001";
const VALUE: &str = "30000000-0000-4000-8000-000000000001";

/// Author one native Name, retaining its identity, annotation and CRLF for either source envelope.
fn rename(project: &Path) -> String {
    let path = project.join("src/Enums/Status/object.yaml");
    let mut owner = ObjectSource::from_yaml(&fs::read_to_string(&path).unwrap()).unwrap();
    for section in &mut owner.document.descriptor.object.sections {
        if let Section::Children(children) = section {
            for child in children {
                if let Child::Inline { object } = child
                    && object.uuid.as_str() == VALUE
                {
                    for section in &mut object.sections {
                        if let Section::Properties(properties) = section {
                            for property in properties {
                                if let Property::Typed {
                                    name: PropertyName::Name,
                                    value,
                                } = property
                                {
                                    *value = PropertyValue::Text("Новое_Значение".into());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    let text = format!(
        "# Keep annotation\r\n{}",
        owner.to_yaml().unwrap().replace('\n', "\r\n")
    );
    fs::write(path, &text).unwrap();
    text
}

/// Owner/value freezes restore Name with one stable Rename diagnostic and the actual blocking UUID.
#[test]
fn prepare_renames_or_restores_under_the_same_namespace_policy_in_ru_and_en() {
    for layout in ["family", "readable"] {
        for frozen in [None, Some(OWNER), Some(VALUE)] {
            let test = TestDir::new();
            super::creation::fixture(&test.0);
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
            let manifest = fs::read(project.join("src/source.json")).unwrap();
            let authored = rename(&project);
            let args = ["prepare", "--accepted-base-sha256", pin, "--format", "json"];
            let reports: Vec<_> = ["ru", "en"]
                .iter()
                .map(|locale| {
                    let output = tool(&project, locale, &args);
                    assert_eq!(
                        output.status.code(),
                        Some(i32::from(frozen.is_some())),
                        "{output:?}"
                    );
                    assert_eq!(output.stderr.is_empty(), frozen.is_none());
                    serde_json::from_slice::<Value>(&output.stdout).unwrap()
                })
                .collect();
            assert_eq!(reports[0], reports[1]);
            let selected = frozen.map_or_else(
                || {
                    assert_eq!(reports[0]["data"]["exclusions"], serde_json::json!([]));
                    reports[0].clone()
                },
                |uuid| {
                    assert!(!project.join(".eska/generations").exists());
                    assert_eq!(reports[0]["error"]["code"], "source_changes_excluded");
                    let exclusions = reports[0]["error"]["diagnostics"].as_array().unwrap();
                    assert_eq!(exclusions.len(), 1);
                    assert_eq!(exclusions[0]["operation"]["kind"], "rename");
                    assert_eq!(exclusions[0]["uuid"], uuid);
                    assert_eq!(exclusions[0]["reason"], "eska_frozen");
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
                },
            );
            let xml = fs::read_to_string(
                Path::new(selected["data"]["xml"].as_str().unwrap()).join("Enums/Status.xml"),
            )
            .unwrap();
            assert_eq!(xml.contains("Новое_Значение"), frozen.is_none());
            assert!(xml.contains(VALUE));
            assert_eq!(
                fs::read_to_string(project.join("src/Enums/Status/object.yaml")).unwrap(),
                authored
            );
            assert_eq!(fs::read(project.join("src/source.json")).unwrap(), manifest);
        }
    }
}

/// An old canonical XML name refuses preparation in either locale even when exclusions are explicitly allowed.
#[test]
fn old_name_reference_refuses_identically_before_generation() {
    let test = TestDir::new();
    super::creation::fixture(&test.0);
    fs::create_dir_all(test.0.join("native/Enums/Status/Ext")).unwrap();
    fs::write(
        test.0.join("native/Enums/Status/Ext/Links.xml"),
        "<Links>Enum.Status.EnumValue.First</Links>",
    )
    .unwrap();
    let imported = success(
        &test.0,
        "en",
        &[
            "import", "native", "project", "--name", "sample", "--layout", "readable", "--format",
            "json",
        ],
    );
    let pin = imported["data"]["accepted_base_sha256"].as_str().unwrap();
    let project = test.0.join("project");
    let authored = rename(&project);
    for strict in [true, false] {
        let mut args = vec!["prepare", "--accepted-base-sha256", pin, "--format", "json"];
        if !strict {
            args.push("--allow-exclusions");
        }
        let mut reports = Vec::new();
        for locale in ["ru", "en"] {
            let output = tool(&project, locale, &args);
            assert_eq!(output.status.code(), Some(1), "{output:?}");
            assert!(!output.stderr.is_empty());
            reports.push(serde_json::from_slice::<Value>(&output.stdout).unwrap());
        }
        assert_eq!(reports[0], reports[1]);
        assert_eq!(
            reports[0]["error"]["code"],
            "source_structural_reference_preserved"
        );
        assert!(!project.join(".eska/generations").exists());
    }
    assert_eq!(
        fs::read_to_string(project.join("src/Enums/Status/object.yaml")).unwrap(),
        authored
    );
}
