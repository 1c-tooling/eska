//! Nested resources update only their own canonical input while sharing YAML authority and quota.

use std::{fs, path::PathBuf, sync::Arc};

use eska::analysis_core::{InputFailure, InputRequest};
use hir::DefDatabase;
use ide_db::metadata::CanonicalMetadata;

use crate::{
    AnalysisSession, DiagnosticCode, DiagnosticsConfig, ModelState, QueryCancellation,
    SessionOptions, SessionRequestError, SessionRevision,
    session::tests::{owner, support::TestDirectory},
    test_form_project,
};

const OWNER: &str = "Catalogs/Items/object.yaml";
const NATIVE: &str = "Catalogs/Items/Forms/Shared.xml";
const DIALOG: &str = "Catalogs/Items/Forms/Shared/Ext/Form.xml";
const MODULE: &str = "Catalogs/Items/Forms/Shared/Ext/Form/Module.bsl";
const OTHER: &str = "Catalogs/Items/Forms/Other/Ext/Form/Module.bsl";
const FOREIGN: &str = "Catalogs/Orders/Forms/Shared/Ext/Form/Module.bsl";

/// Imported files and source mutations belong to this test's unique playground directory.
fn fixture(absent: bool) -> (TestDirectory, PathBuf, AnalysisSession) {
    let directory = TestDirectory::new();
    let root = crate::test_project::project(&directory.0, "// Common body.\n", false);
    test_form_project::add_nested_forms(&root, &directory.0.join("native"), absent);
    let source = root.join("src-with-nested-forms");
    let session = AnalysisSession::open(&source).unwrap();
    (directory, source, session)
}

/// A separated form owns its YAML revision and dialog, without invalidating a sibling or parent.
#[test]
fn separated_form_yaml_and_dialog_drive_only_their_own_current_module_bindings() {
    let (directory, legacy, _) = fixture(false);
    let source = directory.0.join("separated");
    eska::project_core::source_format::prepare_descriptor_layout(&legacy)
        .unwrap()
        .publish(&source)
        .unwrap();
    let mut session = AnalysisSession::open(&source).unwrap();
    let form = owner("Catalogs/Items/Forms/Shared/form.yaml");
    let request = InputRequest::default();
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    assert!(session.summary().descriptor_models_complete);
    assert_eq!(session.resource_owner(&owner(DIALOG)), Some(&form));
    let model = session.owner(&form).unwrap();
    assert_eq!((model.descriptors, model.modeled_descriptors), (1, 1));
    assert!(model.forms.is_empty());
    let initial = module_metadata(&session, MODULE);
    let expected = bsl_metadata::xml_parser::parse_form_from_bsl_path(
        &directory.0.join("native").join(MODULE),
    )
    .unwrap();
    assert_eq!(**initial.form.as_ref().unwrap(), expected);
    let parent = session
        .resolve(&owner(OWNER), session.revision())
        .unwrap()
        .unwrap();
    let neighbor = module_metadata(&session, OTHER);
    let foreign = module_metadata(&session, FOREIGN);
    let saved = fs::read_to_string(source.join(form.as_path())).unwrap();
    let bsl = fs::read(source.join(MODULE)).unwrap();
    assert_eq!(unused_method(&session), "CurrentHandler");
    session
        .apply_overlay(&form, session.revision(), 1, "[", &request)
        .unwrap();
    assert!(module_metadata(&session, MODULE).form.is_none());
    assert!(Arc::ptr_eq(&neighbor, &module_metadata(&session, OTHER)));
    assert!(Arc::ptr_eq(&foreign, &module_metadata(&session, FOREIGN)));
    session
        .apply_overlay(&form, session.revision(), 2, &saved, &request)
        .unwrap();
    session
        .apply_resource_overlay(
            &owner(DIALOG),
            session.revision(),
            7,
            &test_form_project::dialog("CurrentHandler"),
            &request,
        )
        .unwrap();
    assert_eq!(unused_method(&session), "SavedHandler");
    assert!(Arc::ptr_eq(&neighbor, &module_metadata(&session, OTHER)));
    assert!(Arc::ptr_eq(&foreign, &module_metadata(&session, FOREIGN)));
    let CanonicalMetadata::Object(parent) = parent else {
        panic!("catalog")
    };
    let Some(CanonicalMetadata::Object(current)) =
        session.resolve(&owner(OWNER), session.revision()).unwrap()
    else {
        panic!("current catalog")
    };
    assert!(Arc::ptr_eq(&parent, &current));
    assert_eq!(fs::read(source.join(MODULE)).unwrap(), bsl);
    assert_eq!(
        fs::read_to_string(source.join(form.as_path())).unwrap(),
        saved
    );
    let snapshot = session.metadata(&form, session.revision()).unwrap();
    let eska::analysis_core::InputState::Ready(input) = snapshot.input else {
        panic!("current YAML")
    };
    assert_eq!(
        input.documents[0].descriptor.objects[0]
            .metadata
            .id()
            .as_str(),
        "catalog:Items/form:Shared"
    );
}

/// Query the real physical module consumer rather than only inspecting the tree projection.
fn module_metadata(session: &AnalysisSession, module: &str) -> Arc<hir::ModuleMetadata> {
    let file_id = session.modules.get(&owner(module)).unwrap().file;
    session.database.module_metadata(hir::ModuleId { file_id })
}

/// Current form event/command handlers must change a native rule without changing BSL bytes.
fn unused_method(session: &AnalysisSession) -> String {
    let value = session
        .module_diagnostics(
            &owner(MODULE),
            session.revision(),
            &DiagnosticsConfig {
                only_enabled: Some(vec![DiagnosticCode::UnusedLocalMethod]),
                ..DiagnosticsConfig::default()
            },
            &QueryCancellation::default(),
        )
        .unwrap()
        .value;
    assert_eq!(value.len(), 1);
    let range = value[0].range;
    test_form_project::BODY[usize::from(range.start())..usize::from(range.end())].to_owned()
}

/// Pending, absent and binary dialogs cannot obtain managed bindings from disk fallback.
#[test]
fn full_identities_and_nested_coverage_match_native_models() {
    let (directory, _source, mut session) = fixture(true);
    assert_eq!(session.resource_paths().count(), 3);
    assert_eq!(session.resources().total_source_bytes, 0);
    let request = InputRequest::default();
    session
        .load_saved(&owner(OWNER), session.revision(), &request)
        .unwrap();
    let model = session.owner(&owner(OWNER)).unwrap();
    assert!(matches!(model.state, ModelState::Ready(_)));
    assert_eq!((model.descriptors, model.modeled_descriptors), (5, 1));
    assert!(module_metadata(&session, MODULE).form.is_none());
    assert!(
        session
            .metadata_resource(&owner("Catalogs/Items/Forms/Stray/Ext/Form.xml"))
            .is_none()
    );
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    for module in [MODULE, OTHER, FOREIGN] {
        let current = module_metadata(&session, module).form.clone().unwrap();
        let reference = bsl_metadata::xml_parser::parse_form_from_bsl_path(
            &directory.0.join("native").join(module),
        )
        .unwrap();
        assert_eq!(*current, reference);
    }
    let current = module_metadata(&session, MODULE).form.clone().unwrap();
    let foreign = module_metadata(&session, FOREIGN).form.clone().unwrap();
    assert_eq!(current.name, foreign.name);
    assert_ne!(current.uuid, foreign.uuid);
    let model = session.owner(&owner(OWNER)).unwrap();
    assert_eq!((model.descriptors, model.modeled_descriptors), (5, 3));
    for form in &model.forms {
        if form.dialog.is_none() {
            assert!(
                matches!(&form.state, ModelState::Unavailable(InputFailure::Source(error)) if error.code == "analysis_form_dialog_undeclared")
            );
        }
    }
    assert!(session.summary().main_models_complete);
    assert!(!session.summary().descriptor_models_complete);
}

/// Editing one dialog preserves sibling/main values and memoized module context within the same owner.
#[test]
fn nested_resource_lifecycle_preserves_unaffected_slots_and_editor_authority() {
    let (_directory, source, mut session) = fixture(false);
    let request = InputRequest::default();
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    let primary = session
        .resolve(&owner(OWNER), session.revision())
        .unwrap()
        .unwrap();
    let neighbor = module_metadata(&session, OTHER);
    let foreign = module_metadata(&session, FOREIGN);
    let initial = module_metadata(&session, MODULE);
    let bsl = fs::read(source.join(MODULE)).unwrap();
    assert_eq!(unused_method(&session), "CurrentHandler");
    session
        .apply_resource_overlay(
            &owner(DIALOG),
            session.revision(),
            7,
            &test_form_project::dialog("CurrentHandler"),
            &request,
        )
        .unwrap();
    let current = module_metadata(&session, MODULE);
    assert_eq!(
        current.form.as_ref().unwrap().event_handlers[0].handler_name,
        "CurrentHandler"
    );
    assert!(!Arc::ptr_eq(&initial, &current));
    assert!(Arc::ptr_eq(&neighbor, &module_metadata(&session, OTHER)));
    assert!(Arc::ptr_eq(&foreign, &module_metadata(&session, FOREIGN)));
    let Some(CanonicalMetadata::Object(primary_now)) =
        session.resolve(&owner(OWNER), session.revision()).unwrap()
    else {
        panic!("current main object")
    };
    let CanonicalMetadata::Object(primary) = primary else {
        panic!("main object")
    };
    assert!(Arc::ptr_eq(&primary, &primary_now));
    assert_eq!(unused_method(&session), "SavedHandler");
    assert!(session.summary().descriptor_models_complete);
    let revision = session.revision();
    assert!(matches!(
        session.apply_resource_overlay(&owner(DIALOG), revision, 7, "<Form/>", &request),
        Err(SessionRequestError::StaleDocumentVersion { .. })
    ));
    assert!(matches!(
        session.apply_resource_overlay(
            &owner(DIALOG),
            SessionRevision(revision.0 - 1),
            8,
            "<Form/>",
            &request
        ),
        Err(SessionRequestError::StaleRevision { .. })
    ));
    assert!(matches!(
        session.load_resource_saved(&owner(DIALOG), revision, &request),
        Err(SessionRequestError::OverlayActive { .. })
    ));
    assert!(
        session
            .invalidate_saved_inputs(&[owner(DIALOG)], revision)
            .unwrap()
            .is_empty()
    );
    assert_eq!(session.revision(), revision);
    session
        .apply_resource_overlay(&owner(DIALOG), revision, 8, "<NotForm/>", &request)
        .unwrap();
    assert!(module_metadata(&session, MODULE).form.is_none());
    assert!(Arc::ptr_eq(&neighbor, &module_metadata(&session, OTHER)));
    let model = session.owner(&owner(OWNER)).unwrap();
    assert!(matches!(model.state, ModelState::Ready(_)));
    assert_eq!((model.descriptors, model.modeled_descriptors), (3, 2));
    assert!(
        matches!(&model.forms.iter().find(|form| form.native_path == owner(NATIVE)).unwrap().state, ModelState::Unavailable(InputFailure::Source(error)) if error.code == "analysis_form_dialog_invalid")
    );
    assert_eq!(
        session.metadata_resource(&owner(DIALOG)).unwrap().bytes(),
        10
    );
    assert!(!session.summary().descriptor_models_complete);
    fs::write(
        source.join(DIALOG),
        test_form_project::dialog("CurrentHandler"),
    )
    .unwrap();
    session
        .close_resource_overlay(&owner(DIALOG), session.revision(), &request)
        .unwrap();
    assert_eq!(unused_method(&session), "SavedHandler");
    assert!(session.summary().descriptor_models_complete);
    assert_eq!(fs::read(source.join(MODULE)).unwrap(), bsl);
}

/// Owner failures clear every dependent form while a recoverable leaf issue keeps valid siblings ready.
#[test]
fn current_owner_yaml_controls_all_nested_slots_and_captured_adapter_ranges() {
    let (_directory, source, mut session) = fixture(false);
    let request = InputRequest::default();
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    let saved = fs::read_to_string(source.join(OWNER)).unwrap();
    let foreign = module_metadata(&session, FOREIGN);
    session
        .apply_overlay(&owner(OWNER), session.revision(), 1, "[", &request)
        .unwrap();
    assert!(module_metadata(&session, MODULE).form.is_none());
    assert!(module_metadata(&session, OTHER).form.is_none());
    assert!(Arc::ptr_eq(&foreign, &module_metadata(&session, FOREIGN)));
    session
        .apply_overlay(&owner(OWNER), session.revision(), 2, &saved, &request)
        .unwrap();
    assert!(session.summary().descriptor_models_complete);
    // Alter only the nested form's value; coordinates must still refer to this owner's current YAML.
    let broken = saved.replacen("Managed", "Unsupported", 1);
    assert_ne!(broken, saved);
    session
        .apply_overlay(&owner(OWNER), session.revision(), 3, &broken, &request)
        .unwrap();
    let model = session.owner(&owner(OWNER)).unwrap();
    let invalid = model
        .forms
        .iter()
        .find(|form| matches!(form.state, ModelState::Invalid { .. }))
        .unwrap();
    let diagnostics = session
        .metadata_diagnostics(
            &owner(OWNER),
            session.revision(),
            &QueryCancellation::default(),
        )
        .unwrap();
    assert!(
        diagnostics
            .findings
            .iter()
            .any(|finding| finding.native_path == invalid.native_path)
    );
    assert!(matches!(model.state, ModelState::Ready(_)));
    session
        .close_overlay(&owner(OWNER), session.revision(), &request)
        .unwrap();
    assert!(session.summary().descriptor_models_complete);
    fs::remove_file(source.join(OWNER)).unwrap();
    session
        .invalidate_saved_inputs(&[owner(OWNER)], session.revision())
        .unwrap();
    assert!(module_metadata(&session, MODULE).form.is_none());
    assert!(module_metadata(&session, OTHER).form.is_none());
    session
        .load_saved(&owner(OWNER), session.revision(), &request)
        .unwrap();
    assert!(
        session
            .owner(&owner(OWNER))
            .unwrap()
            .forms
            .iter()
            .all(|form| matches!(form.state, ModelState::Unavailable(_)))
    );
}

/// Accepted cancellation/quota/encoding failures clear only the selected form and release its reservation.
#[test]
fn nested_saved_failures_share_the_current_source_budget_and_clear_only_one_form() {
    let (_directory, source, mut session) = fixture(false);
    let request = InputRequest::default();
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    let neighbor = module_metadata(&session, OTHER);
    let total = session.resources().total_source_bytes;
    let text = fs::read_to_string(source.join(DIALOG)).unwrap();
    session.options = SessionOptions {
        max_retained_source_bytes: total,
        ..session.options
    };
    session
        .apply_resource_overlay(
            &owner(DIALOG),
            session.revision(),
            1,
            &(text.clone() + " "),
            &request,
        )
        .unwrap();
    assert!(matches!(
        session.metadata_resource(&owner(DIALOG)).unwrap().state,
        crate::MetadataResourceState::Unavailable(InputFailure::SourceBudgetExceeded { .. })
    ));
    assert!(module_metadata(&session, MODULE).form.is_none());
    assert!(Arc::ptr_eq(&neighbor, &module_metadata(&session, OTHER)));
    assert_eq!(
        session.resources().total_source_bytes,
        total - text.len() as u64
    );
    session
        .close_resource_overlay(&owner(DIALOG), session.revision(), &request)
        .unwrap();
    let cancelled = InputRequest::default();
    cancelled.cancellation.cancel();
    session
        .load_resource_saved(&owner(DIALOG), session.revision(), &cancelled)
        .unwrap();
    assert!(module_metadata(&session, MODULE).form.is_none());
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    assert!(module_metadata(&session, MODULE).form.is_some());
    fs::write(source.join(DIALOG), [0xff]).unwrap();
    session
        .invalidate_saved_inputs(&[owner(DIALOG)], session.revision())
        .unwrap();
    assert!(module_metadata(&session, MODULE).form.is_none());
    session
        .load_resource_saved(&owner(DIALOG), session.revision(), &request)
        .unwrap();
    assert!(
        matches!(&session.metadata_resource(&owner(DIALOG)).unwrap().state, crate::MetadataResourceState::Unavailable(InputFailure::Source(error)) if error.code == "source_encoding_unsupported")
    );
    assert!(Arc::ptr_eq(&neighbor, &module_metadata(&session, OTHER)));
    assert_eq!(
        session.resources().total_source_bytes,
        total - text.len() as u64
    );
    assert!(session.summary().main_models_complete);
    assert!(!session.summary().descriptor_models_complete);
}
