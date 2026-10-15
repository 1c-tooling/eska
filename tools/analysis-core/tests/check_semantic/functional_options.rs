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
