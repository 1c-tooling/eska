//! Receive bindings update through both resolver entry points and the actual native diagnostic.

#[path = "../../tests/support/integration_service_project.rs"]
pub mod fixture;

use super::*;
use crate::session::tests::{owner, support::TestDirectory};
use crate::{DiagnosticCode, DiagnosticsConfig, ModuleState, QueryCancellation};

/// Inspect the tracked kernel value rather than only the metadata-tree projection.
fn service(session: &AnalysisSession, path: &SourcePath) -> Arc<bsl_metadata::IntegrationService> {
    let Some(CanonicalMetadata::IntegrationService(value)) =
        session.resolve(path, session.revision()).unwrap()
    else {
        panic!("expected integration service");
    };
    value
}

/// The manifest provides the body identity before its text is loaded.
fn service_by_file(
    session: &AnalysisSession,
    path: &SourcePath,
) -> Option<Arc<bsl_metadata::IntegrationService>> {
    ide_db::metadata::resolve_integration_service_by_file(
        &session.database,
        session.selected_root.listing,
        session.selected_root.modules.get(path).unwrap().file,
    )
}

/// Native `UnusedLocalMethod` consumes the current receive-handler binding, with source text unchanged.
fn unused_methods(session: &AnalysisSession, path: &SourcePath) -> Vec<crate::Diagnostic> {
    session
        .module_diagnostics(
            path,
            session.revision(),
            &DiagnosticsConfig {
                only_enabled: Some(vec![DiagnosticCode::UnusedLocalMethod]),
                ..DiagnosticsConfig::default()
            },
            &QueryCancellation::default(),
        )
        .unwrap()
        .value
}

/// Check the actual unused method's source name, not only the diagnostic count.
fn assert_unused_method(
    session: &AnalysisSession,
    path: &SourcePath,
    body: &str,
    name: &str,
) -> Vec<crate::Diagnostic> {
    let diagnostics = unused_methods(session, path);
    assert_eq!(diagnostics.len(), 1);
    let range = diagnostics[0].range;
    assert_eq!(
        &body[usize::from(range.start())..usize::from(range.end())],
        name
    );
    diagnostics
}

/// Invalid editor state clears both resolvers; semantic preparation cannot revive saved bindings.
#[test]
fn integration_channels_update_by_name_body_and_semantic_handler_use() {
    let directory = TestDirectory::new();
    let root = crate::test_project::project(&directory.0, "// Current body.\n", false);
    fixture::add_integration_services(&root, &directory.0.join("native"));
    let source = root.join("src-with-integration");
    let mut session = AnalysisSession::open(&source).unwrap();
    let path = owner("IntegrationServices/Bus/object.yaml");
    let neighbor = owner("IntegrationServices/Neighbor/object.yaml");
    let module = owner("IntegrationServices/Bus/Ext/Module.bsl");
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
    let native =
        std::fs::read_to_string(directory.0.join("native/IntegrationServices/Bus.xml")).unwrap();
    assert_eq!(
        *saved,
        bsl_metadata::xml_parser::parse_integration_service_xml(&native, "Bus").unwrap()
    );
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    let body = std::fs::read_to_string(source.join(module.as_path())).unwrap();
    let initial = assert_unused_method(&session, &module, &body, "CurrentReceive");
    let changed = text
        .replace(
            "<ReceiveMessageProcessing>SavedReceive</ReceiveMessageProcessing>",
            "<ReceiveMessageProcessing>CurrentReceive</ReceiveMessageProcessing>",
        )
        .replacen("Incoming", "CurrentIncoming", 1);
    assert_ne!(changed, text);
    session
        .apply_overlay(&path, session.revision(), 7, &changed, &request)
        .unwrap();
    let current = service(&session, &path);
    assert_eq!(current.channels()[0].name(), "CurrentIncoming");
    assert_eq!(
        current.receive_handlers().collect::<Vec<_>>(),
        ["CurrentReceive"]
    );
    assert!(Arc::ptr_eq(
        &current,
        &service_by_file(&session, &module).unwrap()
    ));
    assert!(Arc::ptr_eq(&untouched, &service(&session, &neighbor)));
    assert_unused_method(&session, &module, &body, "SavedReceive");
    let invalid = changed.replace(
        "<ReceiveMessageProcessing>CurrentReceive</ReceiveMessageProcessing>",
        "<ReceiveMessageProcessing><Future/></ReceiveMessageProcessing>",
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
    assert_eq!(unused_methods(&session, &module), initial);
    assert!(session.summary().descriptor_models_complete);
    assert_eq!(
        std::fs::read_to_string(source.join(path.as_path())).unwrap(),
        text
    );
    assert_eq!(
        std::fs::read_to_string(source.join(module.as_path())).unwrap(),
        body
    );
}

/// Both entry points recognize a platform-bound private handler despite the absence of a BSL caller.
#[test]
fn integration_saved_batch_matches_resident_handler_uses_on_both_locales() {
    let directory = TestDirectory::new();
    let root = crate::test_project::project(&directory.0, "// Current body.\n", false);
    fixture::add_integration_services(&root, &directory.0.join("native"));
    let source = root.join("src-with-integration");
    let mut session = AnalysisSession::open(&source).unwrap();
    let module = owner("IntegrationServices/Bus/Ext/Module.bsl");
    session
        .load_semantic_inputs(session.revision(), &InputRequest::default())
        .unwrap();
    for locale in [crate::Locale::Ru, crate::Locale::En] {
        let config = DiagnosticsConfig {
            locale,
            only_enabled: Some(vec![DiagnosticCode::UnusedLocalMethod]),
            ..DiagnosticsConfig::default()
        };
        let expected = session
            .module_diagnostics(
                &module,
                session.revision(),
                &config,
                &QueryCancellation::default(),
            )
            .unwrap();
        let report = crate::batch::check_semantic(
            SourceReader::open(&source).unwrap(),
            &config,
            &crate::batch::BatchOptions::default(),
        )
        .unwrap();
        assert_eq!(report.coverage.semantic_profile_complete, Some(true));
        let actual = report
            .files
            .iter()
            .find(|file| file.source == module)
            .unwrap();
        assert_eq!(actual.source_sha256.as_ref(), Some(&expected.source_sha256));
        assert_eq!(expected.value.len(), 1);
        assert_eq!(actual.findings.len(), 1);
        let finding = &actual.findings[0];
        assert_eq!(finding.code, "UnusedLocalMethod");
        assert_eq!(
            finding.start_byte,
            u32::from(expected.value[0].range.start())
        );
        assert_eq!(finding.end_byte, u32::from(expected.value[0].range.end()));
        assert_eq!(finding.message, expected.value[0].message);
    }
}
