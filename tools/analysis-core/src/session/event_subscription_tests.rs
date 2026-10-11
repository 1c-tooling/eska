//! Current subscription metadata reaches tracked kernel slots through the shared YAML lifecycle.

#[path = "../../tests/support/event_subscription_project.rs"]
mod fixture;

use super::*;
use crate::session::tests::{owner, support::TestDirectory};

/// Require a ready native value from the actual canonical resolver, not a cached test-side model.
fn subscription(
    session: &AnalysisSession,
    path: &SourcePath,
) -> Arc<bsl_metadata::EventSubscription> {
    let Some(CanonicalMetadata::EventSubscription(value)) =
        session.resolve(path, session.revision()).unwrap()
    else {
        panic!("expected current subscription");
    };
    value
}

/// Saved, editor, malformed, unavailable and close all publish the same current callback/source state.
#[test]
fn subscriptions_use_current_yaml_and_never_reuse_a_failed_editor_model() {
    let directory = TestDirectory::new();
    let root = crate::test_project::project(&directory.0, "// Current body.\n", false);
    fixture::add_subscriptions(&root, &directory.0.join("native"));
    let source = root.join("src-with-subscriptions");
    let mut session = AnalysisSession::open(&source).unwrap();
    let path = owner("EventSubscriptions/BeforeSave/object.yaml");
    let neighbor = owner("EventSubscriptions/Neighbor/object.yaml");
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
    let saved = subscription(&session, &path);
    let untouched = subscription(&session, &neighbor);
    let text = std::fs::read_to_string(source.join(path.as_path())).unwrap();
    let native =
        std::fs::read_to_string(directory.0.join("native/EventSubscriptions/BeforeSave.xml"))
            .unwrap();
    assert_eq!(
        *saved,
        bsl_metadata::xml_parser::parse_event_subscription_xml(&native).unwrap()
    );
    let changed = text
        .replace("CommonModule.Tools.Saved", "CommonModule.Tools.Current")
        .replace("BeforeWrite", "OnWrite")
        .replace("cfg:CatalogObject.Items", "cfg:CatalogObject.CurrentItems");
    assert_ne!(changed, text);
    session
        .apply_overlay(&path, session.revision(), 7, &changed, &request)
        .unwrap();
    let current = subscription(&session, &path);
    assert_eq!(current.handler_string(), "CommonModule.Tools.Current");
    assert_eq!(current.event(), "OnWrite");
    assert_eq!(
        current.source(),
        "cfg:CatalogObject.CurrentItems;cfg:DocumentObject"
    );
    assert_eq!(saved.source(), "cfg:CatalogObject.Items;cfg:DocumentObject");
    assert_eq!(saved.handler_string(), "CommonModule.Tools.Saved");
    assert!(Arc::ptr_eq(&untouched, &subscription(&session, &neighbor)));
    let invalid = text.replace("cfg:CatalogObject.Items", "unbound:CatalogObject.Items");
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
    assert!(!session.summary().descriptor_models_complete);
    session
        .apply_overlay(&path, session.revision(), 9, "document: [", &request)
        .unwrap();
    assert!(
        session
            .resolve(&path, session.revision())
            .unwrap()
            .is_none()
    );
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
    assert_eq!(*subscription(&session, &path), *saved);
    assert!(session.summary().descriptor_models_complete);
    assert_eq!(
        std::fs::read_to_string(source.join(path.as_path())).unwrap(),
        text
    );
}
