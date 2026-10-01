use super::*;

/// A separate payload allows testing a missed watcher event at the actual publication boundary.
fn fixture() -> (TestDir, ObjectId, PathBuf, PathBuf) {
    let (directory, workspace, _, _, _) = selector_fixture();
    drop(workspace);
    let file = directory.0.join("src/Catalogs/Goods.xml");
    let input = fs::read_to_string(&file).unwrap().replace("<DefaultObjectForm/>", "<DefaultObjectForm/><CodeType>String</CodeType><CodeLength>9</CodeLength><DescriptionLength>50</DescriptionLength>");
    fs::write(&file, format!("\u{feff}\r\n{input}\r\n")).unwrap();
    let predefined = directory.0.join("src/Catalogs/Goods/Ext/Predefined.xml");
    fs::create_dir_all(predefined.parent().unwrap()).unwrap();
    fs::write(&predefined, "<PredefinedData xmlns='http://v8.1c.ru/8.3/xcf/predef'><Item id='33333333-3333-3333-3333-333333333333'><Name>First</Name><Code>AA12</Code><Description>First item</Description></Item></PredefinedData>").unwrap();
    (
        directory,
        object(MetadataKind::Catalog, "Goods", None),
        file,
        predefined,
    )
}

/// Preview, apply and undo all read the live predefined file, preserving every rejected source byte.
#[test]
fn owner_lengths_revalidate_predefined_data_before_apply_and_undo() {
    let (directory, id, file, predefined) = fixture();
    let original = fs::read_to_string(&file).unwrap();
    let payload = fs::read_to_string(&predefined).unwrap();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let state = project.property_editing(&id).unwrap();
    for (name, value) in [
        ("CodeLength", "3"),
        ("DescriptionLength", "9"),
        ("CodeType", "Number"),
    ] {
        let field = state
            .properties
            .fields
            .iter()
            .find(|field| field.path[0].key.name == name)
            .unwrap();
        let change = PropertyChange::Text {
            value: value.into(),
        };
        for result in [
            project
                .preview_property(&id, &state.properties.snapshot, &field.path, &change)
                .map(|_| ()),
            project
                .update_property(&id, &state.properties.snapshot, &field.path, &change)
                .map(|_| ()),
        ] {
            assert!(
                matches!(result, Err(PropertyEditError::Edit(EditError::IncompatibleProperty(key))) if key.name == "Predefined")
            );
        }
        assert_eq!(fs::read_to_string(&file).unwrap(), original);
    }
    let field = state
        .properties
        .fields
        .iter()
        .find(|field| field.path[0].key.name == "CodeLength")
        .unwrap();
    let change = PropertyChange::Text { value: "4".into() };
    project
        .preview_property(&id, &state.properties.snapshot, &field.path, &change)
        .unwrap();
    fs::write(&predefined, payload.replace("AA12", "AA123")).unwrap();
    assert!(
        project
            .update_property(&id, &state.properties.snapshot, &field.path, &change)
            .is_err()
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), original);
    fs::write(&predefined, &payload).unwrap();
    let change = PropertyChange::Text { value: "12".into() };
    project
        .update_property(&id, &state.properties.snapshot, &field.path, &change)
        .unwrap();
    let saved = fs::read_to_string(&file).unwrap();
    assert_eq!(
        saved,
        original.replace("<CodeLength>9</CodeLength>", "<CodeLength>12</CodeLength>")
    );
    let state = project.property_editing(&id).unwrap();
    fs::write(&predefined, payload.replace("AA12", "ABCDEFGHIJ")).unwrap();
    assert!(
        project
            .undo_property(&id, &state.properties.snapshot, true)
            .is_err()
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), saved);
    fs::write(&predefined, &payload).unwrap();
    project
        .undo_property(&id, &state.properties.snapshot, true)
        .unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), original);
}
