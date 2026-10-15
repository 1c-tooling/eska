//! Form resources drive current module semantics through the shared resource controller.

use std::{fs, path::PathBuf, sync::Arc};

use hir::DefDatabase;
use ide_db::metadata::CanonicalMetadata;

use super::*;
use crate::{
    DiagnosticCode, DiagnosticsConfig, ModelState, QueryCancellation, SessionOptions,
    session::tests::{owner, support::TestDirectory},
    test_form_project,
};

const OWNER: &str = "CommonForms/Dialog/object.yaml";
const RESOURCE: &str = "CommonForms/Dialog/Ext/Form.xml";
const MODULE: &str = "CommonForms/Dialog/Ext/Form/Module.bsl";

/// Every imported native/source directory belongs exclusively to this test.
fn fixture(absent: bool) -> (TestDirectory, PathBuf, AnalysisSession) {
    let directory = TestDirectory::new();
    let root = crate::test_project::project(&directory.0, "// Shared body.\n", false);
    test_form_project::add_forms(&root, &directory.0.join("native"), absent);
    let source = root.join("src-with-forms");
    let session = AnalysisSession::open(&source).unwrap();
    (directory, source, session)
}

/// Inspect the tracked resolver, independently of the tree's retained projection.
fn form(session: &AnalysisSession, path: &SourcePath) -> Arc<bsl_metadata::Form> {
    let Some(CanonicalMetadata::Form(value)) = session.resolve(path, session.revision()).unwrap()
    else {
        panic!("expected current form: {:?}", session.owner(path));
    };
    value
}

/// A native BSL diagnostic observes the current dialog bindings, with unchanged module text.
fn unused_method(session: &AnalysisSession) -> String {
    let diagnostics = session
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
    assert_eq!(diagnostics.len(), 1);
    let range = diagnostics[0].range;
    test_form_project::BODY[usize::from(range.start())..usize::from(range.end())].to_owned()
}

/// Pending declared bytes cannot become empty forms; absent/binary/stray resources stay explicit.
#[test]
fn pending_and_absent_dialogs_do_not_publish_inferred_form_models() {
    let (directory, source, mut session) = fixture(true);
    let request = InputRequest::default();
    let path = owner(OWNER);
    let resource = owner(RESOURCE);
    assert_eq!(session.resource_paths().count(), 2);
    assert_eq!(session.resources().total_source_bytes, 0);
    assert_eq!(session.resource_owner(&resource), Some(&path));
    assert_eq!(
        session.metadata_resource(&resource).unwrap().kind,
        MetadataResourceKind::FormDialog
    );
    assert!(
        session
            .metadata_resource(&owner("CommonForms/Stray/Ext/Form.xml"))
            .is_none()
    );
    session
        .load_saved(&path, session.revision(), &request)
        .unwrap();
    assert_eq!(session.owner(&path).unwrap().state, ModelState::Pending);
    assert!(
        session
            .resolve(&path, session.revision())
            .unwrap()
            .is_none()
    );
    session
        .load_resource_saved(&resource, session.revision(), &request)
        .unwrap();
    let text = fs::read_to_string(source.join(resource.as_path())).unwrap();
    let input = session.metadata_resource(&resource).unwrap();
    assert_eq!(
        input.state,
        MetadataResourceState::Ready(Arc::from(text.clone()))
    );
    assert_eq!(input.bytes(), text.len() as u64);
    assert_eq!(input.revision, session.revision());
    let current = form(&session, &path);
    let reference = bsl_metadata::xml_parser::parse_form_from_bsl_path(
        &directory.0.join("native").join(MODULE),
    )
    .unwrap();
    assert_eq!(*current, reference);
    assert_eq!(current.elements[1].parent_id, Some(1));
    assert_eq!(current.attributes[0].columns.len(), 1);
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    for name in ["Absent", "OrdinaryBinary"] {
        let path = owner(&format!("CommonForms/{name}/object.yaml"));
        assert!(
            matches!(&session.owner(&path).unwrap().state, ModelState::Unavailable(InputFailure::Source(error)) if error.code == "analysis_form_dialog_undeclared")
        );
        assert!(
            session
                .resolve(&path, session.revision())
                .unwrap()
                .is_none()
        );
    }
    assert!(!session.summary().main_models_complete);
}

/// Resource changes invalidate exactly one form/module context and retain independent YAML authority.
#[test]
fn dialog_overlay_changes_native_handler_use_and_close_reads_current_disk() {
    let (_directory, source, mut session) = fixture(false);
    let path = owner(OWNER);
    let resource = owner(RESOURCE);
    let neighbor = owner("CommonForms/Neighbor/object.yaml");
    let request = InputRequest::default();
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    assert!(session.summary().descriptor_models_complete);
    let saved = form(&session, &path);
    let untouched = form(&session, &neighbor);
    let yaml = session.metadata(&path, session.revision()).unwrap();
    let disk = fs::read(source.join(resource.as_path())).unwrap();
    assert_eq!(unused_method(&session), "CurrentHandler");
    session
        .apply_resource_overlay(
            &resource,
            session.revision(),
            7,
            &test_form_project::dialog("CurrentHandler"),
            &request,
        )
        .unwrap();
    let current = form(&session, &path);
    assert_eq!(current.uuid, saved.uuid);
    assert_eq!(current.command_handlers, ["CurrentHandler"]);
    assert_eq!(saved.command_handlers, ["SavedHandler"]);
    assert_eq!(unused_method(&session), "SavedHandler");
    assert!(Arc::ptr_eq(&untouched, &form(&session, &neighbor)));
    let file = session
        .selected_root
        .modules
        .get(&owner(MODULE))
        .unwrap()
        .file;
    let context = session
        .database
        .module_metadata(hir::ModuleId { file_id: file });
    assert!(Arc::ptr_eq(context.form.as_ref().unwrap(), &current));
    assert_eq!(
        session.owner(&path).unwrap().source_sha256,
        yaml.source_sha256
    );
    assert_eq!(session.owner(&path).unwrap().origin, InputOrigin::Saved);
    assert_eq!(fs::read(source.join(resource.as_path())).unwrap(), disk);
    let revision = session.revision();
    let resources = session.resources();
    assert!(
        session
            .apply_resource_overlay(&resource, revision, 7, "invalid", &request)
            .is_err()
    );
    assert!(
        session
            .apply_resource_overlay(
                &resource,
                SessionRevision(revision.0 - 1),
                8,
                "invalid",
                &request
            )
            .is_err()
    );
    assert!(
        session
            .load_resource_saved(&resource, revision, &request)
            .is_err()
    );
    assert!(
        session
            .apply_resource_overlay(
                &owner("CommonForms/Stray/Ext/Form.xml"),
                revision,
                8,
                "invalid",
                &request
            )
            .is_err()
    );
    assert_eq!(session.revision(), revision);
    assert_eq!(session.resources(), resources);
    assert!(Arc::ptr_eq(&current, &form(&session, &path)));
    fs::write(
        source.join(resource.as_path()),
        test_form_project::dialog("CurrentHandler"),
    )
    .unwrap();
    session
        .close_resource_overlay(&resource, session.revision(), &request)
        .unwrap();
    assert_eq!(unused_method(&session), "SavedHandler");
    assert_eq!(
        session.metadata_resource(&resource).unwrap().origin,
        InputOrigin::Saved
    );
}

/// Malformed current XML retains raw bytes but clears the canonical form and module context.
#[test]
fn invalid_xml_or_yaml_clears_current_bindings_without_fallback() {
    let (_directory, source, mut session) = fixture(false);
    let path = owner(OWNER);
    let resource = owner(RESOURCE);
    let request = InputRequest::default();
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    let text = fs::read_to_string(source.join(path.as_path())).unwrap();
    fs::write(source.join("CommonForms/Dialog.xml"), "stray XML fallback").unwrap();
    session
        .apply_resource_overlay(&resource, session.revision(), 1, "<NotForm/>", &request)
        .unwrap();
    assert_eq!(session.metadata_resource(&resource).unwrap().bytes(), 10);
    assert!(
        session
            .metadata_resource(&resource)
            .unwrap()
            .source_sha256
            .is_some()
    );
    assert_eq!(
        session.owner(&path).unwrap().state,
        ModelState::Unavailable(InputFailure::Source(
            eska::project_core::source_format::Error {
                code: "analysis_form_dialog_invalid",
                location: vec![RESOURCE.to_owned()],
            }
        ))
    );
    assert!(
        session
            .resolve(&path, session.revision())
            .unwrap()
            .is_none()
    );
    let file = session
        .selected_root
        .modules
        .get(&owner(MODULE))
        .unwrap()
        .file;
    assert!(
        session
            .database
            .module_metadata(hir::ModuleId { file_id: file })
            .form
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
        .apply_overlay(&path, session.revision(), 1, "document: [", &request)
        .unwrap();
    session
        .apply_resource_overlay(
            &resource,
            session.revision(),
            2,
            &test_form_project::dialog("CurrentHandler"),
            &request,
        )
        .unwrap();
    assert!(
        session
            .resolve(&path, session.revision())
            .unwrap()
            .is_none()
    );
    session
        .apply_overlay(
            &path,
            session.revision(),
            2,
            &text.replace("Managed", "Unrecognized"),
            &request,
        )
        .unwrap();
    assert!(matches!(
        session.owner(&path).unwrap().state,
        ModelState::Invalid { .. }
    ));
    session
        .close_overlay(&path, session.revision(), &request)
        .unwrap();
    assert_eq!(form(&session, &path).command_handlers, ["CurrentHandler"]);
    session
        .close_resource_overlay(&resource, session.revision(), &request)
        .unwrap();
    assert_eq!(form(&session, &path).command_handlers, ["SavedHandler"]);
}

/// Saved invalidation is atomic, preserves editor authority and retries only cancelled saved reads.
#[test]
fn saved_dialog_changes_failures_and_cancellation_share_one_controller() {
    let (_directory, source, mut session) = fixture(false);
    let path = owner(OWNER);
    let resource = owner(RESOURCE);
    let neighbor = owner("CommonForms/Neighbor/Ext/Form.xml");
    let request = InputRequest::default();
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    let revision = session.revision();
    let resources = session.resources();
    assert!(
        session
            .invalidate_saved_inputs(&[resource.clone(), owner("unknown.xml")], revision)
            .is_err()
    );
    assert_eq!(session.revision(), revision);
    assert_eq!(session.resources(), resources);
    session
        .apply_resource_overlay(
            &neighbor,
            revision,
            1,
            &test_form_project::dialog("CurrentHandler"),
            &request,
        )
        .unwrap();
    let before = session.resources().total_source_bytes;
    let bytes = session.metadata_resource(&resource).unwrap().bytes();
    fs::remove_file(source.join(resource.as_path())).unwrap();
    assert_eq!(
        session
            .invalidate_saved_inputs(
                &[resource.clone(), neighbor, resource.clone()],
                session.revision()
            )
            .unwrap()
            .as_slice(),
        std::slice::from_ref(&resource)
    );
    assert_eq!(session.resources().total_source_bytes, before - bytes);
    assert_eq!(session.owner(&path).unwrap().state, ModelState::Pending);
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    assert!(
        matches!(&session.metadata_resource(&resource).unwrap().state, MetadataResourceState::Unavailable(InputFailure::Source(error)) if error.code == "source_file_missing")
    );
    fs::write(source.join(resource.as_path()), [0xff]).unwrap();
    session
        .load_resource_saved(&resource, session.revision(), &request)
        .unwrap();
    assert!(
        matches!(&session.metadata_resource(&resource).unwrap().state, MetadataResourceState::Unavailable(InputFailure::Source(error)) if error.code == "source_encoding_unsupported")
    );
    fs::write(
        source.join(resource.as_path()),
        test_form_project::dialog("SavedHandler"),
    )
    .unwrap();
    let cancelled = InputRequest::default();
    cancelled.cancellation.cancel();
    session
        .load_resource_saved(&resource, session.revision(), &cancelled)
        .unwrap();
    assert_eq!(
        session.metadata_resource(&resource).unwrap().state,
        MetadataResourceState::Unavailable(InputFailure::Cancelled)
    );
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    assert_eq!(form(&session, &path).command_handlers, ["SavedHandler"]);
    session
        .apply_resource_overlay(&resource, session.revision(), 1, "editor", &cancelled)
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
        .close_resource_overlay(&resource, session.revision(), &request)
        .unwrap();
    assert_eq!(form(&session, &path).command_handlers, ["SavedHandler"]);
}

/// A dialog competes with YAML and BSL for the same quota and reuses its prior reservation.
#[test]
fn dialog_bytes_obey_shared_budget_and_revision_exhaustion() {
    let (_directory, source, session) = fixture(false);
    let resource = owner(RESOURCE);
    let path = owner(OWNER);
    let text = fs::read_to_string(source.join(resource.as_path())).unwrap();
    let yaml_bytes = fs::metadata(source.join(path.as_path())).unwrap().len();
    let limit = text.len() as u64 + yaml_bytes;
    drop(session);
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
        .load_module_saved(&owner(MODULE), session.revision(), &request)
        .unwrap();
    assert!(matches!(
        session.module(&owner(MODULE)).unwrap().state,
        crate::ModuleState::Unavailable(_)
    ));
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

/// Roles and forms coexist in one map and compete for one reservation without changing kinds.
#[test]
fn role_and_form_resources_share_budget_without_crossing_owner_boundaries() {
    let directory = TestDirectory::new();
    let root = crate::test_project::project(&directory.0, "// Shared body.\n", false);
    let native = directory.0.join("native");
    let configuration = fs::read_to_string(native.join("Configuration.xml")).unwrap();
    fs::write(
        native.join("Configuration.xml"),
        configuration.replace("</ChildObjects>", "<Role>Reader</Role></ChildObjects>"),
    )
    .unwrap();
    fs::create_dir_all(native.join("Roles/Reader/Ext")).unwrap();
    fs::write(native.join("Roles/Reader.xml"), "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Role uuid=\"70000000-0000-4000-8000-000000000001\"><Properties><Name>Reader</Name></Properties></Role></MetaDataObject>").unwrap();
    let rights = crate::test_role_project::rights("Catalog.Items", false);
    fs::write(native.join("Roles/Reader/Ext/Rights.xml"), &rights).unwrap();
    test_form_project::add_forms(&root, &native, false);
    let source = root.join("src-with-forms");
    let dialog = fs::read_to_string(source.join(RESOURCE)).unwrap();
    let limit = (rights.len() + dialog.len()) as u64;
    let mut session = AnalysisSession::open_with_options(
        &source,
        SessionOptions {
            max_retained_source_bytes: limit,
            ..SessionOptions::default()
        },
    )
    .unwrap();
    let role = owner("Roles/Reader/Ext/Rights.xml");
    let form = owner(RESOURCE);
    let request = InputRequest::default();
    session
        .load_resource_saved(&role, session.revision(), &request)
        .unwrap();
    session
        .load_resource_saved(&form, session.revision(), &request)
        .unwrap();
    assert_eq!(session.resources().total_source_bytes, limit);
    assert_eq!(
        session.metadata_resource(&role).unwrap().kind,
        MetadataResourceKind::RoleRights
    );
    assert_eq!(
        session.metadata_resource(&form).unwrap().kind,
        MetadataResourceKind::FormDialog
    );
    let untouched = session.metadata_resource(&role).unwrap().clone();
    session
        .apply_resource_overlay(&form, session.revision(), 1, &(dialog + " "), &request)
        .unwrap();
    assert!(matches!(
        session.metadata_resource(&form).unwrap().state,
        MetadataResourceState::Unavailable(InputFailure::SourceBudgetExceeded { .. })
    ));
    assert_eq!(session.metadata_resource(&role), Some(&untouched));
    assert_eq!(session.resources().total_source_bytes, rights.len() as u64);
    assert_eq!(
        session.resource_owner(&role),
        Some(&owner("Roles/Reader/object.yaml"))
    );
    assert_eq!(session.resource_owner(&form), Some(&owner(OWNER)));
}

/// A declared nested resource reaches its own slot and the physical BSL module context.
#[test]
fn nested_descriptor_registers_its_own_canonical_input() {
    let (directory, _source, session) = fixture(false);
    drop(session);
    let native = directory.0.join("native");
    let configuration = fs::read_to_string(native.join("Configuration.xml")).unwrap();
    fs::write(
        native.join("Configuration.xml"),
        configuration.replace("</ChildObjects>", "<Catalog>Items</Catalog></ChildObjects>"),
    )
    .unwrap();
    fs::create_dir_all(native.join("Catalogs/Items/Forms/Dialog/Ext/Form")).unwrap();
    fs::write(native.join("Catalogs/Items.xml"), "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Catalog uuid=\"90000000-0000-4000-8000-000000000001\"><Properties><Name>Items</Name></Properties><ChildObjects><Form>Dialog</Form></ChildObjects></Catalog></MetaDataObject>").unwrap();
    fs::write(native.join("Catalogs/Items/Forms/Dialog.xml"), "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Form uuid=\"90000000-0000-4000-8000-000000000002\"><Properties><Name>Dialog</Name><FormType>Managed</FormType></Properties></Form></MetaDataObject>").unwrap();
    fs::write(
        native.join("Catalogs/Items/Forms/Dialog/Ext/Form.xml"),
        test_form_project::dialog("ShouldNeverReachKernel"),
    )
    .unwrap();
    let module = owner("Catalogs/Items/Forms/Dialog/Ext/Form/Module.bsl");
    fs::write(native.join(module.as_path()), test_form_project::BODY).unwrap();
    let source = directory.0.join("nested-source");
    eska::project_core::source_format::import_designer(&native, &source).unwrap();
    let reader = SourceReader::open(&source).unwrap();
    assert_eq!(reader.form_dialog_paths().count(), 3);
    let resource = owner("Catalogs/Items/Forms/Dialog/Ext/Form.xml");
    assert_eq!(
        reader.form_for_dialog(&resource).unwrap().owner,
        &owner("Catalogs/Items/object.yaml")
    );
    let mut session = AnalysisSession::open(&source).unwrap();
    let request = InputRequest::default();
    session
        .load_semantic_inputs(session.revision(), &request)
        .unwrap();
    assert_eq!(session.resource_paths().count(), 3);
    assert!(session.metadata_resource(&resource).is_some());
    let file = session.selected_root.modules.get(&module).unwrap().file;
    assert_eq!(
        session
            .database
            .module_metadata(hir::ModuleId { file_id: file })
            .form
            .as_ref()
            .unwrap()
            .event_handlers[0]
            .handler_name,
        "ShouldNeverReachKernel"
    );
    let catalog = session.owner(&owner("Catalogs/Items/object.yaml")).unwrap();
    assert!(matches!(catalog.state, ModelState::Ready(_)));
    assert_eq!((catalog.descriptors, catalog.modeled_descriptors), (2, 2));
    assert_eq!(
        catalog.forms[0].native_path,
        owner("Catalogs/Items/Forms/Dialog.xml")
    );
    assert!(session.summary().descriptor_models_complete);
}
