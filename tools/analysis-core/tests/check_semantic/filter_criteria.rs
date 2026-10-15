//! The actual checker consumes configured native Find inputs and reference results in both locales.

#[path = "../support/filter_criterion_project.rs"]
mod fixture;

use super::*;

/// Content and Type independently control native missing-field and argument-type findings.
#[test]
fn saved_filter_criteria_reach_checker_field_and_argument_diagnostics_ruen() {
    let body = "Procedure Run()\n Rows=FilterCriteria.Linked.Find(\"key\");\n Value=Rows[0].Choice;\nEndProcedure\n";
    let (directory, root) = support::project(body, false);
    fixture::add_criteria(
        &root,
        &directory.0.join("native"),
        false,
        "70000000-0000-4000-8000-000000000001",
        "",
    );
    settings(
        &root,
        &[
            DiagnosticCode::UnresolvedField,
            DiagnosticCode::TypeMismatch,
        ],
    );
    let source = root.join("src-with-criteria");
    let path = source.join("FilterCriteria/Linked/object.yaml");
    let saved = fs::read_to_string(&path).unwrap();
    for (yaml, code) in [
        (saved.clone(), None),
        (
            saved.replace(
                "Catalog.Items.Attribute.Key",
                "Document.Orders.Attribute.Key",
            ),
            Some("UnresolvedField"),
        ),
        (saved.replace("x:string", "x:boolean"), Some("TypeMismatch")),
    ] {
        fs::write(&path, &yaml).unwrap();
        let mut variants = Vec::new();
        for locale in ["ru-RU", "en-US"] {
            let value = report(
                &run(
                    &root,
                    locale,
                    &["--profile", "semantic", "--format", "json"],
                ),
                i32::from(code.is_some()),
            );
            let batch = &value["data"]["report"];
            assert_eq!(
                batch["coverage"]["semantic_profile_complete"], true,
                "{value}"
            );
            let file = batch["files"]
                .as_array()
                .unwrap()
                .iter()
                .find(|file| file["source"] == "CommonModules/Tools/Module.bsl")
                .unwrap();
            let findings = file["findings"].as_array().unwrap();
            assert_eq!(findings.len(), usize::from(code.is_some()), "{file}");
            if let Some(code) = code {
                assert_eq!(findings[0]["code"], code, "{file}");
            }
            variants.push(value);
        }
        assert_eq!(variants[0], variants[1]);
    }
    assert_eq!(
        fs::read_to_string(source.join("CommonModules/Tools/Module.bsl")).unwrap(),
        body
    );
}
