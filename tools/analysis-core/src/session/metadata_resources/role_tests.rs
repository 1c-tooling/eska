//! Role identity and current rights share one host clock without XML fallbacks or hidden reads.

use std::{fs, path::PathBuf, sync::Arc};

use eska::analysis_core::{Cancellation, InputState};
use ide_db::metadata::CanonicalMetadata;

use super::*;
use crate::{
    ModelState, SessionOptions,
    session::tests::{owner, support::TestDirectory},
    test_role_project,
};

/// Build a fresh imported source with two declared resources and one genuinely absent resource.
fn fixture() -> (TestDirectory, PathBuf, AnalysisSession) {
    let directory = TestDirectory::new();
    let root = crate::test_project::project(&directory.0, "// Current body.\n", false);
    test_role_project::add_roles(&root, &directory.0.join("native"));
    let source = root.join("src-with-roles");
    let session = AnalysisSession::open(&source).unwrap();
    (directory, source, session)
}

/// Assert the actual Salsa resolver's value, independently of the retained tree projection.
fn role(session: &AnalysisSession, path: &SourcePath) -> Arc<bsl_metadata::Role> {
    let Some(CanonicalMetadata::Role(value)) = session.resolve(path, session.revision()).unwrap()
    else {
        panic!("expected current role: {:?}", session.owner(path));
    };
    value
}

/// Opening only composes declared identities; a pending declared resource cannot become defaults.
#[test]
fn declared_pending_rights_and_absent_rights_have_distinct_models() {
    let (_directory, source, mut session) = fixture();
    let path = owner("Roles/Reader/object.yaml");
    let resource = owner("Roles/Reader/Ext/Rights.xml");
    let request = InputRequest::default();
    assert_eq!(session.resource_paths().count(), 2);
    assert_eq!(session.resources().total_source_bytes, 0);
    assert!(matches!(
        session.metadata_resource(&resource).unwrap().state,
        MetadataResourceState::Pending
    ));
    session
        .load_saved(&path, session.revision(), &request)
        .unwrap();
    assert!(matches!(
        session.metadata(&path, session.revision()).unwrap().input,
        InputState::Ready(_)
    ));
    assert_eq!(session.owner(&path).unwrap().state, ModelState::Pending);
    assert!(
        session
            .resolve(&path, session.revision())
            .unwrap()
            .is_none()
    );
    let saved_yaml = session.owner(&path).unwrap().source_sha256.clone();
    session
        .load_resource_saved(&resource, session.revision(), &request)
        .unwrap();
    let saved_text = fs::read_to_string(source.join(resource.as_path())).unwrap();
    let current = session.metadata_resource(&resource).unwrap();
    assert_eq!(current.bytes(), saved_text.len() as u64);
    assert_eq!(
        current.state,
        MetadataResourceState::Ready(Arc::from(saved_text.clone()))
    );
    assert_eq!(current.revision, session.revision());
    assert_eq!(session.owner(&path).unwrap().source_sha256, saved_yaml);
    let model = role(&session, &path);
    assert_eq!(
        model.uuid().to_string(),
        "70000000-0000-4000-8000-000000000001"
    );
    assert_eq!(model.objects()[0].name, "Items");
    assert_eq!(
        *model.data(),
        bsl_metadata::xml_parser::parse_rights_xml(&saved_text).unwrap()
    );
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    assert!(session.summary().main_models_complete);
    assert!(session.summary().descriptor_models_complete);
    assert_eq!(
        *role(&session, &owner("Roles/Absent/object.yaml")).data(),
        bsl_metadata::RoleData::default()
    );
}

/// Resource edits replace their own canonical data while retained snapshots and a neighbor survive.
#[test]
fn rights_overlay_updates_the_role_without_changing_yaml_or_neighbors() {
    let (_directory, source, mut session) = fixture();
    let path = owner("Roles/Reader/object.yaml");
    let resource = owner("Roles/Reader/Ext/Rights.xml");
    let neighbor = owner("Roles/Neighbor/object.yaml");
    let request = InputRequest::default();
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    let saved = role(&session, &path);
    let untouched = role(&session, &neighbor);
    let yaml = session.metadata(&path, session.revision()).unwrap();
    let saved_bytes = fs::read(source.join(resource.as_path())).unwrap();
    let changed = test_role_project::rights("Document.Current", true);
    session
        .apply_resource_overlay(&resource, session.revision(), 7, &changed, &request)
        .unwrap();
    let current = role(&session, &path);
    assert_eq!(current.uuid(), saved.uuid());
    assert_eq!(current.objects()[0].name, "Current");
    assert_eq!(current.objects()[0].restrictions, ["WHERE Current = TRUE"]);
    assert!(current.data().set_for_new_objects());
    assert!(current.data().independent_rights_of_child_objects());
    assert_eq!(saved.objects()[0].name, "Items");
    assert!(Arc::ptr_eq(&untouched, &role(&session, &neighbor)));
    let model = session.owner(&path).unwrap();
    assert_eq!(model.revision, session.revision());
    assert_eq!(model.origin, InputOrigin::Saved);
    assert_eq!(model.source_sha256, yaml.source_sha256);
    assert_eq!(
        session.metadata_resource(&resource).unwrap().origin,
        InputOrigin::Overlay {
            document_version: 7
        }
    );
    assert_ne!(
        session
            .metadata_resource(&resource)
            .unwrap()
            .source_sha256
            .as_deref(),
        None
    );
    assert_eq!(
        fs::read(source.join(resource.as_path())).unwrap(),
        saved_bytes
    );
    let current_revision = session.revision();
    let resources = session.resources();
    for result in [
        session.apply_resource_overlay(&resource, current_revision, 7, "invalid", &request),
        session.load_resource_saved(&resource, current_revision, &request),
        session.apply_resource_overlay(
            &resource,
            SessionRevision(current_revision.0 - 1),
            8,
            "invalid",
            &request,
        ),
        session.apply_resource_overlay(
            &owner("Roles/Absent/Ext/Rights.xml"),
            current_revision,
            8,
            "invalid",
            &request,
        ),
    ] {
        assert!(result.is_err());
    }
    assert_eq!(session.revision(), current_revision);
    assert_eq!(session.resources(), resources);
    assert!(Arc::ptr_eq(&current, &role(&session, &path)));
    session
        .close_resource_overlay(&resource, session.revision(), &request)
        .unwrap();
    assert_eq!(*role(&session, &path), *saved);
    assert_eq!(
        session.metadata_resource(&resource).unwrap().origin,
        InputOrigin::Saved
    );
}

/// Invalid current XML keeps its bytes but clears the role and never borrows YAML coordinates.
#[test]
fn invalid_rights_clear_only_the_current_role_and_close_reads_current_disk() {
    let (_directory, source, mut session) = fixture();
    let path = owner("Roles/Reader/object.yaml");
    let resource = owner("Roles/Reader/Ext/Rights.xml");
    let request = InputRequest::default();
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    fs::write(source.join("Roles/Reader.xml"), "stray XML fallback").unwrap();
    session
        .apply_resource_overlay(&resource, session.revision(), 1, "<NotRights/>", &request)
        .unwrap();
    assert!(matches!(
        session.metadata_resource(&resource).unwrap().state,
        MetadataResourceState::Ready(_)
    ));
    let failure = InputFailure::Source(eska::project_core::source_format::Error {
        code: "analysis_role_rights_invalid",
        location: vec![resource.as_str().to_owned()],
    });
    assert_eq!(
        session.owner(&path).unwrap().state,
        ModelState::Unavailable(failure)
    );
    assert!(
        session
            .resolve(&path, session.revision())
            .unwrap()
            .is_none()
    );
    assert!(!session.summary().main_models_complete);
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    assert!(
        session
            .resolve(&path, session.revision())
            .unwrap()
            .is_none()
    );
    let changed = test_role_project::rights("Catalog.DiskCurrent", true);
    fs::write(source.join(resource.as_path()), changed).unwrap();
    session
        .close_resource_overlay(&resource, session.revision(), &request)
        .unwrap();
    assert_eq!(role(&session, &path).objects()[0].name, "DiskCurrent");
    session
        .apply_overlay(&path, session.revision(), 1, "document: [", &request)
        .unwrap();
    session
        .apply_resource_overlay(&resource, session.revision(), 2, "<Rights/>", &request)
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
    assert!(role(&session, &path).objects().is_empty());
}

/// A batch preflights resources and editor authority; saved failures free their exact reservation.
#[test]
fn saved_resource_invalidation_is_atomic_and_failures_clear_canonical_data() {
    let (_directory, source, mut session) = fixture();
    let path = owner("Roles/Reader/object.yaml");
    let resource = owner("Roles/Reader/Ext/Rights.xml");
    let neighbor = owner("Roles/Neighbor/Ext/Rights.xml");
    let request = InputRequest::default();
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    let before = session.revision();
    let resources = session.resources();
    let saved = role(&session, &path);
    assert!(
        session
            .invalidate_saved_inputs(&[resource.clone(), owner("unknown.xml")], before)
            .is_err()
    );
    assert_eq!(session.revision(), before);
    assert_eq!(session.resources(), resources);
    assert!(Arc::ptr_eq(&saved, &role(&session, &path)));
    session
        .apply_resource_overlay(&neighbor, session.revision(), 1, "<Rights/>", &request)
        .unwrap();
    let old_bytes = session.metadata_resource(&resource).unwrap().bytes();
    let before_bytes = session.resources().total_source_bytes;
    fs::remove_file(source.join(resource.as_path())).unwrap();
    assert_eq!(
        session
            .invalidate_saved_inputs(
                &[resource.clone(), neighbor.clone(), resource.clone()],
                session.revision()
            )
            .unwrap()
            .as_slice(),
        std::slice::from_ref(&resource)
    );
    assert_eq!(
        session.resources().total_source_bytes,
        before_bytes - old_bytes
    );
    assert_eq!(session.owner(&path).unwrap().state, ModelState::Pending);
    assert!(
        session
            .resolve(&path, session.revision())
            .unwrap()
            .is_none()
    );
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    assert!(matches!(
        session.metadata_resource(&resource).unwrap().state,
        MetadataResourceState::Unavailable(InputFailure::Source(_))
    ));
    assert!(
        session
            .resolve(&path, session.revision())
            .unwrap()
            .is_none()
    );
    fs::write(source.join(resource.as_path()), [0xff]).unwrap();
    session
        .load_resource_saved(&resource, session.revision(), &request)
        .unwrap();
    assert!(
        matches!(&session.metadata_resource(&resource).unwrap().state, MetadataResourceState::Unavailable(InputFailure::Source(error)) if error.code == "source_encoding_unsupported")
    );
    let cancellation = Cancellation::default();
    cancellation.cancel();
    let cancelled = InputRequest {
        cancellation,
        ..InputRequest::default()
    };
    session
        .apply_resource_overlay(&resource, session.revision(), 2, "<Rights/>", &cancelled)
        .unwrap();
    assert_eq!(
        session.metadata_resource(&resource).unwrap().state,
        MetadataResourceState::Unavailable(InputFailure::Cancelled)
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
        .close_resource_overlay(&resource, session.revision(), &request)
        .unwrap();
    fs::write(source.join(resource.as_path()), "<Rights/>").unwrap();
    session
        .invalidate_saved_inputs(std::slice::from_ref(&resource), session.revision())
        .unwrap();
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    assert!(role(&session, &path).objects().is_empty());
}

/// XML resources, owner YAML and BSL consume the same quota; a replacement reuses its reservation.
#[test]
fn rights_use_the_shared_source_budget_and_clock_preflight() {
    let (_directory, source, session) = fixture();
    let resource = owner("Roles/Reader/Ext/Rights.xml");
    let path = owner("Roles/Reader/object.yaml");
    let module = owner("CommonModules/Tools/Ext/Module.bsl");
    let rights_bytes = fs::metadata(source.join(resource.as_path())).unwrap().len();
    let yaml_bytes = fs::metadata(source.join(path.as_path())).unwrap().len();
    drop(session);
    let limit = rights_bytes + yaml_bytes;
    let mut session = AnalysisSession::open_with_options(
        &source,
        SessionOptions {
            max_retained_source_bytes: limit,
            ..SessionOptions::default()
        },
    )
    .unwrap();
    let request = InputRequest::default();
    session
        .load_resource_saved(&resource, session.revision(), &request)
        .unwrap();
    session
        .load_saved(&path, session.revision(), &request)
        .unwrap();
    assert_eq!(session.resources().metadata_source_bytes, limit);
    session
        .load_module_saved(&module, session.revision(), &request)
        .unwrap();
    assert!(matches!(
        session.module(&module).unwrap().state,
        crate::ModuleState::Unavailable(_)
    ));
    let text = fs::read_to_string(source.join(resource.as_path())).unwrap();
    session
        .apply_resource_overlay(&resource, session.revision(), 1, &text, &request)
        .unwrap();
    assert_eq!(session.resources().total_source_bytes, limit);
    session
        .apply_resource_overlay(&resource, session.revision(), 2, &(text + " "), &request)
        .unwrap();
    assert!(
        matches!(session.metadata_resource(&resource).unwrap().state, MetadataResourceState::Unavailable(InputFailure::SourceBudgetExceeded { limit: actual }) if actual == limit)
    );
    assert_eq!(session.resources().total_source_bytes, yaml_bytes);
    assert!(
        session
            .resolve(&path, session.revision())
            .unwrap()
            .is_none()
    );
    session.revision = SessionRevision(u64::MAX);
    let before = session.metadata_resource(&resource).unwrap().clone();
    assert_eq!(
        session.close_resource_overlay(&resource, session.revision(), &request),
        Err(SessionRequestError::RevisionExhausted)
    );
    assert_eq!(session.metadata_resource(&resource), Some(&before));
}
