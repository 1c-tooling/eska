//! Complete inline table removal uses the existing prepare protocol and accepted subtree permissions.

use super::attribute_fixture::{
    COLUMN, FIELD, OWNER, SHIFTED, STATIONARY, TABLE, fixture as field_fixture,
};
use super::*;
use eska::project_core::source_format::{Child, ObjectSource, Section};

const SERVICES: [&str; 4] = [
    "a0000000-0000-4000-8000-000000000001",
    "b0000000-0000-4000-8000-000000000001",
    "c0000000-0000-4000-8000-000000000001",
    "d0000000-0000-4000-8000-000000000001",
];

/// Explicit native declaration slots and a complete property profile replace only the private fixture's inert service bytes.
fn fixture(root: &Path, dependency: Option<&str>) {
    field_fixture(root, None);
    let path = root.join("native/Catalogs/Records.xml");
    let mut native = fs::read_to_string(&path).unwrap();
    native = native.replace(
        "version=\"2.20\"",
        "xmlns:xr=\"http://v8.1c.ru/8.3/xcf/readable\" version=\"2.20\"",
    );
    let declarations=[("TabularSection",0),("TabularSectionRow",2)].map(|(role,index)| {
        format!("<xr:GeneratedType name=\"Catalog{role}.Records.Rows\" category=\"{role}\"><xr:TypeId>{}</xr:TypeId><xr:ValueId>{}</xr:ValueId></xr:GeneratedType>",SERVICES[index],SERVICES[index+1])
    }).concat();
    native=native.replace("<Future>Retained table service bytes</Future>",&declarations).replace("<Name>Rows</Name></Properties>","<Name>Rows</Name><Synonym/><Comment/><ToolTip/><FillChecking>DontCheck</FillChecking><Use>ForItem</Use><LineNumberLength>5</LineNumberLength></Properties>");
    fs::write(path, native).unwrap();
    if let Some(kind) = dependency {
        let directory = root.join("native/Catalogs/Records/Ext");
        fs::create_dir_all(&directory).unwrap();
        let (path, body) = if kind == "xml" {
            (
                "Extra.xml",
                "<Form><DataPath><![CDATA[Объект.Rows]]></DataPath></Form>".into(),
            )
        } else if kind == "service" {
            ("Extra.xml", format!("<Root ref=\"{}\"/>", SERVICES[3]))
        } else {
            (
                "ObjectModule.bsl",
                "\u{feff}Object.Rows.Clear();\r\n".into(),
            )
        };
        fs::write(directory.join(path), body).unwrap();
    }
}

/// Authoring removes the whole declared subtree without touching manifest hashes, resources or accepted storage.
fn remove(root: &Path) -> String {
    let path = root.join("src/Catalogs/Records/object.yaml");
    let mut owner = ObjectSource::from_yaml(&fs::read_to_string(&path).unwrap()).unwrap();
    for section in &mut owner.document.descriptor.object.sections {
        if let Section::Children(children) = section {
            children.retain(
                |child| !matches!(child,Child::Inline { object } if object.uuid.as_str()==TABLE),
            );
        }
    }
    let text = format!(
        "# Retain table annotation\r\n{}",
        owner.to_yaml().unwrap().replace('\n', "\r\n")
    );
    fs::write(path, &text).unwrap();
    text
}

/// Parent/table/every column freeze restores removal; stationary root fields do not block an independent last table.
#[test]
fn prepare_deletes_or_restores_complete_tables_in_both_layouts_and_locales() {
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
            let authored = remove(&project);
            let denied =
                frozen.is_some_and(|uuid| [OWNER, TABLE, COLUMN, STATIONARY].contains(&uuid));
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
                assert_eq!(diagnostics[0]["operation"]["kind"], "delete");
                assert_eq!(diagnostics[0]["reason"], "eska_frozen");
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
            for uuid in [TABLE, COLUMN, STATIONARY].into_iter().chain(SERVICES) {
                assert_eq!(xml.contains(uuid), denied);
            }
            for uuid in [OWNER, FIELD, SHIFTED] {
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

/// Remaining table/service mentions refuse publication in every locale and recovery mode.
#[test]
fn table_and_service_dependencies_refuse_prepare_before_generation_in_both_locales() {
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
                    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
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
                    reports.push(report);
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
