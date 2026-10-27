use super::*;
use crate::backend::project::{
    metadata_edit::{EditError, PropertyChange},
    metadata_workspace::{EditingSnapshot, PropertyEditError, RenameError},
};

/// Create real scalar history before opening the competing source session.
fn comment_history(project: &mut ProjectSession, id: &ObjectId) -> EditingSnapshot {
    let editing = project.property_editing(id).unwrap();
    let comment = &editing
        .properties
        .fields
        .iter()
        .find(|field| field.value == "before")
        .unwrap()
        .path;
    project
        .update_property(
            id,
            &editing.properties.snapshot,
            None,
            comment,
            &PropertyChange::Text {
                value: "after".into(),
            },
        )
        .unwrap();
    project.property_editing(id).unwrap()
}

/// Competing source and descriptor writers cannot borrow a source-session lease or bypass it through history.
#[test]
fn source_session_fences_direct_query_property_and_undo_writes() {
    let (directory, mut workspace, node, path) = xml_fixture();
    let descriptor = directory.0.join("src/CommonForms/Queries.xml");
    let input = fs::read_to_string(&descriptor)
        .unwrap()
        .replace("</Properties>", "<Comment>before</Comment></Properties>");
    fs::write(&descriptor, input).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let NodeId::Object(id) = &node else {
        panic!("form owner")
    };
    let editing = comment_history(project, id);
    let comment = &editing
        .properties
        .fields
        .iter()
        .find(|field| field.value == "after")
        .unwrap()
        .path;
    assert!(editing.writable);
    let owner = XML.strip_prefix('\u{feff}').unwrap();
    let mut session = xml_session(project, &node, &path, owner);
    assert!(session.source_restriction_id().is_some());
    let document = session.document().clone();
    let before = bytes(&directory.0);
    assert!(!project.property_editing(id).unwrap().writable);
    assert!(matches!(
        project.update_property(
            id,
            &editing.properties.snapshot,
            None,
            comment,
            &PropertyChange::Text {
                value: "forbidden".into()
            }
        ),
        Err(PropertyEditError::Edit(EditError::ReadOnly))
    ));
    assert!(matches!(
        project.undo_property(id, &editing.properties.snapshot, None, true),
        Err(PropertyEditError::Edit(EditError::ReadOnly))
    ));
    assert!(matches!(
        project.plan_query(
            &document,
            revision(&document),
            session.identity(),
            "SELECT forbidden",
            &[],
            None
        ),
        Err(QueryDocumentError::ReadOnly)
    ));
    assert!(matches!(
        QuerySession::open(
            project,
            document.clone(),
            session.identity().clone(),
            None,
            QueryOwnerFormat::Editor,
            QueryOwnerRestrictions::default()
        ),
        Err(QuerySessionError::OwnerBusy)
    ));
    let prepared = session
        .prepare(
            project,
            session.revision(),
            editor(7, &snapshot(owner), false),
            &[],
            None,
        )
        .unwrap();
    assert!(
        prepared.plan.is_empty(),
        "only the owning source session may prepare its plan"
    );
    session.abort_apply(prepared.token).unwrap();
    assert_eq!(bytes(&directory.0), before);
    session.close();
    assert_eq!(session.source_restriction_id(), None);
    assert!(project.property_editing(id).unwrap().writable);
    project
        .plan_query(
            &document,
            revision(&document),
            session.identity(),
            "SELECT permitted",
            &[],
            None,
        )
        .unwrap();
    project
        .undo_property(id, &editing.properties.snapshot, None, true)
        .unwrap();
    assert!(
        fs::read_to_string(descriptor)
            .unwrap()
            .contains("<Comment>before</Comment>")
    );
}

/// A reviewed plan cannot move a retained owner's directory; an unrelated structural refresh retains the restriction.
#[test]
fn source_session_fences_rename_and_survives_unrelated_rename_refresh() {
    let (directory, mut workspace, node, path) = xml_fixture();
    let module = directory.0.join("src/CommonModules/ЗащищенныйМодуль/Ext");
    fs::remove_file(module.join("Module.bin")).unwrap();
    fs::write(module.join("Module.bsl"), "// ordinary source\n").unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let NodeId::Object(id) = &node else {
        panic!("form owner")
    };
    let reviewed = project.preview_rename(id, "Renamed").unwrap();
    assert!(reviewed.issues.is_empty(), "{:#?}", reviewed.issues);
    let mut session = xml_session(project, &node, &path, XML.strip_prefix('\u{feff}').unwrap());
    let before = bytes(&directory.0.join("src"));
    let preview = project.preview_rename(id, "Renamed").unwrap();
    assert!(
        preview
            .issues
            .iter()
            .any(|issue| issue.reason == "source_session_active")
    );
    assert!(matches!(
        project.apply_rename(id, "Renamed", &reviewed.snapshot, true),
        Err(RenameError::Blocked(_))
    ));
    assert_eq!(bytes(&directory.0.join("src")), before);
    let unrelated = object(MetadataKind::Catalog, "Контрагенты", None);
    let rename = project.preview_rename(&unrelated, "Partners").unwrap();
    assert!(rename.issues.is_empty(), "{:#?}", rename.issues);
    project
        .apply_rename(&unrelated, "Partners", &rename.snapshot, true)
        .unwrap();
    assert!(!project.property_editing(id).unwrap().writable);
    let refreshed = project.query_document(&node, &path, 7, None).unwrap();
    assert!(matches!(
        project.plan_query(
            &refreshed,
            revision(&refreshed),
            session.identity(),
            "SELECT forbidden",
            &[],
            None
        ),
        Err(QueryDocumentError::ReadOnly)
    ));
    session.close();
    assert!(project.property_editing(id).unwrap().writable);
}

/// Structural Undo must respect a source session opened after the original rename was committed.
#[test]
fn source_session_fences_existing_rename_history() {
    let (directory, mut workspace, node, _) = xml_fixture();
    let module = directory.0.join("src/CommonModules/ЗащищенныйМодуль/Ext");
    fs::remove_file(module.join("Module.bin")).unwrap();
    fs::write(module.join("Module.bsl"), "// ordinary source\n").unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let NodeId::Object(id) = &node else {
        panic!("form owner")
    };
    let plan = project.preview_rename(id, "Renamed").unwrap();
    let renamed = project
        .apply_rename(id, "Renamed", &plan.snapshot, true)
        .unwrap();
    let target = renamed.new_object_id;
    let editing = project.property_editing(&target).unwrap();
    let node = NodeId::Object(target.clone());
    let path = Path::new("CommonForms/Renamed/Ext/Form.xml");
    let mut session = xml_session(project, &node, path, XML.strip_prefix('\u{feff}').unwrap());
    let before = bytes(&directory.0.join("src"));
    assert!(matches!(
        project.undo_property(&target, &editing.properties.snapshot, None, true),
        Err(PropertyEditError::Edit(EditError::ReadOnly))
    ));
    assert_eq!(bytes(&directory.0.join("src")), before);
    session.close();
    project
        .undo_property(&target, &editing.properties.snapshot, None, true)
        .unwrap();
    assert!(
        directory
            .0
            .join("src/CommonForms/Queries/Ext/Form.xml")
            .exists()
    );
    assert!(!directory.0.join("src").join(path).exists());
}

/// Closing one session cannot release another owner, and dropping its last proposed state releases a forgotten lease.
#[test]
fn source_restrictions_release_only_their_own_lifetimes() {
    let (_directory, mut workspace, node, path) = xml_fixture();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let mut xml = xml_session(project, &node, &path, XML.strip_prefix('\u{feff}').unwrap());
    let NodeId::Object(root) = project.root().clone() else {
        panic!("root object")
    };
    let bsl_node = NodeId::Module {
        owner: root,
        role: ModuleRole::ManagedApplication,
    };
    let bsl_path = Path::new("Ext/ManagedApplicationModule.bsl");
    let bsl_document = project
        .query_document(&bsl_node, bsl_path, 7, None)
        .unwrap();
    let bsl_identity = identity(&bsl_document);
    let bsl = QuerySession::open(
        project,
        bsl_document.clone(),
        bsl_identity.clone(),
        None,
        QueryOwnerFormat::Source,
        QueryOwnerRestrictions {
            user_read_only: false,
        },
    )
    .unwrap();
    assert_ne!(xml.source_restriction_id(), bsl.source_restriction_id());
    let delayed_state = xml.clone();
    xml.close();
    let xml_document = xml.document();
    assert!(
        project
            .plan_query(
                xml_document,
                revision(xml_document),
                xml.identity(),
                "SELECT 9",
                &[],
                None
            )
            .is_err()
    );
    drop(delayed_state);
    project
        .plan_query(
            xml_document,
            revision(xml_document),
            xml.identity(),
            "SELECT 9",
            &[],
            None,
        )
        .unwrap();
    assert!(matches!(
        project.plan_query(
            &bsl_document,
            revision(&bsl_document),
            &bsl_identity,
            "ВЫБРАТЬ 9",
            &[],
            None
        ),
        Err(QueryDocumentError::ReadOnly)
    ));
    drop(bsl);
    project
        .plan_query(
            &bsl_document,
            revision(&bsl_document),
            &bsl_identity,
            "ВЫБРАТЬ 9",
            &[],
            None,
        )
        .unwrap();
}

/// Matching numeric IDs in another project session cannot authorize an older session's application.
#[test]
fn source_restriction_capability_is_bound_to_its_project_session() {
    let (directory, mut workspace, node, path) = xml_fixture();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let owner = XML.strip_prefix('\u{feff}').unwrap();
    let mut session = xml_session(project, &node, &path, owner);
    let mut replacement = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let fresh = replacement.project_mut(&ProjectScope::Standalone).unwrap();
    let document = fresh.query_document(&node, &path, 7, Some(owner)).unwrap();
    let selected = crate::backend::project::query::read_xml(owner).unwrap()[0]
        .identity
        .clone();
    assert!(matches!(
        QuerySession::open(
            fresh,
            document,
            selected,
            None,
            QueryOwnerFormat::Editor,
            QueryOwnerRestrictions::default()
        ),
        Err(QuerySessionError::OwnerBusy)
    ));
    let second_path = PathBuf::from("Ext/ManagedApplicationModule.bsl");
    fs::write(directory.0.join("src").join(&second_path), INPUT).unwrap();
    let NodeId::Object(root) = fresh.root().clone() else {
        panic!("root");
    };
    let second_node = NodeId::Module {
        owner: root,
        role: ModuleRole::ManagedApplication,
    };
    let document = fresh
        .query_document(&second_node, &second_path, 7, Some(INPUT))
        .unwrap();
    let selected = identity(&document);
    let other = QuerySession::open(
        fresh,
        document,
        selected,
        None,
        QueryOwnerFormat::Source,
        QueryOwnerRestrictions::default(),
    )
    .unwrap();
    assert_eq!(
        session.source_restriction_id(),
        other.source_restriction_id()
    );
    assert!(
        matches!(session.prepare(fresh, session.revision(), editor(7, &snapshot(owner), false), &[], None),
        Err(QuerySessionError::Document(error)) if matches!(*error, QueryDocumentError::OwnerChanged))
    );
    drop(other);
    assert!(
        session
            .prepare(
                fresh,
                session.revision(),
                editor(7, &snapshot(owner), false),
                &[],
                None
            )
            .is_err()
    );
}

/// Releasing a temporary source owner cannot remove a support policy prohibition acquired during its lifetime.
#[test]
fn source_restriction_release_preserves_current_support_policy() {
    let (directory, mut workspace, node, path) = setup();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let document = project.query_document(&node, &path, 7, None).unwrap();
    let selected = identity(&document);
    let mut session = QuerySession::open(
        project,
        document.clone(),
        selected.clone(),
        None,
        QueryOwnerFormat::Source,
        QueryOwnerRestrictions {
            user_read_only: false,
        },
    )
    .unwrap();
    let root = "11111111-1111-1111-1111-111111111111";
    fs::write(directory.0.join("src/Ext/ParentConfigurations.bin"),
        format!("{{6,0,1,{root},0,{root},\"1\",\"Vendor\",\"Test\",1,0,0,{root},{root},0,0,0,1,0,0,0,1,0,1,0,1,1,1,1}}"))
        .unwrap();
    let restriction = session.source_restriction_id();
    assert!(restriction.is_some());
    assert_eq!(session.close(), restriction);
    assert!(
        project
            .support_files(std::slice::from_ref(&path))
            .unwrap()
            .files[0]
            .read_only
    );
    assert!(matches!(
        project.plan_query(
            &document,
            revision(&document),
            &selected,
            "ВЫБРАТЬ 9",
            &[],
            None
        ),
        Err(QueryDocumentError::ReadOnly)
    ));
}

/// Another backend sees the same physical query owner as read-only until the owning session releases it.
#[test]
fn source_session_restriction_is_shared_with_an_independent_workspace() {
    use crate::backend::project::metadata_workspace::SourceAccessRequest;
    let (directory, mut workspace, node, path) = setup();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let document = project.query_document(&node, &path, 7, None).unwrap();
    let selected = identity(&document);
    let mut session = QuerySession::open(
        project,
        document,
        selected,
        None,
        QueryOwnerFormat::Source,
        QueryOwnerRestrictions::default(),
    )
    .unwrap();
    let mut independent = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = independent.project_mut(&ProjectScope::Standalone).unwrap();
    let requests = [SourceAccessRequest {
        path: path.clone(),
        user_read_only: false,
    }];
    let protection = project.source_protection(&requests, true).unwrap();
    assert!(protection[0].shared_read_only);
    assert!(!protection[0].writable());
    session.close();
    let protection = project.source_protection(&requests, true).unwrap();
    assert!(!protection[0].shared_read_only);
    assert!(protection[0].writable());
}

/// Draft formatting cannot borrow constructor ownership, stale native text or an obsolete support permission.
#[test]
fn draft_formatting_requires_current_owned_and_writable_session() {
    let (directory, mut workspace, node, path) = setup();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let document = project.query_document(&node, &path, 7, None).unwrap();
    let selected = identity(&document);
    let mut session = QuerySession::open(
        project,
        document,
        selected,
        None,
        QueryOwnerFormat::Source,
        QueryOwnerRestrictions::default(),
    )
    .unwrap();
    session
        .check_draft_formatting(project, session.revision(), session.draft(), false)
        .unwrap();
    session
        .set_draft(session.revision(), Arc::from("SELECT\n1 AS Value"))
        .unwrap();
    session
        .check_draft_formatting(project, session.revision(), "SELECT\r\n1 AS Value", false)
        .unwrap();
    assert!(matches!(
        session.check_draft_formatting(project, session.revision(), "different draft", false),
        Err(QuerySessionError::StaleDraft)
    ));
    assert!(matches!(
        session.check_draft_formatting(project, session.revision(), session.draft(), true),
        Err(QuerySessionError::UserReadOnly)
    ));
    let token = session.open_constructor(session.revision()).unwrap();
    assert!(matches!(
        session.check_draft_formatting(project, session.revision(), session.draft(), false),
        Err(QuerySessionError::ConstructorBusy)
    ));
    session.cancel_constructor(token).unwrap();
    let root = "11111111-1111-1111-1111-111111111111";
    fs::write(directory.0.join("src/Ext/ParentConfigurations.bin"),format!("{{6,0,1,{root},0,{root},\"1\",\"Vendor\",\"Test\",1,0,0,{root},{root},0,0,0,1,0,0,0,1,0,1,0,1,1,1,1}}")).unwrap();
    assert!(
        session
            .check_draft_formatting(project, session.revision(), session.draft(), false)
            .is_err()
    );
    assert_eq!(
        fs::read_to_string(directory.0.join("src").join(path)).unwrap(),
        INPUT
    );
}
