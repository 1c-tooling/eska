//! Structural history round trips real source paths, exact bytes and per-object editing sequences.

use super::*;
use eska::project::{
    metadata_edit::{EditError, PropertyChange},
    metadata_workspace::{ProjectSession, PropertyEditError},
};

/// Use an unrestricted source with a Unicode payload, inline child and repeated BSL references.
fn history_fixture() -> (TestDir, MetadataWorkspace) {
    let directory = TestDir::new();
    let source = directory.0.join("src");
    fs::create_dir_all(source.join("Catalogs/Old/Ext/Empty")).unwrap();
    fs::create_dir_all(source.join("Catalogs/Old/Forms")).unwrap();
    fs::create_dir_all(source.join("Ext")).unwrap();
    fs::write(
        directory.0.join("eska.toml"),
        "[project]\ntype='configuration'\n",
    )
    .unwrap();
    fs::write(source.join("Ext/ParentConfigurations.bin"), "{6,0,0,0,0,0}").unwrap();
    descriptor(
        &source,
        "Configuration.xml",
        "Configuration",
        "Demo",
        "<Catalog>Old</Catalog><Catalog>Older</Catalog>",
    );
    descriptor(
        &source,
        "Catalogs/Old.xml",
        "Catalog",
        "Old",
        "<Attribute uuid='22222222-2222-2222-2222-222222222222'><Properties><Name>Field</Name><Comment>child</Comment></Properties></Attribute><Form>Part</Form>",
    );
    descriptor(&source, "Catalogs/Older.xml", "Catalog", "Older", "");
    descriptor(&source, "Catalogs/Old/Forms/Part.xml", "Form", "Part", "");
    fs::write(source.join("Catalogs/Old/Ext/picture.bin"), [255, 0, 23]).unwrap();
    fs::write(
        source.join("Ext/SessionModule.bsl"),
        "\u{feff}A = Catalogs.Old;\r\nB = Справочники.Old;\r\n",
    )
    .unwrap();
    let workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    (directory, workspace)
}

/// BOM, CRLF and compact formatting must survive both directions of structural history.
fn descriptor(source: &Path, path: &str, kind: &str, name: &str, children: &str) {
    fs::write(source.join(path), format!("\u{feff}<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><{kind} uuid='11111111-1111-1111-1111-111111111111'><Properties><Name>{name}</Name><Comment>before</Comment></Properties><ChildObjects>{children}</ChildObjects></{kind}></MetaDataObject>\r\n")).unwrap();
}

/// Drive the same scalar editor API used by CLI and IDE clients.
fn comment(project: &mut ProjectSession, id: &ObjectId, value: &str) {
    let state = project.property_editing(id).unwrap();
    let field = state
        .properties
        .fields
        .iter()
        .find(|field| field.path.len() == 1 && field.path[0].key.name == "Comment")
        .unwrap();
    project
        .update_property(
            id,
            &state.properties.snapshot,
            None,
            &field.path,
            &PropertyChange::Text {
                value: value.into(),
            },
        )
        .unwrap();
}

/// Always supply the currently displayed descriptor snapshot and consume the returned object identity.
fn replay(project: &mut ProjectSession, id: &ObjectId, undo: bool) -> ObjectId {
    let state = project.property_editing(id).unwrap();
    project
        .undo_property(id, &state.properties.snapshot, None, undo)
        .unwrap()
        .object_id
}

/// Root, descriptor and inline names share a single chronological history with scalar changes.
#[test]
fn rename_and_scalar_history_round_trip_every_identity_shape() {
    for id in [
        object(MetadataKind::Configuration, "Demo", None),
        object(MetadataKind::Catalog, "Old", None),
        object(
            MetadataKind::Attribute,
            "Field",
            Some(object(MetadataKind::Catalog, "Old", None)),
        ),
    ] {
        let (directory, mut workspace) = history_fixture();
        let source = directory.0.join("src");
        let initial = bytes(&source);
        let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
        comment(project, &id, "first & <value>");
        let before_rename = bytes(&source);
        let plan = project.preview_rename(&id, "НовоеДлинноеИмя").unwrap();
        assert!(plan.issues.is_empty(), "{:?}", plan.issues);
        let next = plan.new_object_id.clone();
        project
            .apply_rename(&id, &plan.new_name, &plan.snapshot, true)
            .unwrap();
        let renamed = bytes(&source);
        assert!(project.property_editing(&next).unwrap().undo.is_some());
        assert_eq!(
            project.preview_rename(&next, &plan.old_name).unwrap().uuid,
            plan.uuid
        );
        comment(project, &next, "last");
        let final_bytes = bytes(&source);
        assert_eq!(replay(project, &next, true), next);
        assert_eq!(bytes(&source), renamed);
        assert_eq!(replay(project, &next, true), id);
        assert_eq!(bytes(&source), before_rename);
        assert_eq!(replay(project, &id, true), id);
        assert_eq!(bytes(&source), initial);
        assert_eq!(replay(project, &id, false), id);
        assert_eq!(replay(project, &id, false), next);
        assert_eq!(replay(project, &next, false), next);
        assert_eq!(bytes(&source), final_bytes);
        assert!(project.property_editing(&next).unwrap().redo.is_none());
        assert!(source.join("Catalogs").is_dir());
    }
}

/// Migrating descendant histories must neither lose unrelated prefixes nor replay through new references.
#[test]
fn rename_history_rekeys_descendants_and_refuses_new_sources() {
    let (directory, mut workspace) = history_fixture();
    let source = directory.0.join("src");
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let id = object(MetadataKind::Catalog, "Old", None);
    let other = object(MetadataKind::Catalog, "Older", None);
    let child = object(MetadataKind::Form, "Part", Some(id.clone()));
    comment(project, &other, "unrelated");
    comment(project, &child, "child change");
    let plan = project.preview_rename(&id, "Новый").unwrap();
    project
        .apply_rename(&id, &plan.new_name, &plan.snapshot, true)
        .unwrap();
    let next = plan.new_object_id;
    let next_child = object(MetadataKind::Form, "Part", Some(next.clone()));
    assert!(
        project
            .property_editing(&next_child)
            .unwrap()
            .undo
            .is_some()
    );
    assert!(project.property_editing(&other).unwrap().undo.is_some());
    assert_eq!(replay(project, &next_child, true), next_child);
    let state = project.property_editing(&next).unwrap();
    assert!(matches!(
        project.undo_property(&next, &state.properties.snapshot, None, true),
        Err(PropertyEditError::Edit(EditError::Conflict))
    ));
    replay(project, &next_child, false);
    let added = source.join("new-reference.bsl");
    fs::write(&added, "A = Catalogs.Новый;").unwrap();
    let before = bytes(&source);
    assert!(matches!(
        project.undo_property(&next, &state.properties.snapshot, None, true),
        Err(PropertyEditError::Edit(EditError::Conflict))
    ));
    assert_eq!(bytes(&source), before);
    fs::remove_file(added).unwrap();
    assert_eq!(replay(project, &next, true), id);
    assert!(project.property_editing(&child).unwrap().undo.is_some());
    assert_eq!(replay(project, &child, true), child);
    assert_eq!(replay(project, &other, true), other);
}
