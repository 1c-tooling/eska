use super::*;
use eska::project::metadata_edit::ScalarSchema;

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

/// Give both an inline tabular section and a top-level job a real descriptor with lexical trivia.
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
            "<ChildObjects><Catalog>Goods</Catalog><ScheduledJob>Job</ScheduledJob></ChildObjects>",
        ),
    )
    .unwrap();
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
            "ScheduledJobs/Job.xml",
            "ScheduledJob",
            "Job",
            &format!("<{property}>{initial}</{property}>"),
            "",
        );
        object(kind, "Job", None)
    };
    let file = source.join(if kind == MetadataKind::TabularSection {
        "Catalogs/Goods.xml"
    } else {
        "ScheduledJobs/Job.xml"
    });
    let input = format!("\u{feff}\r\n{}\r\n", fs::read_to_string(&file).unwrap());
    fs::write(&file, &input).unwrap();
    (directory, id, file, input)
}
