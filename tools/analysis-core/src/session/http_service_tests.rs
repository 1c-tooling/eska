//! HTTP routes and the module backlink share one current owner revision.

#[path = "../../tests/support/http_service_project.rs"]
mod fixture;

use super::*;
use crate::ModuleState;
use crate::session::tests::{owner, support::TestDirectory};

/// Observe the real tracked resolver, separately from the projection retained for the metadata tree.
fn service(session: &AnalysisSession, path: &SourcePath) -> Arc<bsl_metadata::HTTPService> {
    let Some(CanonicalMetadata::HTTPService(value)) =
        session.resolve(path, session.revision()).unwrap()
    else {
        panic!("expected current service");
    };
    value
}

/// The body backlink uses declared file identity, never module text or an XML path lookup.
fn service_by_file(
    session: &AnalysisSession,
    path: &SourcePath,
) -> Option<Arc<bsl_metadata::HTTPService>> {
    ide_db::metadata::resolve_http_service_by_file(
        &session.database,
        session.listing,
        session.modules.get(path).unwrap().file,
    )
}

/// Routes and handlers update by name and module; failed editors clear both without touching a neighbor.
#[test]
fn http_services_use_current_yaml_and_the_declared_module_backlink() {
    let directory = TestDirectory::new();
    let root = crate::test_project::project(&directory.0, "// Current body.\n", false);
    fixture::add_http_services(&root, &directory.0.join("native"));
    let source = root.join("src-with-http");
    let mut session = AnalysisSession::open(&source).unwrap();
    let path = owner("HTTPServices/API/object.yaml");
    let neighbor = owner("HTTPServices/Neighbor/object.yaml");
    let module = owner("HTTPServices/API/Ext/Module.bsl");
    let request = InputRequest::default();
    assert!(service_by_file(&session, &module).is_none());
    session
        .load_saved(&path, session.revision(), &request)
        .unwrap();
    session
        .load_saved(&neighbor, session.revision(), &request)
        .unwrap();
    let saved = service(&session, &path);
    assert!(Arc::ptr_eq(
        &saved,
        &service_by_file(&session, &module).unwrap()
    ));
    assert!(matches!(
        session.module(&module).unwrap().state,
        ModuleState::Pending
    ));
    let untouched = service(&session, &neighbor);
    let text = std::fs::read_to_string(source.join(path.as_path())).unwrap();
    let native = std::fs::read_to_string(directory.0.join("native/HTTPServices/API.xml")).unwrap();
    assert_eq!(
        *saved,
        bsl_metadata::xml_parser::parse_http_service_xml(&native, "API").unwrap()
    );
    let changed = text
        .replace("<RootURL>saved</RootURL>", "<RootURL>current</RootURL>")
        .replace(
            "<Template>/saved</Template>",
            "<Template>/current/{id}</Template>",
        )
        .replace(
            "<HTTPMethod>GET</HTTPMethod>",
            "<HTTPMethod>POST</HTTPMethod>",
        )
        .replace(
            "<Handler>PingGet</Handler>",
            "<Handler>CurrentPost</Handler>",
        );
    assert_ne!(changed, text);
    session
        .apply_overlay(&path, session.revision(), 7, &changed, &request)
        .unwrap();
    let current = service(&session, &path);
    assert_eq!(current.root_url(), "current");
    assert_eq!(current.url_templates()[0].template(), "/current/{id}");
    assert_eq!(
        current.url_templates()[0].methods()[0].http_method(),
        "POST"
    );
    assert_eq!(
        current.url_templates()[0].methods()[0].handler(),
        "CurrentPost"
    );
    assert!(Arc::ptr_eq(
        &current,
        &service_by_file(&session, &module).unwrap()
    ));
    assert!(Arc::ptr_eq(&untouched, &service(&session, &neighbor)));
    let invalid = changed.replace(
        "<Handler>CurrentPost</Handler>",
        "<Handler><Future/></Handler>",
    );
    session
        .apply_overlay(&path, session.revision(), 8, &invalid, &request)
        .unwrap();
    assert!(matches!(
        session.owner(&path).unwrap().state,
        ModelState::Invalid { .. }
    ));
    assert!(service_by_file(&session, &module).is_none());
    session
        .apply_overlay(&path, session.revision(), 9, "document: [", &request)
        .unwrap();
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    assert!(service_by_file(&session, &module).is_none());
    session
        .close_overlay(&path, session.revision(), &request)
        .unwrap();
    assert_eq!(*service(&session, &path), *saved);
    assert!(session.summary().descriptor_models_complete);
    assert_eq!(
        std::fs::read_to_string(source.join(path.as_path())).unwrap(),
        text
    );
}
