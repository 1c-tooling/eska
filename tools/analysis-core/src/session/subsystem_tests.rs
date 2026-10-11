//! Current main subsystem models share the normal YAML lifecycle and retain explicit nested coverage.

#[path = "../../tests/support/subsystem_project.rs"]
mod fixture;

use super::*;
use crate::session::tests::{owner, support::TestDirectory};

/// Query the tracked native resolver rather than rebuilding the expected adapter value.
fn subsystem(session: &AnalysisSession, path: &SourcePath) -> Arc<bsl_metadata::Subsystem> {
    let Some(CanonicalMetadata::Subsystem(value)) =
        session.resolve(path, session.revision()).unwrap()
    else {
        panic!("expected current subsystem");
    };
    value
}

/// Content changes replace only their owner; invalid editor input never revives an old graph value.
#[test]
fn subsystems_use_current_yaml_and_preserve_unrelated_models() {
    let directory = TestDirectory::new();
    let root = crate::test_project::project(&directory.0, "// Current body.\n", false);
    fixture::add_subsystems(&root, &directory.0.join("native"));
    let source = root.join("src-with-subsystems");
    let mut session = AnalysisSession::open(&source).unwrap();
    let path = owner("Subsystems/Services/object.yaml");
    let neighbor = owner("Subsystems/Neighbor/object.yaml");
    let request = InputRequest::default();
    assert!(
        session
            .resolve(&path, session.revision())
            .unwrap()
            .is_none()
    );
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    assert!(session.summary().descriptor_models_complete);
    let saved = subsystem(&session, &path);
    let untouched = subsystem(&session, &neighbor);
    let text = std::fs::read_to_string(source.join(path.as_path())).unwrap();
    let native =
        std::fs::read_to_string(directory.0.join("native/Subsystems/Services.xml")).unwrap();
    assert_eq!(
        *saved,
        bsl_metadata::xml_parser::parse_subsystem_xml(&native).unwrap()
    );
    let changed = text.replace("Catalog.Items", "Document.Current");
    assert_ne!(changed, text);
    session
        .apply_overlay(&path, session.revision(), 7, &changed, &request)
        .unwrap();
    let current = subsystem(&session, &path);
    assert_eq!(
        current.content(),
        &[(bsl_metadata::MdoType::Document, "Current".into())]
    );
    assert_eq!(
        saved.content(),
        &[(bsl_metadata::MdoType::Catalog, "Items".into())]
    );
    assert!(Arc::ptr_eq(&untouched, &subsystem(&session, &neighbor)));
    let invalid = changed.replace("xr:Item", "xr:Future");
    session
        .apply_overlay(&path, session.revision(), 8, &invalid, &request)
        .unwrap();
    assert!(matches!(
        session.owner(&path).unwrap().state,
        ModelState::Invalid { .. }
    ));
    assert!(
        session
            .resolve(&path, session.revision())
            .unwrap()
            .is_none()
    );
    session
        .apply_overlay(&path, session.revision(), 9, "document: [", &request)
        .unwrap();
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    assert!(
        session
            .resolve(&path, session.revision())
            .unwrap()
            .is_none()
    );
    session
        .close_overlay(&path, session.revision(), &request)
        .unwrap();
    assert_eq!(*subsystem(&session, &path), *saved);
    assert!(session.summary().descriptor_models_complete);
    assert_eq!(
        std::fs::read_to_string(source.join(path.as_path())).unwrap(),
        text
    );
}
