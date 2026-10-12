//! Offline preparation refuses removal before publication when native support or references require the value.

use super::*;
use eska::project_core::source_format::{Child, ObjectSource, Section};

const OWNER: &str = "20000000-0000-4000-8000-000000000001";
const DELETED: &str = "40000000-0000-4000-8000-000000000001";

/// An existing accepted value, rather than an unaccepted addition, is removed by the tested command.
fn fixture(root: &Path, dependency: bool) {
    super::creation::fixture(root);
    let path = root.join("native/Enums/Status.xml");
    let xml = fs::read_to_string(&path).unwrap().replace(
        "</ChildObjects>",
        &format!("<EnumValue uuid=\"{DELETED}\"><Properties><Name>Second</Name><Synonym/><Comment/></Properties></EnumValue></ChildObjects>"),
    );
    fs::write(path, xml).unwrap();
    if dependency {
        fs::create_dir_all(root.join("native/Enums/Status/Ext")).unwrap();
        fs::write(
            root.join("native/Enums/Status/Ext/Resource.xml"),
            "<Resource><Value>Enum.Status.EnumValue.Second</Value></Resource>",
        )
        .unwrap();
    }
}

/// Manual deletion leaves annotation, CRLF and the reverse manifest under the author's control.
fn remove(root: &Path) -> String {
    let path = root.join("src/Enums/Status/object.yaml");
    let mut owner = ObjectSource::from_yaml(&fs::read_to_string(&path).unwrap()).unwrap();
    for section in &mut owner.document.descriptor.object.sections {
        if let Section::Children(children) = section {
            children.retain(|child| !matches!(child, Child::Inline { object } if object.uuid.as_str() == DELETED));
        }
    }
    let authored = format!(
        "# Keep annotation\r\n{}",
        owner.to_yaml().unwrap().replace('\n', "\r\n")
    );
    fs::write(path, &authored).unwrap();
    authored
}

/// Both layouts and locales apply owner/member freeze and report one deletion with its actual authority.
#[test]
fn prepare_removes_or_restores_value_under_the_accepted_native_policy() {
    for layout in ["family", "readable"] {
        for frozen in [None, Some(OWNER), Some(DELETED)] {
            let test = TestDir::new();
            fixture(&test.0, false);
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
            let manifest = fs::read(project.join("src/source.json")).unwrap();
            let authored = remove(&project);
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
                    let diagnostics = reports[0]["error"]["diagnostics"].as_array().unwrap();
                    assert_eq!(diagnostics.len(), 1);
                    assert_eq!(diagnostics[0]["operation"]["kind"], "delete");
                    assert_eq!(diagnostics[0]["uuid"], uuid);
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
                },
            );
            let xml = fs::read_to_string(
                Path::new(selected["data"]["xml"].as_str().unwrap()).join("Enums/Status.xml"),
            )
            .unwrap();
            assert_eq!(xml.contains(DELETED), frozen.is_some());
            assert_eq!(
                fs::read_to_string(project.join("src/Enums/Status/object.yaml")).unwrap(),
                authored
            );
            assert_eq!(fs::read(project.join("src/source.json")).unwrap(), manifest);
        }
    }
}

/// A remaining decoded XML dependency is a locale-independent refusal even with explicit exclusions.
#[test]
fn resource_reference_refuses_removal_in_both_locales_before_generation() {
    let test = TestDir::new();
    fixture(&test.0, true);
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
    let authored = remove(&project);
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
