//! Traffic follows accepted host publications rather than reservation deltas or model readiness.

use std::{fs, path::PathBuf};

use eska::{
    analysis_core::{InputFailure, InputRequest},
    project_core::source_format::{SourcePath, SourceReader, import_designer_queries},
};

use crate::{
    AnalysisScope, AnalysisSession, MetadataResourceState, ModelState, ModuleFailure, ModuleState,
    QueryTextState, SessionOptions, SessionRevision, SessionTraffic, query_creation_fixture,
    session::tests::{fixture, owner},
    test_project, test_role_project,
    test_support::TestDirectory,
};

/// Assertions name both independent meters: repeated input bytes and accepted publications.
fn assert_traffic(session: &AnalysisSession, bytes: u64, publications: u64) {
    assert_eq!(
        session.input_traffic(),
        SessionTraffic {
            cumulative_ready_source_bytes: bytes,
            accepted_publications: publications,
        }
    );
    assert_eq!(session.revision().0, publications);
}

/// All rejected paths and an ignored overlay watch retain both cumulative meters.
fn assert_rejections_preserve_traffic(
    session: &mut AnalysisSession,
    metadata: &SourcePath,
    module: &SourcePath,
    yaml: &str,
) {
    let request = InputRequest::default();
    let traffic = session.input_traffic();
    let current = session.revision();
    assert!(
        session
            .apply_module_overlay(module, current, 3, "// Stale.", &request)
            .is_err()
    );
    assert!(
        session
            .apply_overlay(metadata, current, 1, yaml, &request)
            .is_err()
    );
    assert!(
        session
            .apply_module_overlay(
                &owner("undeclared.bsl"),
                current,
                4,
                "// Unknown.",
                &request
            )
            .is_err()
    );
    assert!(
        session
            .apply_module_overlay(module, SessionRevision::default(), 4, "// Stale.", &request)
            .is_err()
    );
    assert!(
        session
            .invalidate_saved_inputs(&[metadata.clone(), module.clone()], current)
            .unwrap()
            .is_empty()
    );
    session.trim_batch_caches();
    assert_eq!(session.input_traffic(), traffic);
}

/// A deduplicated saved watch releases each input once; already-pending inputs are true no-ops.
fn assert_saved_invalidation(
    session: &mut AnalysisSession,
    metadata: SourcePath,
    module: SourcePath,
    bytes: u64,
) {
    let changed = session
        .invalidate_saved_inputs(
            &[metadata.clone(), module.clone(), module.clone()],
            session.revision(),
        )
        .unwrap();
    assert_eq!(changed.len(), 2);
    assert_traffic(session, bytes, 8);
    assert!(
        session
            .invalidate_saved_inputs(&[metadata, module], session.revision())
            .unwrap()
            .is_empty()
    );
    assert_traffic(session, bytes, 8);
}

/// Keep role resources in a fresh existing import fixture without opening unrelated inputs.
fn role_host() -> (TestDirectory, PathBuf, AnalysisSession) {
    let directory = TestDirectory::new();
    let project = test_project::project(&directory.0, "// Body.\n", false);
    test_role_project::add_roles(&project, &directory.0.join("native"));
    let root = project.join("src-with-roles");
    let session = AnalysisSession::open(&root).unwrap();
    (directory, root, session)
}

/// Saved release and rejected resource reads advance publications without recharging the owner.
fn assert_resource_release_and_failure(
    session: &mut AnalysisSession,
    resource: &SourcePath,
    bytes: u64,
) {
    assert_eq!(
        session
            .invalidate_saved_inputs(std::slice::from_ref(resource), session.revision())
            .unwrap(),
        vec![resource.clone()]
    );
    assert_traffic(session, bytes, 5);
    let refused = InputRequest {
        max_document_bytes: 0,
        ..InputRequest::default()
    };
    session
        .load_resource_saved(resource, session.revision(), &refused)
        .unwrap();
    assert!(matches!(
        session.metadata_resource(resource).unwrap().state,
        MetadataResourceState::Unavailable(InputFailure::DocumentTooLarge { .. })
    ));
    assert_traffic(session, bytes, 6);
}

/// Reuse the shared plain-query import fixture with explicit scope for all four native types.
fn query_host(kind: &str) -> (TestDirectory, PathBuf, SourcePath, AnalysisSession) {
    let directory = TestDirectory::new();
    let native = query_creation_fixture::native(&directory.0, kind);
    let root = directory.0.join("source");
    import_designer_queries(&native.input, &root).unwrap();
    let reader = SourceReader::open(&root).unwrap();
    let source = reader.query_paths().next().unwrap().clone();
    let session = AnalysisSession::from_reader_with_scope(
        reader,
        SessionOptions::default(),
        AnalysisScope::LocalSyntax,
    )
    .unwrap();
    (directory, root, source, session)
}

/// A failed newer query owns its version; close counts the saved text, while release adds zero bytes.
fn assert_query_failure_and_release(
    session: &mut AnalysisSession,
    source: &SourcePath,
    saved_bytes: u64,
    bytes: u64,
) {
    let request = InputRequest::default();
    let refused = InputRequest {
        max_document_bytes: 0,
        ..InputRequest::default()
    };
    session
        .apply_query_overlay(source, session.revision(), 3, "// Refused Я😀", &refused)
        .unwrap();
    assert!(matches!(
        session.query_input(source).unwrap().state,
        QueryTextState::Unavailable(_)
    ));
    assert!(session.query_input(source).unwrap().source_sha256.is_none());
    assert_traffic(session, bytes, 3);
    session
        .close_query_overlay(source, session.revision(), &request)
        .unwrap();
    let bytes = bytes + saved_bytes;
    assert_traffic(session, bytes, 4);
    assert_eq!(
        session
            .invalidate_saved_inputs(std::slice::from_ref(source), session.revision())
            .unwrap(),
        vec![source.clone()]
    );
    assert_traffic(session, bytes, 5);
    assert!(
        session
            .invalidate_saved_inputs(std::slice::from_ref(source), session.revision())
            .unwrap()
            .is_empty()
    );
    assert_traffic(session, bytes, 5);
}

/// Equal-sized and identical newer replacements count their full UTF-8 bytes, while rejects do not.
#[test]
fn metadata_and_module_traffic_counts_full_replacements_without_counting_noops() {
    let (_directory, root) = fixture();
    let metadata = owner("CommonModules/Tools/object.yaml");
    let module = owner("CommonModules/Tools/Ext/Module.bsl");
    let yaml = fs::read_to_string(root.join(metadata.as_path())).unwrap();
    let mut session = AnalysisSession::open(&root).unwrap();
    let request = InputRequest::default();
    assert_traffic(&session, 0, 0);
    session
        .load_saved(&metadata, session.revision(), &request)
        .unwrap();
    session
        .apply_overlay(&metadata, session.revision(), 1, &yaml, &request)
        .unwrap();
    let yaml_bytes = yaml.len() as u64 * 2;
    assert_traffic(&session, yaml_bytes, 2);

    let first = "// Я😀\r\n";
    let second = "// Ю😀\r\n";
    assert_eq!(first.len(), second.len());
    assert!(first.len() > first.chars().count());
    for (version, text) in [(1, first), (2, second), (3, second)] {
        session
            .apply_module_overlay(&module, session.revision(), version, text, &request)
            .unwrap();
    }
    let bytes = yaml_bytes + first.len() as u64 * 3;
    assert_traffic(&session, bytes, 5);
    assert_rejections_preserve_traffic(&mut session, &metadata, &module, &yaml);
    assert_traffic(&session, bytes, 5);
}

/// Accepted failures clear current models without adding Ready bytes, and releases never subtract traffic.
#[test]
fn unavailable_inputs_and_releases_keep_the_cumulative_ledger_monotonic() {
    let (_directory, root) = fixture();
    let metadata = owner("CommonModules/Tools/object.yaml");
    let module = owner("CommonModules/Tools/Ext/Module.bsl");
    let yaml = fs::read_to_string(root.join(metadata.as_path())).unwrap();
    let body = fs::read(root.join(module.as_path())).unwrap();
    let mut session = AnalysisSession::open(&root).unwrap();
    let request = InputRequest::default();
    session
        .load_saved(&metadata, session.revision(), &request)
        .unwrap();
    session
        .load_module_saved(&module, session.revision(), &request)
        .unwrap();
    let bytes = (yaml.len() + body.len()) as u64;
    assert_traffic(&session, bytes, 2);
    let refused = InputRequest::default().with_source_budget(0, 0);
    session
        .apply_overlay(&metadata, session.revision(), 1, &yaml, &refused)
        .unwrap();
    session
        .apply_module_overlay(&module, session.revision(), 1, "// Я😀", &refused)
        .unwrap();
    assert!(matches!(
        session.owner(&metadata).unwrap().state,
        ModelState::Unavailable(InputFailure::SourceBudgetExceeded { limit: 0 })
    ));
    assert!(matches!(
        session.module(&module).unwrap().state,
        ModuleState::Unavailable(ModuleFailure::Input(InputFailure::SourceBudgetExceeded {
            limit: 0
        }))
    ));
    assert!(session.owner(&metadata).unwrap().source_sha256.is_none());
    assert!(session.module(&module).unwrap().source_sha256.is_none());
    assert_eq!(session.resources().total_source_bytes, 0);
    assert_traffic(&session, bytes, 4);

    session
        .close_overlay(&metadata, session.revision(), &request)
        .unwrap();
    session
        .close_module_overlay(&module, session.revision(), &request)
        .unwrap();
    assert_traffic(&session, bytes * 2, 6);
    assert_saved_invalidation(&mut session, metadata, module, bytes * 2);
}

/// Rights reads, malformed Ready XML and resource releases must never recharge unchanged owning YAML.
#[test]
fn resource_publications_charge_only_the_new_resource_bytes() {
    let (_directory, root, mut session) = role_host();
    let metadata = owner("Roles/Reader/object.yaml");
    let resource = owner("Roles/Reader/Ext/Rights.xml");
    let yaml = fs::read(root.join(metadata.as_path())).unwrap();
    let rights = fs::read(root.join(resource.as_path())).unwrap();
    let request = InputRequest::default();
    session
        .load_saved(&metadata, session.revision(), &request)
        .unwrap();
    session
        .load_resource_saved(&resource, session.revision(), &request)
        .unwrap();
    let bytes = (yaml.len() + rights.len()) as u64;
    assert_traffic(&session, bytes, 2);
    let draft = "<Invalid>Я😀\r\n";
    session
        .apply_resource_overlay(&resource, session.revision(), 1, draft, &request)
        .unwrap();
    assert!(matches!(
        session.metadata_resource(&resource).unwrap().state,
        MetadataResourceState::Ready(_)
    ));
    let bytes = bytes + draft.len() as u64;
    assert_traffic(&session, bytes, 3);
    session
        .close_resource_overlay(&resource, session.revision(), &request)
        .unwrap();
    let bytes = bytes + rights.len() as u64;
    assert_traffic(&session, bytes, 4);
    assert_resource_release_and_failure(&mut session, &resource, bytes);
    assert_eq!(fs::read(root.join(metadata.as_path())).unwrap(), yaml);
    assert_eq!(fs::read(root.join(resource.as_path())).unwrap(), rights);
}

/// Declared query text uses the same ledger in every native profile without registering a BSL file.
#[test]
fn query_replacements_failures_and_releases_share_the_host_traffic_ledger() {
    for kind in query_creation_fixture::TYPES {
        let (_directory, root, source, mut session) = query_host(kind);
        let saved = fs::read(root.join(source.as_path())).unwrap();
        let request = InputRequest::default();
        let draft = "\u{feff}// Я😀\r\nSELECT 1\r\n";
        for version in 1..=2 {
            session
                .apply_query_overlay(&source, session.revision(), version, draft, &request)
                .unwrap();
        }
        let bytes = draft.len() as u64 * 2;
        assert_traffic(&session, bytes, 2);
        assert!(
            session
                .apply_query_overlay(&source, session.revision(), 2, draft, &request)
                .is_err()
        );
        assert_traffic(&session, bytes, 2);
        assert_query_failure_and_release(&mut session, &source, saved.len() as u64, bytes);
    }
}

/// Saturation cannot wrap the traffic meter or reject an otherwise valid input publication.
#[test]
fn cumulative_ready_bytes_saturate_without_changing_source_admission() {
    let (_directory, root) = fixture();
    let mut session = AnalysisSession::open(&root).unwrap();
    session.cumulative_ready_source_bytes = u64::MAX - 1;
    let module = owner("CommonModules/Tools/Ext/Module.bsl");
    let text = "// Я😀\r\n";
    for version in 1..=2 {
        session
            .apply_module_overlay(
                &module,
                session.revision(),
                version,
                text,
                &InputRequest::default(),
            )
            .unwrap();
        assert_traffic(&session, u64::MAX, u64::try_from(version).unwrap());
        assert_eq!(session.resources().module_text_bytes, text.len() as u64);
    }
}
