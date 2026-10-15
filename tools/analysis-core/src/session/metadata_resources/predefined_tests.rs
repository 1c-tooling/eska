//! Current predefined data drives the native object and completion consumers without XML fallback.

use std::{fs, path::PathBuf, sync::Arc};

use eska::analysis_core::{Cancellation, InputState};
use hir::DefDatabase;
use ide_db::metadata::CanonicalMetadata;

use super::*;
use crate::{
    Locale, ModelState, QueryCancellation,
    session::tests::{owner, support::TestDirectory},
    test_predefined_project,
};

const OWNER: &str = "Catalogs/Items/object.yaml";
const RESOURCE: &str = "Catalogs/Items/Ext/Predefined.xml";
const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";
const BODY: &str = "Procedure Run()\n Value = Catalogs.Items.Saved;\nEndProcedure\n";

/// All saved inputs belong to this fixture; same-owner forms detect excessive reprojection.
fn fixture() -> (TestDirectory, PathBuf, AnalysisSession) {
    let directory = TestDirectory::new();
    let root = crate::test_project::project(&directory.0, BODY, false);
    test_predefined_project::add_predefined(&root, &directory.0.join("native"));
    let source = root.join("src-with-predefined");
    let session = AnalysisSession::open(&source).unwrap();
    (directory, source, session)
}

/// Inspect the actual tracked native resolver rather than just the retained tree state.
fn object(session: &AnalysisSession, path: &SourcePath) -> Arc<bsl_metadata::MetadataObject> {
    let Some(CanonicalMetadata::Object(value)) = session.resolve(path, session.revision()).unwrap()
    else {
        panic!("expected current object: {:?}", session.owner(path));
    };
    value
}

/// Extract candidate labels at the same unchanged BSL manager access in either presentation locale.
fn names(session: &AnalysisSession, locale: Locale) -> Vec<String> {
    let offset = u32::try_from(BODY.find("Saved").unwrap()).unwrap();
    session
        .module_completions(
            &owner(MODULE),
            session.revision(),
            offset,
            locale,
            &QueryCancellation::default(),
        )
        .unwrap()
        .value
        .into_iter()
        .map(|item| item.label)
        .collect()
}

/// Pending declared data cannot become defaults; genuinely absent data preserves an empty native list.
#[test]
fn declared_predefined_resources_use_current_native_items_for_all_four_families() {
    let (_directory, source, mut session) = fixture();
    let request = InputRequest::default();
    let path = owner(OWNER);
    let resource = owner(RESOURCE);
    assert_eq!(session.resource_paths().count(), 6); // Five predefined resources and one form dialog.
    assert!(
        session
            .metadata_resource(&owner("Catalogs/Stray/Ext/Predefined.xml"))
            .is_none()
    );
    session
        .load_saved(&path, session.revision(), &request)
        .unwrap();
    assert_eq!(session.owner(&path).unwrap().state, ModelState::Pending);
    assert!(matches!(
        session.metadata(&path, session.revision()).unwrap().input,
        InputState::Ready(_)
    ));
    assert!(
        session
            .resolve(&path, session.revision())
            .unwrap()
            .is_none()
    );
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    assert!(session.summary().main_models_complete);
    assert!(session.summary().descriptor_models_complete);
    assert_eq!(
        session.metadata_resource(&resource).unwrap().kind,
        MetadataResourceKind::PredefinedData
    );
    for (folder, name) in [
        ("Catalogs", "Items"),
        ("Catalogs", "Neighbor"),
        ("ChartsOfAccounts", "Accounts"),
        ("ChartsOfCalculationTypes", "Calculations"),
        ("ChartsOfCharacteristicTypes", "Characteristics"),
    ] {
        let text =
            fs::read_to_string(source.join(format!("{folder}/{name}/Ext/Predefined.xml"))).unwrap();
        let value = object(&session, &owner(&format!("{folder}/{name}/object.yaml")));
        assert_eq!(
            value.predefined_items,
            bsl_metadata::xml_parser::parse_predefined_xml(&text)
        );
        assert_eq!(value.predefined_items.len(), 2);
    }
    assert!(
        object(&session, &owner("Catalogs/Absent/object.yaml"))
            .predefined_items
            .is_empty()
    );
    assert_eq!(
        session.metadata_resource(&resource).unwrap().bytes(),
        fs::metadata(source.join(RESOURCE)).unwrap().len()
    );
}

/// Resource edits invalidate real completion queries while preserving YAML, neighbors and form memos.
#[test]
fn predefined_overlay_updates_completions_without_reprojecting_neighbor_or_nested_form() {
    let (_directory, source, mut session) = fixture();
    let request = InputRequest::default();
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    let path = owner(OWNER);
    let resource = owner(RESOURCE);
    let saved = object(&session, &path);
    let neighbor_path = owner("Catalogs/Neighbor/object.yaml");
    let neighbor = object(&session, &neighbor_path);
    let view = session.metadata(&path, session.revision()).unwrap();
    let form = view.model.forms[0].clone();
    let form_file = session.selected_root.modules
        [&owner("Catalogs/Items/Forms/Shared/Ext/Form/Module.bsl")]
        .file;
    let form_context = session
        .database
        .module_metadata(hir::ModuleId { file_id: form_file });
    let body_input = session.module(&owner(MODULE)).unwrap().clone();
    let saved_bytes = fs::read(source.join(RESOURCE)).unwrap();
    for locale in [Locale::Ru, Locale::En] {
        assert!(names(&session, locale).contains(&"Saved".to_owned()));
    }
    let current = test_predefined_project::data("Current");
    session
        .apply_resource_overlay(&resource, session.revision(), 7, &current, &request)
        .unwrap();
    let model = object(&session, &path);
    assert_eq!(model.uuid, saved.uuid);
    assert_eq!(model.predefined_items[0].name, "Current");
    assert_eq!(saved.predefined_items[0].name, "Saved");
    assert!(Arc::ptr_eq(&neighbor, &object(&session, &neighbor_path)));
    let view_current = session.metadata(&path, session.revision()).unwrap();
    assert_eq!(view_current.input, view.input);
    assert_eq!(view_current.origin, InputOrigin::Saved);
    assert_eq!(view_current.source_sha256, view.source_sha256);
    assert_eq!(view_current.model.forms[0], form);
    assert!(Arc::ptr_eq(
        &form_context,
        &session
            .database
            .module_metadata(hir::ModuleId { file_id: form_file })
    ));
    assert_eq!(*session.module(&owner(MODULE)).unwrap(), body_input);
    for locale in [Locale::Ru, Locale::En] {
        let labels = names(&session, locale);
        assert!(labels.contains(&"Current".to_owned()));
        assert!(!labels.contains(&"Saved".to_owned()));
    }
    let revision = session.revision();
    let resources = session.resources();
    assert!(matches!(
        session.apply_resource_overlay(&resource, revision, 7, "invalid", &request),
        Err(SessionRequestError::StaleDocumentVersion { .. })
    ));
    assert!(matches!(
        session.load_resource_saved(&resource, revision, &request),
        Err(SessionRequestError::OverlayActive { .. })
    ));
    assert!(matches!(
        session.apply_resource_overlay(
            &resource,
            SessionRevision(revision.0 - 1),
            8,
            "invalid",
            &request
        ),
        Err(SessionRequestError::StaleRevision { .. })
    ));
    assert_eq!(session.revision(), revision);
    assert_eq!(session.resources(), resources);
    assert_eq!(fs::read(source.join(RESOURCE)).unwrap(), saved_bytes);
    session
        .close_resource_overlay(&resource, revision, &request)
        .unwrap();
    assert_eq!(
        object(&session, &path).predefined_items,
        saved.predefined_items
    );
}

/// Invalid XML remains current text, clears only its object, and never falls back on old disk items.
#[test]
fn invalid_predefined_data_clears_the_object_while_close_reloads_current_disk() {
    let (_directory, source, mut session) = fixture();
    let request = InputRequest::default();
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    let path = owner(OWNER);
    let resource = owner(RESOURCE);
    let form = session.owner(&path).unwrap().forms[0].clone();
    for (version, text) in [(1, "<PredefinedData>"), (2, "<Other/>")] {
        session
            .apply_resource_overlay(&resource, session.revision(), version, text, &request)
            .unwrap();
        assert_eq!(
            session.metadata_resource(&resource).unwrap().state,
            MetadataResourceState::Ready(Arc::from(text))
        );
        assert_eq!(
            session.owner(&path).unwrap().state,
            ModelState::Unavailable(InputFailure::Source(
                eska::project_core::source_format::Error {
                    code: "analysis_predefined_data_invalid",
                    location: vec![RESOURCE.to_owned()]
                }
            ))
        );
        assert!(
            session
                .resolve(&path, session.revision())
                .unwrap()
                .is_none()
        );
        assert_eq!(session.owner(&path).unwrap().forms[0], form);
        assert!(!names(&session, Locale::En).contains(&"Saved".to_owned()));
    }
    let current = test_predefined_project::data("DiskCurrent");
    fs::write(source.join(RESOURCE), &current).unwrap();
    session
        .close_resource_overlay(&resource, session.revision(), &request)
        .unwrap();
    assert_eq!(
        object(&session, &path).predefined_items[0].name,
        "DiskCurrent"
    );
    fs::remove_file(source.join(RESOURCE)).unwrap();
    session
        .load_resource_saved(&resource, session.revision(), &request)
        .unwrap();
    assert!(
        matches!(session.owner(&path).unwrap().state, ModelState::Unavailable(InputFailure::Source(ref error)) if error.code == "source_file_missing")
    );
    fs::write(source.join(RESOURCE), [0xff]).unwrap();
    session
        .load_resource_saved(&resource, session.revision(), &request)
        .unwrap();
    assert!(
        matches!(session.owner(&path).unwrap().state, ModelState::Unavailable(InputFailure::Source(ref error)) if error.code == "source_encoding_unsupported")
    );
    session
        .apply_resource_overlay(
            &resource,
            session.revision(),
            3,
            "<PredefinedData/>",
            &request,
        )
        .unwrap();
    assert!(object(&session, &path).predefined_items.is_empty());
    session
        .apply_overlay(&path, session.revision(), 1, "document: [", &request)
        .unwrap();
    assert!(
        session
            .resolve(&path, session.revision())
            .unwrap()
            .is_none()
    );
    assert!(
        session
            .owner(&path)
            .unwrap()
            .forms
            .iter()
            .all(|form| matches!(form.state, ModelState::Unavailable(_)))
    );
}

/// Replacing a resource uses its own reservation; bounded failures and cancellation release old text.
#[test]
fn predefined_data_obeys_shared_quota_size_and_cancellation() {
    let (_directory, source, mut session) = fixture();
    let request = InputRequest::default();
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    let path = owner(OWNER);
    let resource = owner(RESOURCE);
    let bytes = session.metadata_resource(&resource).unwrap().bytes();
    let total = session.resources().total_source_bytes;
    session.options.max_retained_source_bytes = total;
    let longer = test_predefined_project::data("NameLongerThanSaved");
    session
        .apply_resource_overlay(&resource, session.revision(), 1, &longer, &request)
        .unwrap();
    assert!(matches!(
        session.owner(&path).unwrap().state,
        ModelState::Unavailable(InputFailure::SourceBudgetExceeded { .. })
    ));
    assert_eq!(session.resources().total_source_bytes, total - bytes);
    session
        .close_resource_overlay(&resource, session.revision(), &request)
        .unwrap();
    assert_eq!(session.resources().total_source_bytes, total);
    let small = InputRequest {
        max_document_bytes: 1,
        ..InputRequest::default()
    };
    session
        .load_resource_saved(&resource, session.revision(), &small)
        .unwrap();
    assert!(matches!(
        session.owner(&path).unwrap().state,
        ModelState::Unavailable(InputFailure::DocumentTooLarge { .. })
    ));
    session
        .load_resource_saved(&resource, session.revision(), &request)
        .unwrap();
    let cancellation = Cancellation::default();
    cancellation.cancel();
    let cancelled = InputRequest {
        cancellation,
        ..InputRequest::default()
    };
    session
        .load_resource_saved(&resource, session.revision(), &cancelled)
        .unwrap();
    assert_eq!(
        session.owner(&path).unwrap().state,
        ModelState::Unavailable(InputFailure::Cancelled)
    );
    assert_eq!(session.resources().total_source_bytes, total - bytes);
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    assert!(session.summary().descriptor_models_complete);
    assert_eq!(session.resources().total_source_bytes, total);
    assert_eq!(object(&session, &path).predefined_items[0].name, "Saved");
    assert_eq!(
        fs::read_to_string(source.join(RESOURCE)).unwrap(),
        test_predefined_project::data("Saved")
    );
}
