//! Current consumer views share source authority and refuse stale or failed input without disk fallback.

use std::fs;

use eska::project_core::source_format::{QueryProfile, SourceReader};
use ide_db::metadata::CanonicalMetadata;

use super::*;
use crate::{
    AnalysisScope, SessionOptions, test_query_analysis_project as fixture,
    test_support::TestDirectory,
};

/// Pure storage views work in syntax scope without inventing an external object's owning base.
fn syntax_host(project: &fixture::Project) -> AnalysisSession {
    AnalysisSession::from_reader_with_scope(
        SourceReader::open(&project.root.join("src")).unwrap(),
        SessionOptions::default(),
        AnalysisScope::LocalSyntax,
    )
    .unwrap()
}

/// The form query is selected by its manifest resource, never by the physical artifact stem.
fn form_query(session: &AnalysisSession, project: &fixture::Project) -> SourcePath {
    session
        .query_paths()
        .find(|path| *path != &project.source)
        .unwrap()
        .clone()
}

/// Source accounting excludes derived form XML while retaining each raw authoring input exactly once.
fn assert_reservations(session: &AnalysisSession) {
    let owners: u64 = session
        .selected_root
        .inputs
        .owner_paths()
        .map(
            |path| match &session.selected_root.inputs.owner(path).unwrap().state {
                InputState::Ready(snapshot) => snapshot.source_bytes,
                _ => 0,
            },
        )
        .sum();
    let resources: u64 = session
        .resource_paths()
        .map(|path| session.metadata_resource(path).unwrap().bytes())
        .sum();
    let queries: u64 = session
        .query_paths()
        .map(|path| session.query_input(path).unwrap().bytes())
        .sum();
    assert_eq!(
        session.resources().metadata_source_bytes,
        owners + resources
    );
    assert_eq!(
        session.resources().total_source_bytes,
        owners + resources + queries
    );
    assert_eq!(session.resources().module_text_bytes, 0);
}

/// All four artifact profiles consume actual form/DCS storage without fake Configuration or full coverage.
#[test]
fn all_four_current_storage_views_bind_hashes_origins_and_one_source_reservation() {
    for kind in fixture::TYPES {
        let directory = TestDirectory::new();
        let project = fixture::project(&directory.0, kind);
        let before = fixture::snapshot(&project);
        let mut session = syntax_host(&project);
        let request = InputRequest::default();
        assert_eq!(session.resource_paths().count(), 1);
        let paths = session.query_paths().cloned().collect::<Vec<_>>();
        for path in paths {
            session
                .prepare_query_context(&path, session.revision(), &request)
                .unwrap();
            let value = session
                .query_context(&path, session.revision(), &request)
                .unwrap();
            assert_eq!(value.root, session.selected_root_id());
            assert_eq!(value.revision, session.revision());
            assert_eq!(value.query.source, path);
            assert_eq!(value.query.origin, InputOrigin::Saved);
            assert_eq!(
                value.query.source_sha256,
                session
                    .query_input(&path)
                    .unwrap()
                    .source_sha256
                    .as_ref()
                    .unwrap()
                    .as_str()
            );
            assert_eq!(value.consumer.source(), &value.resource.source);
            assert!(!value.consumer.consumer_complete());
            match value.consumer.profile() {
                QueryProfile::ManagedForm => {
                    assert_eq!(value.consumer.dynamic_lists()[0].name.value, "Rows");
                    assert_eq!(value.owner, value.resource);
                    assert!(value.resource_revision.is_none());
                    assert!(session.metadata_resource(&value.owner.source).is_none());
                }
                QueryProfile::Composition => {
                    assert_eq!(value.consumer.datasets()[0].path[0].value, "Data");
                    assert!(value.resource_revision.is_some());
                    assert_ne!(value.owner.source, value.resource.source);
                }
            }
            assert_reservations(&session);
        }
        assert_eq!(fixture::snapshot(&project), before);
    }
}

/// Canonical form members and pure consumer names follow the same current YAML body publication.
#[test]
fn unified_form_current_body_and_bad_reference_never_use_independent_saved_dialog() {
    let directory = TestDirectory::new();
    let project = fixture::project(&directory.0, "configuration");
    let mut session = AnalysisSession::open(&project.root.join("src")).unwrap();
    let request = InputRequest::default();
    let path = form_query(&session, &project);
    let owner = session.query_declaration(&path).unwrap().owner.clone();
    let file = project.root.join("src").join(owner.as_path());
    let saved = fs::read_to_string(&file).unwrap();
    let draft = saved.replace("name=\\\"Rows\\\"", "name=\\\"CurrentRows\\\"");
    assert_ne!(saved, draft);
    fs::remove_file(project.root.join("src").join(path.as_path())).unwrap();
    session
        .apply_query_overlay(&path, session.revision(), 1, "SELECT 1 AS Value", &request)
        .unwrap();
    session
        .prepare_query_context(&path, session.revision(), &request)
        .unwrap();
    session
        .apply_overlay(&owner, session.revision(), 3, &draft, &request)
        .unwrap();
    let view = session
        .query_context(&path, session.revision(), &request)
        .unwrap();
    assert_eq!(view.consumer.dynamic_lists()[0].name.value, "CurrentRows");
    assert_eq!(
        view.owner.origin,
        InputOrigin::Overlay {
            document_version: 3
        }
    );
    assert_eq!(view.owner, view.resource);
    let Some(CanonicalMetadata::Form(form)) = session.resolve(&owner, session.revision()).unwrap()
    else {
        panic!("current canonical form");
    };
    assert_eq!(form.attributes[0].name, "CurrentRows");
    let invalid = draft.replace(path.as_str(), project.source.as_str());
    session
        .apply_overlay(&owner, session.revision(), 4, &invalid, &request)
        .unwrap();
    assert!(matches!(
        session.query_context(&path, session.revision(), &request),
        Err(QueryContextRequestError::Source(_))
    ));
    assert!(
        session
            .resolve(&owner, session.revision())
            .unwrap()
            .is_some()
    );
    session
        .close_overlay(&owner, session.revision(), &request)
        .unwrap();
    assert_eq!(
        session
            .query_context(&path, session.revision(), &request)
            .unwrap()
            .consumer
            .dynamic_lists()[0]
            .name
            .value,
        "Rows"
    );
    assert_eq!(fs::read_to_string(file).unwrap(), saved);
    assert_reservations(&session);
}

/// Canonical metadata preparation must not read a saved neighboring query behind a valid editor input.
#[test]
fn metadata_preparation_does_not_materialize_plain_query_values_or_consumer_xml() {
    let directory = TestDirectory::new();
    let project = fixture::project(&directory.0, "configuration");
    let mut session = AnalysisSession::open(&project.root.join("src")).unwrap();
    let request = InputRequest::default();
    let form = form_query(&session, &project);
    let dcs = resource_source(
        &session,
        &session
            .query_declaration(&project.source)
            .unwrap()
            .native_resource,
    )
    .unwrap();
    fs::remove_file(project.root.join("src").join(form.as_path())).unwrap();
    fs::remove_file(project.root.join("src").join(dcs.as_path())).unwrap();
    session
        .apply_query_overlay(&form, session.revision(), 1, "SELECT 1 AS Value", &request)
        .unwrap();
    session
        .load_query_saved(&project.source, session.revision(), &request)
        .unwrap();
    session
        .prepare_query_analysis(session.revision(), &request)
        .unwrap();
    assert!(
        session
            .query_analysis_diagnostics(
                &project.source,
                session.revision(),
                &crate::DiagnosticsConfig::default(),
                &request,
                &crate::QueryCancellation::default()
            )
            .is_ok()
    );
    assert!(matches!(
        session.metadata_resource(&dcs).unwrap().state,
        MetadataResourceState::Pending
    ));
    assert_eq!(
        session
            .query_context(&form, session.revision(), &request)
            .unwrap()
            .query
            .origin,
        InputOrigin::Overlay {
            document_version: 1
        }
    );
    assert_reservations(&session);
}

/// Current DCS fields and failed raw XML remain independent from the readable current owner descriptor.
#[test]
fn dcs_editor_resource_current_owner_and_invalid_xml_never_fall_back() {
    let directory = TestDirectory::new();
    let project = fixture::project(&directory.0, "configuration");
    let mut session = syntax_host(&project);
    let request = InputRequest::default();
    session
        .prepare_query_context(&project.source, session.revision(), &request)
        .unwrap();
    let old = session
        .query_context(&project.source, session.revision(), &request)
        .unwrap();
    let resource = old.resource.source;
    let saved = fs::read_to_string(project.root.join("src").join(resource.as_path())).unwrap();
    let draft = saved.replace("</name>", "</name><field xsi:type=\"DataSetFieldField\"><dataPath>CurrentValue</dataPath><field>Value</field></field>");
    session
        .apply_resource_overlay(&resource, session.revision(), 1, &draft, &request)
        .unwrap();
    let current = session
        .query_context(&project.source, session.revision(), &request)
        .unwrap();
    assert_eq!(
        current.consumer.datasets()[0].fields[0].data_path.value,
        "CurrentValue"
    );
    assert_eq!(
        current.resource.origin,
        InputOrigin::Overlay {
            document_version: 1
        }
    );
    assert_eq!(current.owner_revision, old.owner_revision);
    assert_ne!(current.resource_revision, old.resource_revision);
    session
        .apply_resource_overlay(&resource, session.revision(), 2, "broken XML", &request)
        .unwrap();
    assert!(matches!(
        session.metadata_resource(&resource).unwrap().state,
        MetadataResourceState::Ready(_)
    ));
    assert!(matches!(
        session.query_context(&project.source, session.revision(), &request),
        Err(QueryContextRequestError::Source(_))
    ));
    session
        .close_resource_overlay(&resource, session.revision(), &request)
        .unwrap();
    assert!(
        session
            .query_context(&project.source, session.revision(), &request)
            .unwrap()
            .consumer
            .datasets()[0]
            .fields
            .is_empty()
    );
    let owner_file = project
        .root
        .join("src")
        .join(current.owner.source.as_path());
    let owner = fs::read_to_string(owner_file).unwrap();
    let changed = owner.replace("DataCompositionSchema", "HTMLDocument");
    session
        .apply_overlay(
            &current.owner.source,
            session.revision(),
            1,
            &changed,
            &request,
        )
        .unwrap();
    assert!(matches!(
        session.query_context(&project.source, session.revision(), &request),
        Err(QueryContextRequestError::Source(_))
    ));
    assert_reservations(&session);
}

/// Cancelled, stale and quota-failed current inputs cannot produce a clean storage read view.
#[test]
fn query_context_preflight_and_shared_quota_preserve_current_failure_authority() {
    let directory = TestDirectory::new();
    let project = fixture::project(&directory.0, "configuration");
    let mut session = syntax_host(&project);
    let cancelled = InputRequest::default();
    cancelled.cancellation.cancel();
    let initial = session.revision();
    assert_eq!(
        session.prepare_query_context(&project.source, initial, &cancelled),
        Err(QueryContextRequestError::Cancelled)
    );
    assert_eq!(session.revision(), initial);
    let request = InputRequest::default();
    session
        .load_query_saved(&project.source, initial, &request)
        .unwrap();
    let before = session.resources();
    assert!(matches!(
        session.prepare_query_context(&project.source, initial, &request),
        Err(QueryContextRequestError::Request(
            SessionRequestError::StaleRevision { .. }
        ))
    ));
    assert_eq!(session.resources(), before);
    session.options.max_retained_source_bytes = 0;
    session
        .apply_query_overlay(&project.source, session.revision(), 1, "SELECT 1", &request)
        .unwrap();
    assert!(matches!(
        session.query_context(&project.source, session.revision(), &request),
        Err(QueryContextRequestError::Unavailable {
            failure: InputFailure::SourceBudgetExceeded { .. },
            ..
        })
    ));
    assert_eq!(session.resources().query_text_bytes, 0);
    assert_eq!(
        session.query_input(&project.source).unwrap().origin,
        InputOrigin::Overlay {
            document_version: 1
        }
    );
}

/// A saved event clears current DCS authority; editor failures survive until explicit close/reload.
#[test]
fn dcs_watch_close_missing_and_repair_bind_the_current_resource_without_owner_reread() {
    let directory = TestDirectory::new();
    let project = fixture::project(&directory.0, "configuration");
    let mut session = syntax_host(&project);
    let request = InputRequest::default();
    session
        .prepare_query_context(&project.source, session.revision(), &request)
        .unwrap();
    let original = session
        .query_context(&project.source, session.revision(), &request)
        .unwrap();
    let resource = original.resource.source;
    let file = project.root.join("src").join(resource.as_path());
    let saved = fs::read_to_string(&file).unwrap();
    fs::write(
        &file,
        saved.replace("<!-- keep -->", "<!-- current disk -->"),
    )
    .unwrap();
    session
        .invalidate_saved_inputs(std::slice::from_ref(&resource), session.revision())
        .unwrap();
    assert!(matches!(
        session.query_context(&project.source, session.revision(), &request),
        Err(QueryContextRequestError::Pending(_))
    ));
    session
        .prepare_query_context(&project.source, session.revision(), &request)
        .unwrap();
    let current = session
        .query_context(&project.source, session.revision(), &request)
        .unwrap();
    assert_ne!(
        current.resource.source_sha256,
        original.resource.source_sha256
    );
    assert_eq!(current.owner_revision, original.owner_revision);
    session
        .apply_resource_overlay(&resource, session.revision(), 1, "invalid XML", &request)
        .unwrap();
    fs::remove_file(&file).unwrap();
    assert!(
        session
            .invalidate_saved_inputs(std::slice::from_ref(&resource), session.revision())
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        session.query_context(&project.source, session.revision(), &request),
        Err(QueryContextRequestError::Source(_))
    ));
    session
        .close_resource_overlay(&resource, session.revision(), &request)
        .unwrap();
    assert!(matches!(
        session.query_context(&project.source, session.revision(), &request),
        Err(QueryContextRequestError::Unavailable {
            failure: InputFailure::Source(_),
            ..
        })
    ));
    fs::write(file, saved).unwrap();
    session
        .invalidate_saved_inputs(std::slice::from_ref(&resource), session.revision())
        .unwrap();
    session
        .prepare_query_context(&project.source, session.revision(), &request)
        .unwrap();
    assert_eq!(
        session
            .query_context(&project.source, session.revision(), &request)
            .unwrap()
            .resource
            .source_sha256,
        original.resource.source_sha256
    );
    assert_reservations(&session);
}
