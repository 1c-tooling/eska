use super::*;
use eska::project::metadata_edit::ScalarSchema;
use std::fmt::Write;

/// Charts and their referenced plans are real declared objects; payload edits remain separate.
fn fixture() -> (TestDir, ObjectId, PathBuf, PathBuf) {
    let (directory, workspace, _, _) = editable_fixture("configuration");
    drop(workspace);
    let source = directory.0.join("src");
    let root = source.join("Configuration.xml");
    fs::write(&root, fs::read_to_string(&root).unwrap().replace("<ChildObjects/>", "<ChildObjects><ChartOfAccounts>Accounts</ChartOfAccounts><ChartOfCharacteristicTypes>Types</ChartOfCharacteristicTypes><ChartOfCharacteristicTypes>Other</ChartOfCharacteristicTypes></ChildObjects>")).unwrap();
    for name in ["Types", "Other"] {
        selector_file(
            &source,
            &format!("ChartsOfCharacteristicTypes/{name}.xml"),
            "ChartOfCharacteristicTypes",
            name,
            "",
            "",
        );
    }
    selector_file(
        &source,
        "ChartsOfAccounts/Accounts.xml",
        "ChartOfAccounts",
        "Accounts",
        "<ExtDimensionTypes>ChartOfCharacteristicTypes.Types</ExtDimensionTypes><MaxExtDimensionCount>2</MaxExtDimensionCount>",
        "",
    );
    let file = source.join("ChartsOfAccounts/Accounts.xml");
    fs::write(
        &file,
        format!("\u{feff}\r\n{}\r\n", fs::read_to_string(&file).unwrap()),
    )
    .unwrap();
    let predefined = source.join("ChartsOfAccounts/Accounts/Ext/Predefined.xml");
    fs::create_dir_all(predefined.parent().unwrap()).unwrap();
    fs::write(&predefined, payload(2)).unwrap();
    (
        directory,
        object(MetadataKind::ChartOfAccounts, "Accounts", None),
        file,
        predefined,
    )
}

/// Nested predefined accounts participate in the same per-account limit.
fn payload(count: usize) -> String {
    let mut rows = String::new();
    for index in 0..count {
        write!(
            rows,
            "<ExtDimensionType name='ChartOfCharacteristicTypes.Types.Item{index}'/>"
        )
        .unwrap();
    }
    format!(
        "<PredefinedData xmlns='http://v8.1c.ru/8.3/xcf/predef'><Item><ChildItems><Item><ExtDimensionTypes>{rows}</ExtDimensionTypes></Item></ChildItems></Item></PredefinedData>"
    )
}

/// Range, existing rows and live payload changes are enforced on both writes and history.
#[test]
fn subaccount_count_and_plan_changes_preserve_existing_predefined_rows() {
    let (directory, id, file, predefined) = fixture();
    let original = fs::read_to_string(&file).unwrap();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let state = project.property_editing(&id).unwrap();
    let count = state
        .properties
        .fields
        .iter()
        .find(|field| field.path[0].key.name == "MaxExtDimensionCount")
        .unwrap();
    let plan = state
        .properties
        .fields
        .iter()
        .find(|field| field.path[0].key.name == "ExtDimensionTypes")
        .unwrap();
    assert!(matches!(
        count.schema,
        ScalarSchema::Integer { min: 0, max: 50 }
    ));
    for value in ["-1", "51", "1", "0"] {
        assert!(
            project
                .update_property(
                    &id,
                    &state.properties.snapshot,
                    None,
                    &count.path,
                    &PropertyChange::Text {
                        value: value.into()
                    }
                )
                .is_err()
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), original);
    }
    for (value, dependent) in [
        ("", "MaxExtDimensionCount"),
        ("ChartOfCharacteristicTypes.Other", "Predefined"),
    ] {
        assert!(
            matches!(project.update_property(&id, &state.properties.snapshot, None, &plan.path, &PropertyChange::Text { value: value.into() }), Err(PropertyEditError::Edit(EditError::IncompatibleProperty(key))) if key.name == dependent)
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), original);
    }
    project
        .update_property(
            &id,
            &state.properties.snapshot,
            None,
            &count.path,
            &PropertyChange::Text { value: "50".into() },
        )
        .unwrap();
    let after = project.property_editing(&id).unwrap();
    let saved = original.replace("<MaxExtDimensionCount>2", "<MaxExtDimensionCount>50");
    assert_eq!(fs::read_to_string(&file).unwrap(), saved);
    fs::write(&predefined, payload(3)).unwrap();
    assert!(
        project
            .undo_property(&id, &after.properties.snapshot, None, true)
            .is_err()
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), saved);
    fs::write(&predefined, payload(2)).unwrap();
    project
        .undo_property(&id, &after.properties.snapshot, None, true)
        .unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), original);
}

/// With no existing rows, clearing the count then the plan is an explicit pair of narrow edits.
#[test]
fn subaccount_count_requires_a_plan_and_cannot_be_enabled_by_fabricated_addresses() {
    let (directory, id, file, predefined) = fixture();
    fs::write(&predefined, payload(0)).unwrap();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let state = project.property_editing(&id).unwrap();
    let count = state
        .properties
        .fields
        .iter()
        .find(|field| field.path[0].key.name == "MaxExtDimensionCount")
        .unwrap();
    let plan = state
        .properties
        .fields
        .iter()
        .find(|field| field.path[0].key.name == "ExtDimensionTypes")
        .unwrap();
    project
        .update_property(
            &id,
            &state.properties.snapshot,
            None,
            &count.path,
            &PropertyChange::Text { value: "0".into() },
        )
        .unwrap();
    let zero = project.property_editing(&id).unwrap();
    project
        .update_property(
            &id,
            &zero.properties.snapshot,
            None,
            &plan.path,
            &PropertyChange::Text {
                value: String::new(),
            },
        )
        .unwrap();
    let disabled = project.property_editing(&id).unwrap();
    let saved = fs::read_to_string(&file).unwrap();
    assert!(
        disabled
            .properties
            .read_only_properties
            .iter()
            .any(|item| item.key.name == "MaxExtDimensionCount"
                && item.reason == "ext_dimension_types_missing")
    );
    assert!(
        !disabled
            .properties
            .fields
            .iter()
            .any(|field| field.path == count.path)
    );
    assert!(
        project
            .update_property(
                &id,
                &disabled.properties.snapshot,
                None,
                &count.path,
                &PropertyChange::Text { value: "1".into() }
            )
            .is_err()
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), saved);
    project
        .update_property(
            &id,
            &disabled.properties.snapshot,
            None,
            &plan.path,
            &PropertyChange::Text {
                value: "ChartOfCharacteristicTypes.Other".into(),
            },
        )
        .unwrap();
    let enabled = project.property_editing(&id).unwrap();
    assert!(
        enabled
            .properties
            .fields
            .iter()
            .any(|field| field.path == count.path)
    );
}
