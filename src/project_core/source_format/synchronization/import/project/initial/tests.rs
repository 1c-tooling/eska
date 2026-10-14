//! The creation getter accepts only independently pinned initial candidates, never published history.

use super::*;
use crate::{
    project_core::{
        ProjectName, ProjectType,
        creation::{NewProjectOptions, new_project},
        native_version::ProjectVersion,
    },
    test_support::TestDir,
};

#[test]
/// Initial candidates work before enrollment; changed pins/source/control schemas cannot gain admission.
fn initial_only_lookup_refuses_wrong_pins_and_published_or_redirected_control_state() {
    let test = TestDir::new();
    let options = NewProjectOptions::new(
        ProjectType::Configuration,
        ProjectVersion::parse("0.1.0").unwrap(),
    );
    let created = new_project(
        &test.0.join("member"),
        ProjectName::parse("member".to_owned()).unwrap(),
        &options,
    )
    .unwrap();
    let pin = created.initial_ancestor.unwrap();
    assert!(open_initial_project_import_ancestor(&created.project, &pin).is_ok());
    let wrong = ImportAncestorRef::from_sha256(&"a".repeat(64)).unwrap();
    assert_eq!(
        open_initial_project_import_ancestor(&created.project, &wrong)
            .err()
            .unwrap()
            .code,
        "source_ancestor_untrusted"
    );
    let path = created.project.root().join(STATE);
    let original = fs::read(&path).unwrap();
    let mut redirected: serde_json::Value = serde_json::from_slice(&original).unwrap();
    redirected["source"] = serde_json::json!("other");
    for modified in [
        serde_json::json!({"schema_version":1,"source":"src","transaction":{"sha256":"b".repeat(64)},"ancestor":{"sha256":pin.sha256()}}),
        redirected,
    ] {
        fs::write(&path, serde_json::to_vec(&modified).unwrap()).unwrap();
        assert_eq!(
            open_initial_project_import_ancestor(&created.project, &pin)
                .err()
                .unwrap()
                .code,
            "source_import_state_invalid"
        );
        assert!(
            created
                .project
                .source()
                .join("configuration.yaml")
                .is_file()
        );
    }
    fs::write(&path, original).unwrap();
    assert!(open_initial_project_import_ancestor(&created.project, &pin).is_ok());
}
