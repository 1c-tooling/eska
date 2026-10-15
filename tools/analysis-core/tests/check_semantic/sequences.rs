use super::*;

#[path = "../support/sequence_project.rs"]
mod sequence_fixture;

/// Machine reports remain identical across locales for complete, finding and incomplete sequence states.
fn reports(root: &std::path::Path, exit: i32) -> serde_json::Value {
    let mut values = Vec::new();
    for locale in ["ru-RU", "en-US"] {
        values.push(report(
            &run(root, locale, &["--profile", "semantic", "--format", "json"]),
            exit,
        ));
    }
    assert_eq!(values[0], values[1]);
    values.remove(0)
}

/// Real native calls see saved sequence types, while an invalid declaration prevents complete batch success.
#[test]
fn sequence_record_type_and_current_failure_reach_checker_ruen() {
    let body = "Procedure Run()\n Rows=Sequences.Chain.CreateRecordSet();\n Row=Rows.Add();\n Value=Month(Row.KeyЯ);\nEndProcedure\n";
    let (directory, root) = support::project(body, false);
    sequence_fixture::add_sequence(
        &root,
        &directory.0.join("native"),
        "",
        false,
        "70000000-0000-4000-8000-000000000001",
        "",
    );
    settings(&root, &[DiagnosticCode::TypeMismatch]);
    let source = root.join("src-with-sequences");
    let sequence = source.join("Sequences/Chain/object.yaml");
    let saved = fs::read_to_string(&sequence)
        .unwrap()
        .replace("xs:string", "xs:dateTime");
    fs::write(&sequence, &saved).unwrap();
    let first = reports(&root, 0);
    assert_eq!(
        first["data"]["report"]["coverage"]["semantic_profile_complete"],
        true
    );
    let changed = saved.replace("xs:dateTime", "xs:boolean");
    assert_ne!(changed, saved);
    fs::write(&sequence, changed).unwrap();
    let current = reports(&root, 1);
    assert!(
        current["data"]["report"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|file| file["findings"].as_array().unwrap())
            .any(|finding| finding["code"] == "TypeMismatch")
    );
    fs::write(&sequence, "broken: [").unwrap();
    let incomplete = reports(&root, 2);
    assert_eq!(
        incomplete["data"]["report"]["coverage"]["semantic_profile_complete"],
        false
    );
    assert_eq!(
        fs::read_to_string(source.join("CommonModules/Tools/Module.bsl")).unwrap(),
        body
    );
}
