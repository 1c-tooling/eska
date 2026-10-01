use super::*;
use eska::project::{
    metadata_edit::{FieldStep, ScalarSchema},
    metadata_model::PropertyKey,
};

/// Exercise the actual schema, planner and history instead of only the range lookup table.
#[test]
fn bounded_metadata_integers_preserve_bytes_and_reject_out_of_range_values() {
    for (kind, property, initial, min, max) in [
        (
            MetadataKind::ScheduledJob,
            "RestartCountOnFailure",
            0,
            0,
            1_000_000,
        ),
        (
            MetadataKind::ScheduledJob,
            "RestartIntervalOnFailure",
            5,
            0,
            1_000_000,
        ),
        (MetadataKind::TabularSection, "LineNumberLength", 5, 5, 9),
        (MetadataKind::Catalog, "LevelCount", 2, 2, 10),
        (MetadataKind::ChartOfAccounts, "OrderLength", 9, 0, 628),
        (
            MetadataKind::AccountingRegister,
            "PeriodAdjustmentLength",
            1,
            0,
            3,
        ),
    ] {
        let (directory, id, file, input) = fixture(kind, property, initial);
        let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
        let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
        let editing = project.property_editing(&id).unwrap();
        let field = editing
            .properties
            .fields
            .iter()
            .find(|field| field.path[0].key.name == property)
            .unwrap();
        assert!(
            matches!(field.schema, ScalarSchema::Integer { min: lo, max: hi } if lo == min && hi == max)
        );
        for value in [
            (min - 1).to_string(),
            (max + 1).to_string(),
            "1.5".into(),
            "1e3".into(),
        ] {
            assert!(
                project
                    .update_property(
                        &id,
                        &editing.properties.snapshot,
                        None,
                        &field.path,
                        &PropertyChange::Text { value }
                    )
                    .is_err()
            );
            assert_eq!(fs::read_to_string(&file).unwrap(), input);
        }
        let change = PropertyChange::Text {
            value: max.to_string(),
        };
        let plan = project
            .preview_property(
                &id,
                &editing.properties.snapshot,
                None,
                &field.path,
                &change,
            )
            .unwrap();
        assert_eq!(
            plan.output(),
            input.replace(
                &format!("<{property}>{initial}</{property}>"),
                &format!("<{property}>{max}</{property}>")
            )
        );
        project
            .update_property(
                &id,
                &editing.properties.snapshot,
                None,
                &field.path,
                &change,
            )
            .unwrap();
        let after = project.property_editing(&id).unwrap();
        project
            .undo_property(&id, &after.properties.snapshot, None, true)
            .unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), input);
    }
}

/// Top-level and inline owners share the publication tests while retaining their exact contexts.
fn fixture(
    kind: MetadataKind,
    property: &str,
    initial: i64,
) -> (TestDir, ObjectId, PathBuf, String) {
    let (directory, workspace, _, _) = editable_fixture("configuration");
    drop(workspace);
    let source = directory.0.join("src");
    let root = source.join("Configuration.xml");
    fs::write(
        &root,
        fs::read_to_string(&root).unwrap().replace(
            "<ChildObjects/>",
            "<ChildObjects><Catalog>Goods</Catalog><ScheduledJob>Job</ScheduledJob><ChartOfAccounts>Accounts</ChartOfAccounts><AccountingRegister>Ledger</AccountingRegister></ChildObjects>",
        ),
    )
    .unwrap();
    let (class, name, path, siblings) = match kind {
        MetadataKind::Catalog => (
            "Catalog",
            "Goods",
            "Catalogs/Goods.xml",
            "<Hierarchical>true</Hierarchical><HierarchyType>HierarchyFoldersAndItems</HierarchyType><LimitLevelCount>true</LimitLevelCount>",
        ),
        MetadataKind::ChartOfAccounts => (
            "ChartOfAccounts",
            "Accounts",
            "ChartsOfAccounts/Accounts.xml",
            "<CodeLength>9</CodeLength><AutoOrderByCode>false</AutoOrderByCode>",
        ),
        MetadataKind::AccountingRegister => (
            "AccountingRegister",
            "Ledger",
            "AccountingRegisters/Ledger.xml",
            "",
        ),
        _ => ("ScheduledJob", "Job", "ScheduledJobs/Job.xml", ""),
    };
    let id = if kind == MetadataKind::TabularSection {
        selector_file(
            &source,
            "Catalogs/Goods.xml",
            "Catalog",
            "Goods",
            "",
            &format!(
                "<TabularSection uuid=\"22222222-2222-2222-2222-222222222222\"><Properties><Name>Rows</Name><{property}>{initial}</{property}></Properties></TabularSection>"
            ),
        );
        object(
            kind,
            "Rows",
            Some(object(MetadataKind::Catalog, "Goods", None)),
        )
    } else {
        selector_file(
            &source,
            path,
            class,
            name,
            &format!("{siblings}<{property}>{initial}</{property}>"),
            "",
        );
        object(kind, name, None)
    };
    let file = source.join(if kind == MetadataKind::TabularSection {
        "Catalogs/Goods.xml"
    } else {
        path
    });
    let input = format!("\u{feff}\r\n{}\r\n", fs::read_to_string(&file).unwrap());
    fs::write(&file, &input).unwrap();
    (directory, id, file, input)
}

/// Schema and writer enforce hierarchy prerequisites even for a fabricated client field address.
#[test]
fn hierarchy_controls_follow_flags_and_reject_invalid_activation() {
    let (directory, id, file, original) = fixture(MetadataKind::Catalog, "LevelCount", 2);
    for (hierarchy, limit, reason) in [
        (false, true, "hierarchy_disabled"),
        (true, false, "level_limit_disabled"),
    ] {
        let input = original
            .replace("<Hierarchical>true", &format!("<Hierarchical>{hierarchy}"))
            .replace(
                "<LimitLevelCount>true",
                &format!("<LimitLevelCount>{limit}"),
            );
        fs::write(&file, &input).unwrap();
        let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
        let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
        let state = project.property_editing(&id).unwrap();
        assert!(
            state
                .properties
                .read_only_properties
                .iter()
                .any(|item| item.key.name == "LevelCount" && item.reason == reason)
        );
        assert!(
            !state
                .properties
                .fields
                .iter()
                .any(|field| field.path[0].key.name == "LevelCount")
        );
        let path = vec![FieldStep {
            key: PropertyKey {
                namespace: Some("http://v8.1c.ru/8.3/MDClasses".into()),
                name: "LevelCount".into(),
            },
            occurrence: 0,
        }];
        assert!(
            project
                .update_property(
                    &id,
                    &state.properties.snapshot,
                    None,
                    &path,
                    &PropertyChange::Text { value: "5".into() }
                )
                .is_err()
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), input);
    }
    let input = original
        .replace("<LimitLevelCount>true", "<LimitLevelCount>false")
        .replace("<LevelCount>2", "<LevelCount>11");
    fs::write(&file, &input).unwrap();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let state = project.property_editing(&id).unwrap();
    let field = state
        .properties
        .fields
        .iter()
        .find(|field| field.path[0].key.name == "LimitLevelCount")
        .unwrap();
    assert!(
        matches!(project.update_property(&id, &state.properties.snapshot, None, &field.path, &PropertyChange::Text { value: "true".into() }), Err(PropertyEditError::Edit(EditError::IncompatibleProperty(key))) if key.name == "LevelCount")
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), input);
}

/// Order changes cannot truncate a live predefined value or violate automatic ordering by code.
#[test]
fn order_length_checks_code_and_live_predefined_order_including_history() {
    let (directory, id, file, original) = fixture(MetadataKind::ChartOfAccounts, "OrderLength", 9);
    let auto = original.replace("<AutoOrderByCode>false", "<AutoOrderByCode>true");
    fs::write(&file, &auto).unwrap();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let state = project.property_editing(&id).unwrap();
    let field = state
        .properties
        .fields
        .iter()
        .find(|field| field.path[0].key.name == "OrderLength")
        .unwrap();
    assert!(matches!(
        field.schema,
        ScalarSchema::Integer { min: 9, max: 628 }
    ));
    assert!(
        project
            .update_property(
                &id,
                &state.properties.snapshot,
                None,
                &field.path,
                &PropertyChange::Text { value: "8".into() }
            )
            .is_err()
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), auto);
    drop(workspace);
    fs::write(&file, &original).unwrap();
    let predefined = directory
        .0
        .join("src/ChartsOfAccounts/Accounts/Ext/Predefined.xml");
    fs::create_dir_all(predefined.parent().unwrap()).unwrap();
    let payload = "<PredefinedData xmlns='http://v8.1c.ru/8.3/xcf/predef'><Item><Order>😀</Order><ChildItems><Item><Order> 12</Order></Item></ChildItems></Item></PredefinedData>";
    fs::write(&predefined, payload).unwrap();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let state = project.property_editing(&id).unwrap();
    for value in ["0", "1", "2"] {
        assert!(
            matches!(project.update_property(&id, &state.properties.snapshot, None, &field.path, &PropertyChange::Text { value: value.into() }), Err(PropertyEditError::Edit(EditError::IncompatibleProperty(key))) if key.name == "Predefined")
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), original);
    }
    project
        .update_property(
            &id,
            &state.properties.snapshot,
            None,
            &field.path,
            &PropertyChange::Text { value: "12".into() },
        )
        .unwrap();
    let updated = project.property_editing(&id).unwrap();
    let saved = fs::read_to_string(&file).unwrap();
    assert_eq!(saved, original.replace("<OrderLength>9", "<OrderLength>12"));
    fs::write(&predefined, payload.replace(" 12", "1234567890")).unwrap();
    assert!(
        project
            .undo_property(&id, &updated.properties.snapshot, None, true)
            .is_err()
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), saved);
    fs::write(&predefined, payload).unwrap();
    project
        .undo_property(&id, &updated.properties.snapshot, None, true)
        .unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), original);
}
