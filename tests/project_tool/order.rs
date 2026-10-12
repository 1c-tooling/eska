//! The offline CLI applies permitted native order and exposes refusals without localized JSON.

use super::*;
use eska::project_core::source_format::{Child, ObjectSource, Section};

/// A native enum value list has no payload or generated identity that could change during order edits.
fn fixture(root: &Path) {
    fs::create_dir_all(root.join("native/Enums")).unwrap();
    fs::write(root.join("native/Configuration.xml"),
        "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Configuration uuid=\"10000000-0000-4000-8000-000000000001\"><Properties><Name>Sample</Name></Properties><ChildObjects><Enum>Status</Enum></ChildObjects></Configuration></MetaDataObject>"
    ).unwrap();
    fs::write(root.join("native/Enums/Status.xml"),
        "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Enum uuid=\"20000000-0000-4000-8000-000000000001\"><Properties><Name>Status</Name></Properties><ChildObjects><EnumValue uuid=\"30000000-0000-4000-8000-000000000001\"><Properties><Name>First</Name></Properties></EnumValue><EnumValue uuid=\"30000000-0000-4000-8000-000000000002\"><Properties><Name>Second</Name></Properties></EnumValue></ChildObjects></Enum></MetaDataObject>"
    ).unwrap();
}

/// Authored comments/EOL and the source map stay separate from derived native order.
fn reverse(root: &Path) -> String {
    let file = root.join("src/Enums/Status/object.yaml");
    let mut value = ObjectSource::from_yaml(&fs::read_to_string(&file).unwrap()).unwrap();
    for section in &mut value.document.descriptor.object.sections {
        if let Section::Children(children) = section {
            assert!(
                children
                    .iter()
                    .all(|child| matches!(child, Child::Inline { .. }))
            );
            children.reverse();
        }
    }
    let text = format!(
        "# Preserve comments\r\n{}",
        value.to_yaml().unwrap().replace('\n', "\r\n")
    );
    fs::write(file, &text).unwrap();
    text
}

/// Both layouts and locales share order selection; freeze adds one stable reorder diagnostic.
#[test]
fn prepare_applies_order_or_reports_freeze_without_rewriting_sources() {
    for layout in ["family", "readable"] {
        for frozen in [false, true] {
            let test = TestDir::new();
            fixture(&test.0);
            let mut args = vec![
                "import", "native", "project", "--name", "sample", "--layout", layout, "--format",
                "json",
            ];
            if frozen {
                args.extend(["--freeze", "30000000-0000-4000-8000-000000000001"]);
            }
            let imported = success(&test.0, "ru", &args);
            let pin = imported["data"]["accepted_base_sha256"].as_str().unwrap();
            let root = test.0.join("project");
            let working = reverse(&root);
            let args = ["prepare", "--accepted-base-sha256", pin, "--format", "json"];
            let reports: Vec<_> = ["ru", "en"]
                .iter()
                .map(|locale| {
                    let output = tool(&root, locale, &args);
                    assert_eq!(output.status.code(), Some(i32::from(frozen)), "{output:?}");
                    assert_eq!(output.stderr.is_empty(), !frozen);
                    serde_json::from_slice::<Value>(&output.stdout).unwrap()
                })
                .collect();
            assert_eq!(reports[0], reports[1]);
            let selected = if frozen {
                assert!(!root.join(".eska/generations").exists());
                assert_eq!(reports[0]["error"]["code"], "source_changes_excluded");
                let diagnostics = reports[0]["error"]["diagnostics"].as_array().unwrap();
                assert_eq!(diagnostics.len(), 1);
                assert_eq!(diagnostics[0]["operation"]["kind"], "reorder");
                assert_eq!(diagnostics[0]["reason"], "eska_frozen");
                let recovery = [
                    "prepare",
                    "--accepted-base-sha256",
                    pin,
                    "--allow-exclusions",
                    "--format",
                    "json",
                ];
                let report = success(&root, "ru", &recovery);
                assert_eq!(report, success(&root, "en", &recovery));
                report
            } else {
                assert_eq!(reports[0]["data"]["exclusions"], serde_json::json!([]));
                reports[0].clone()
            };
            let xml = Path::new(selected["data"]["xml"].as_str().unwrap()).join("Enums/Status.xml");
            let xml = fs::read_to_string(xml).unwrap();
            assert_eq!(
                xml.find("<Name>First</Name>").unwrap() < xml.find("<Name>Second</Name>").unwrap(),
                frozen
            );
            assert_eq!(
                fs::read_to_string(root.join("src/Enums/Status/object.yaml")).unwrap(),
                working
            );
        }
    }
}
