//! The offline CLI validates fresh native identity without treating working data as accepted policy.

use super::*;
use eska::project_core::source_format::{
    Child, NativeUuid, ObjectSource, Property, PropertyName, PropertyValue, Section,
};

const OWNER: &str = "20000000-0000-4000-8000-000000000001";
const CREATED: &str = "40000000-0000-4000-8000-000000000001";

/// The minimal native profile contains every property required for new enum-value construction.
pub fn fixture(root: &Path) {
    fs::create_dir_all(root.join("native/Enums")).unwrap();
    fs::write(root.join("native/Configuration.xml"),
        "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Configuration uuid=\"10000000-0000-4000-8000-000000000001\"><Properties><Name>Sample</Name></Properties><ChildObjects><Enum>Status</Enum></ChildObjects></Configuration></MetaDataObject>"
    ).unwrap();
    fs::write(root.join("native/Enums/Status.xml"), format!(
        "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Enum uuid=\"{OWNER}\"><Properties><Name>Status</Name></Properties><ChildObjects><EnumValue uuid=\"30000000-0000-4000-8000-000000000001\"><Properties><Name>First</Name><Synonym/><Comment/></Properties></EnumValue></ChildObjects></Enum></MetaDataObject>"
    )).unwrap();
}

/// Raw authoring keeps annotation/EOL and adds one value without changing the mandatory reverse map.
fn append(root: &Path, name: &str) -> String {
    let file = root.join("src/Enums/Status/object.yaml");
    let mut owner = ObjectSource::from_yaml(&fs::read_to_string(&file).unwrap()).unwrap();
    let children = owner
        .document
        .descriptor
        .object
        .sections
        .iter_mut()
        .find_map(|section| {
            if let Section::Children(children) = section {
                Some(children)
            } else {
                None
            }
        })
        .unwrap();
    let Child::Inline { mut object } = children[0].clone() else {
        unreachable!()
    };
    object.uuid = NativeUuid::parse(CREATED).unwrap();
    let Section::Properties(properties) = &mut object.sections[0] else {
        unreachable!()
    };
    for property in properties {
        if let Property::Typed {
            name: PropertyName::Name,
            value,
        } = property
        {
            *value = PropertyValue::Text(name.into());
        }
    }
    children.push(Child::Inline { object });
    let text = format!(
        "# Keep annotation\r\n{}",
        owner.to_yaml().unwrap().replace('\n', "\r\n")
    );
    fs::write(file, &text).unwrap();
    text
}

/// The same creation/freeze policy and stable diagnostics apply to both layouts and both locales.
#[test]
fn prepare_creates_value_or_reports_owner_freeze_without_rewriting_authoring_bytes() {
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
            let authored = append(&project, "Новое_Значение");
            let args = ["prepare", "--accepted-base-sha256", pin, "--format", "json"];
            let reports: Vec<_> = ["ru", "en"]
                .iter()
                .map(|locale| {
                    let output = tool(&project, locale, &args);
                    assert_eq!(output.status.code(), Some(i32::from(frozen)), "{output:?}");
                    assert_eq!(output.stderr.is_empty(), !frozen);
                    serde_json::from_slice::<Value>(&output.stdout).unwrap()
                })
                .collect();
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
                reports[0].clone()
            };
            let xml = fs::read_to_string(
                Path::new(selected["data"]["xml"].as_str().unwrap()).join("Enums/Status.xml"),
            )
            .unwrap();
            assert_eq!(xml.contains(CREATED), !frozen);
            assert_eq!(xml.contains("Новое_Значение"), !frozen);
            assert_eq!(
                fs::read_to_string(project.join("src/Enums/Status/object.yaml")).unwrap(),
                authored
            );
            assert_eq!(fs::read(project.join("src/source.json")).unwrap(), manifest);
        }
    }
}

/// Invalid new names produce a structured locale-independent failure with no partial generation.
#[test]
fn invalid_new_name_refuses_identically_in_ru_and_en() {
    let test = TestDir::new();
    fixture(&test.0);
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
    let authored = append(&project, "Invalid name");
    let args = ["prepare", "--accepted-base-sha256", pin, "--format", "json"];
    let mut reports = Vec::new();
    for locale in ["ru", "en"] {
        let output = tool(&project, locale, &args);
        assert_eq!(output.status.code(), Some(1));
        assert!(!output.stderr.is_empty());
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["error"]["code"], "source_metadata_name_invalid");
        reports.push(report);
    }
    assert_eq!(reports[0], reports[1]);
    assert!(!project.join(".eska/generations").exists());
    assert_eq!(
        fs::read_to_string(project.join("src/Enums/Status/object.yaml")).unwrap(),
        authored
    );
}
