//! The offline project CLI applies the same filling-reference validation and protection in both locales.

use super::*;

const FIRST: &str = "Enum.Status.EnumValue.First";
const SECOND: &str = "Enum.Status.EnumValue.Second";
const CATALOG: &str = "50000000-0000-4000-8000-000000000001";
const ATTRIBUTE: &str = "60000000-0000-4000-8000-000000000001";

/// Declared native targets are separate from one editable dependent field; no platform process is involved.
fn fixture(root: &Path) {
    super::creation::fixture(root);
    let config = root.join("native/Configuration.xml");
    fs::write(
        &config,
        fs::read_to_string(&config)
            .unwrap()
            .replace("</ChildObjects>", "<Catalog>Items</Catalog></ChildObjects>"),
    )
    .unwrap();
    let enumeration = root.join("native/Enums/Status.xml");
    fs::write(&enumeration,fs::read_to_string(&enumeration).unwrap().replace("</ChildObjects>","<EnumValue uuid=\"40000000-0000-4000-8000-000000000001\"><Properties><Name>Second</Name><Synonym/><Comment/></Properties></EnumValue></ChildObjects>")).unwrap();
    fs::create_dir(root.join("native/Catalogs")).unwrap();
    fs::write(root.join("native/Catalogs/Items.xml"),format!(
        "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" xmlns:i=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:r=\"http://v8.1c.ru/8.3/xcf/readable\" version=\"2.20\"><Catalog uuid=\"{CATALOG}\"><Properties><Name>Items</Name></Properties><ChildObjects><Attribute uuid=\"{ATTRIBUTE}\"><Properties><Name>Status</Name><FillValue i:type=\"r:DesignTimeRef\">{FIRST}</FillValue></Properties></Attribute></ChildObjects></Catalog></MetaDataObject>"
    )).unwrap();
}

/// Raw YAML authoring retains comments/transport and changes only the single editable reference.
fn author(project: &Path, target: &str) -> String {
    let path = project.join("src/Catalogs/Items/object.yaml");
    let text = format!(
        "# Keep annotation\r\n{}",
        fs::read_to_string(&path)
            .unwrap()
            .replace(FIRST, target)
            .replace('\n', "\r\n")
    );
    fs::write(path, &text).unwrap();
    text
}

/// Both layouts/locales retain `QName` helpers, freeze behavior, inputs and the public JSON envelope.
#[test]
fn prepare_selects_or_restores_named_fill_value_under_owner_and_value_freeze() {
    for layout in ["family", "readable"] {
        for frozen in [None, Some(CATALOG), Some(ATTRIBUTE)] {
            let directory = TestDir::new();
            fixture(&directory.0);
            let mut args = vec![
                "import", "native", "project", "--name", "sample", "--layout", layout, "--format",
                "json",
            ];
            if let Some(uuid) = frozen {
                args.extend(["--freeze", uuid]);
            }
            let imported = success(&directory.0, "en", &args);
            let pin = imported["data"]["accepted_base_sha256"].as_str().unwrap();
            let project = directory.0.join("project");
            let manifest = fs::read(project.join("src/source.json")).unwrap();
            let authored = author(&project, SECOND);
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
            let selected = if frozen.is_some() {
                assert!(!project.join(".eska/generations").exists());
                assert_eq!(reports[0]["error"]["code"], "source_changes_excluded");
                let exclusions = reports[0]["error"]["diagnostics"].as_array().unwrap();
                assert_eq!(exclusions.len(), 1);
                assert_eq!(
                    exclusions[0]["operation"],
                    serde_json::json!({"kind":"property","property":"FillValue"})
                );
                assert_eq!(exclusions[0]["reason"], "eska_frozen");
                let args = [
                    "prepare",
                    "--accepted-base-sha256",
                    pin,
                    "--allow-exclusions",
                    "--format",
                    "json",
                ];
                let restored = success(&project, "ru", &args);
                assert_eq!(restored, success(&project, "en", &args));
                restored
            } else {
                assert_eq!(reports[0]["data"]["exclusions"], serde_json::json!([]));
                reports[0].clone()
            };
            let xml = fs::read_to_string(
                Path::new(selected["data"]["xml"].as_str().unwrap()).join("Catalogs/Items.xml"),
            )
            .unwrap();
            assert!(xml.contains(&format!(
                "<FillValue i:type=\"r:DesignTimeRef\">{}</FillValue>",
                if frozen.is_some() { FIRST } else { SECOND }
            )));
            assert_eq!(
                fs::read_to_string(project.join("src/Catalogs/Items/object.yaml")).unwrap(),
                authored
            );
            assert_eq!(fs::read(project.join("src/source.json")).unwrap(), manifest);
        }
    }
}

/// Exclusions never authorize a missing or foreign-class dependency; failures precede publication.
#[test]
fn invalid_filling_target_refuses_before_generation_in_both_layouts_and_locales() {
    for layout in ["family", "readable"] {
        for (target, code) in [
            (
                "Enum.Status.EnumValue.Missing",
                "source_metadata_reference_missing",
            ),
            ("Catalog.Items", "source_metadata_reference_invalid"),
        ] {
            let directory = TestDir::new();
            fixture(&directory.0);
            let imported = success(
                &directory.0,
                "en",
                &[
                    "import", "native", "project", "--name", "sample", "--layout", layout,
                    "--format", "json",
                ],
            );
            let pin = imported["data"]["accepted_base_sha256"].as_str().unwrap();
            let project = directory.0.join("project");
            let authored = author(&project, target);
            for strict in [false, true] {
                let mut args = vec!["prepare", "--accepted-base-sha256", pin, "--format", "json"];
                if !strict {
                    args.push("--allow-exclusions");
                }
                let reports: Vec<_> = ["ru", "en"]
                    .iter()
                    .map(|locale| {
                        let output = tool(&project, locale, &args);
                        assert_eq!(output.status.code(), Some(1), "{output:?}");
                        assert!(!output.stderr.is_empty());
                        serde_json::from_slice::<Value>(&output.stdout).unwrap()
                    })
                    .collect();
                assert_eq!(reports[0], reports[1]);
                assert_eq!(reports[0]["error"]["code"], code);
                assert!(!project.join(".eska/generations").exists());
            }
            assert_eq!(
                fs::read_to_string(project.join("src/Catalogs/Items/object.yaml")).unwrap(),
                authored
            );
        }
    }
}
