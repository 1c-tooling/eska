//! Saved native `GetFunctionalOption` inference reaches the actual checker with locale-independent findings.

#[path = "../support/functional_option_project.rs"]
mod fixture;

use super::*;

/// Current reference storage gives real missing-field diagnostics through the native global-call result.
#[test]
fn functional_option_current_reference_storage_reaches_checker_ruen() {
    let body = "Procedure Run()\n Var Chosen;\n Chosen=GetFunctionalOption(\"FeatureЯ\");\n Result=Chosen.Absent;\nEndProcedure\n";
    let (directory, root) = support::project(body, false);
    fixture::add_options(&root, &directory.0.join("native"));
    settings(&root, &[DiagnosticCode::UnresolvedField]);
    let source = root.join("src-with-options");
    let constant = source.join("Constants/Storage/object.yaml");
    let saved = fs::read_to_string(&constant).unwrap();
    let reference = saved.replace("x:string", "cfg:CatalogRef.Items");
    assert_ne!(reference, saved);
    fs::write(&constant, reference).unwrap();
    let mut results = Vec::new();
    for locale in ["ru-RU", "en-US"] {
        let value = report(
            &run(
                &root,
                locale,
                &["--profile", "semantic", "--format", "json"],
            ),
            1,
        );
        assert_eq!(
            value["data"]["report"]["coverage"]["semantic_profile_complete"], true,
            "{value}"
        );
        let module = value["data"]["report"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|file| file["source"] == "CommonModules/Tools/Module.bsl")
            .unwrap();
        assert_eq!(module["findings"].as_array().unwrap().len(), 1, "{module}");
        assert_eq!(module["findings"][0]["code"], "UnresolvedField");
        results.push(value);
    }
    assert_eq!(results[0], results[1]);
    assert_eq!(
        fs::read_to_string(source.join("CommonModules/Tools/Module.bsl")).unwrap(),
        body
    );
}

#[path = "../support/functional_option_parameter_project.rs"]
mod parameters_fixture;

/// Native mutable-structure leaf hints stay soft, while an invalid current parameter prevents complete batch success.
#[test]
fn functional_option_parameter_checker_keeps_soft_values_and_current_failure_ruen() {
    let body = "&AtClient\nProcedure Run()\n Params=GetInterfaceFunctionalOptionParameters();\n Value=Month(Params.SettingЯ);\nEndProcedure\n";
    let (directory, root) = support::project(body, false);
    let native = directory.0.join("native");
    fixture::add_options(&root, &native);
    parameters_fixture::add_parameters(&root, &native);
    settings(&root, &[DiagnosticCode::TypeMismatch]);
    let source = root.join("src-with-option-parameters");
    let register = source.join("InformationRegisters/Settings/object.yaml");
    let saved = fs::read_to_string(&register).unwrap();
    let boolean = saved.replace("x:string", "x:boolean");
    assert_ne!(saved, boolean);
    fs::write(register, boolean).unwrap();
    let mut results = Vec::new();
    for locale in ["ru-RU", "en-US"] {
        let value = report(
            &run(
                &root,
                locale,
                &["--profile", "semantic", "--format", "json"],
            ),
            0,
        );
        assert_eq!(
            value["data"]["report"]["coverage"]["semantic_profile_complete"], true,
            "{value}"
        );
        results.push(value);
    }
    assert_eq!(results[0], results[1]);
    // A known incompatible scalar proves the real global's date parameter is checked.
    let module = source.join("CommonModules/Tools/Module.bsl");
    fs::write(&module, body.replace("Params.SettingЯ", "False")).unwrap();
    for locale in ["ru-RU", "en-US"] {
        let value = report(
            &run(
                &root,
                locale,
                &["--profile", "semantic", "--format", "json"],
            ),
            1,
        );
        assert!(
            value["data"]["report"]["files"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|file| file["findings"].as_array().unwrap())
                .any(|finding| finding["code"] == "TypeMismatch")
        );
    }
    fs::write(&module, body).unwrap();
    let parameter = source.join("FunctionalOptionsParameters/SettingЯ/object.yaml");
    let saved = fs::read_to_string(&parameter).unwrap();
    let invalid = saved.replace(
        "InformationRegister.Settings.Dimension.Key",
        "Document.Orders",
    );
    assert_ne!(saved, invalid);
    fs::write(parameter, invalid).unwrap();
    let mut results = Vec::new();
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
            value["data"]["report"]["coverage"]["semantic_profile_complete"], false,
            "{value}"
        );
        results.push(value);
    }
    assert_eq!(results[0], results[1]);
}
