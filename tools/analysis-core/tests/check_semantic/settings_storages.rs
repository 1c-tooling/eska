use super::*;

#[path = "../support/settings_storage_project.rs"]
mod storage_fixture;

/// Direct checker uses the same native current exports and refuses incomplete metadata in both locales.
#[test]
fn settings_storage_current_manager_and_failure_reach_checker_ruen() {
    let body = "Procedure Run()\n Manager=SettingsStorages.CurrentЯ;\n Manager.SavedMethod();\n Value=Manager.Load(\"key\");\nEndProcedure\n";
    let (directory, root) = support::project(body, false);
    storage_fixture::add_storage(
        &root,
        &directory.0.join("native"),
        storage_fixture::SAVED,
        false,
        "70000000-0000-4000-8000-000000000001",
        "",
    );
    settings(&root, &[DiagnosticCode::UnresolvedMethodCall]);
    let source = root.join("src-with-storages");
    let mut previous = None;
    for (manager, exit) in [(storage_fixture::SAVED, 0), (storage_fixture::CURRENT, 1)] {
        fs::write(
            source.join("SettingsStorages/CurrentЯ/ManagerModule.bsl"),
            manager,
        )
        .unwrap();
        let mut outputs = Vec::new();
        for locale in ["ru-RU", "en-US"] {
            let value = report(
                &run(
                    &root,
                    locale,
                    &["--profile", "semantic", "--format", "json"],
                ),
                exit,
            );
            if exit == 1 {
                assert!(
                    value["data"]["report"]["files"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .flat_map(|file| file["findings"].as_array().unwrap())
                        .any(|finding| finding["code"] == "UnresolvedMethodCall")
                );
            }
            outputs.push(value);
        }
        assert_eq!(outputs[0], outputs[1]);
        previous = Some(outputs.remove(0));
    }
    assert_eq!(
        previous.unwrap()["data"]["report"]["coverage"]["semantic_profile_complete"],
        true
    );
    fs::write(
        source.join("SettingsStorages/CurrentЯ/object.yaml"),
        "broken: [",
    )
    .unwrap();
    for locale in ["ru-RU", "en-US"] {
        let value = report(
            &run(
                &root,
                locale,
                &["--profile", "semantic", "--format", "json"],
            ),
            2,
        );
        assert_eq!(
            value["data"]["report"]["coverage"]["semantic_profile_complete"],
            false
        );
    }
    assert_eq!(
        fs::read_to_string(source.join("CommonModules/Tools/Module.bsl")).unwrap(),
        body
    );
}
