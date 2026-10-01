use super::*;
use eska::project::metadata_edit::{EditableField, ScalarSchema};

/// A real source preserves BOM, CRLF and unrelated values across an owner-domain change.
fn fixture(
    kind: MetadataKind,
    tag: &str,
    folder: &str,
    inherited: bool,
) -> (TestDir, ObjectId, PathBuf, String) {
    let (directory, workspace, _, _) = editable_fixture("configuration");
    drop(workspace);
    let source = directory.0.join("src");
    let root = source.join("Configuration.xml");
    fs::write(&root, fs::read_to_string(&root).unwrap().replace("<ChildObjects/>", &format!("<ChildObjects><{tag}>Probe</{tag}><DocumentNumerator>Shared</DocumentNumerator></ChildObjects>"))).unwrap();
    selector_file(
        &source,
        "DocumentNumerators/Shared.xml",
        "DocumentNumerator",
        "Shared",
        "<NumberLength>9</NumberLength><NumberType>String</NumberType>",
        "",
    );
    let properties = format!(
        "<NumberType>String</NumberType>\r\n<NumberLength>9</NumberLength><NumberAllowedLength>Variable</NumberAllowedLength><NumberPeriodicity>Year</NumberPeriodicity><Autonumbering>false</Autonumbering><CheckUnique>false</CheckUnique><InputByString/><Numerator>{}</Numerator>",
        if inherited {
            "DocumentNumerator.Shared"
        } else {
            ""
        }
    );
    let relative = format!("{folder}/Probe.xml");
    selector_file(&source, &relative, tag, "Probe", &properties, "");
    let file = source.join(relative);
    let input = format!("\u{feff}\r\n{}\r\n", fs::read_to_string(&file).unwrap());
    fs::write(&file, &input).unwrap();
    (directory, object(kind, "Probe", None), file, input)
}

/// Select only an explicitly published top-level editor.
fn field(fields: &[EditableField], name: &str) -> EditableField {
    fields
        .iter()
        .find(|field| field.path.len() == 1 && field.path[0].key.name == name)
        .unwrap()
        .clone()
}

/// Type changes narrow the number domain; rejected writes and undo preserve exact lexical bytes.
#[test]
fn number_length_uses_owner_type_and_roundtrips_through_history() {
    for (kind, tag, folder) in [
        (MetadataKind::Document, "Document", "Documents"),
        (MetadataKind::Task, "Task", "Tasks"),
        (
            MetadataKind::BusinessProcess,
            "BusinessProcess",
            "BusinessProcesses",
        ),
    ] {
        let (directory, id, file, original) = fixture(kind, tag, folder, false);
        let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
        let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
        let state = project.property_editing(&id).unwrap();
        let number = field(&state.properties.fields, "NumberLength");
        assert!(matches!(
            number.schema,
            ScalarSchema::Integer { min: 0, max: 50 }
        ));
        for value in ["-1", "51", "1.5", "1e1"] {
            assert!(
                project
                    .update_property(
                        &id,
                        &state.properties.snapshot,
                        &number.path,
                        &PropertyChange::Text {
                            value: value.into()
                        }
                    )
                    .is_err()
            );
            assert_eq!(fs::read_to_string(&file).unwrap(), original);
        }
        for (name, value) in [
            ("NumberLength", "50"),
            ("NumberLength", "38"),
            ("NumberType", "Number"),
            ("NumberLength", "0"),
        ] {
            let state = project.property_editing(&id).unwrap();
            let selected = field(&state.properties.fields, name);
            project
                .update_property(
                    &id,
                    &state.properties.snapshot,
                    &selected.path,
                    &PropertyChange::Text {
                        value: value.into(),
                    },
                )
                .unwrap();
            if value == "50" {
                let state = project.property_editing(&id).unwrap();
                let selected = field(&state.properties.fields, "NumberType");
                assert!(
                    matches!(project.update_property(&id, &state.properties.snapshot, &selected.path, &PropertyChange::Text { value: "Number".into() }), Err(PropertyEditError::Edit(EditError::IncompatibleProperty(key))) if key.name == "NumberLength")
                );
            }
            if name == "NumberType" {
                let state = project.property_editing(&id).unwrap();
                assert!(matches!(
                    field(&state.properties.fields, "NumberLength").schema,
                    ScalarSchema::Integer { min: 0, max: 38 }
                ));
            }
        }
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            original
                .replace(
                    "<NumberLength>9</NumberLength>",
                    "<NumberLength>0</NumberLength>"
                )
                .replace(
                    "<NumberType>String</NumberType>",
                    "<NumberType>Number</NumberType>"
                )
        );
        for _ in 0..4 {
            let state = project.property_editing(&id).unwrap();
            project
                .undo_property(&id, &state.properties.snapshot, true)
                .unwrap();
        }
        assert_eq!(fs::read_to_string(&file).unwrap(), original);
    }
}

/// Clearing the numerator restores direct editors; undo locks them again without rewriting their values.
#[test]
fn document_inherited_parameters_cannot_be_written_through_forged_paths() {
    let (directory, id, file, original) =
        fixture(MetadataKind::Document, "Document", "Documents", true);
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let state = project.property_editing(&id).unwrap();
    for name in [
        "NumberType",
        "NumberLength",
        "NumberAllowedLength",
        "NumberPeriodicity",
        "CheckUnique",
    ] {
        assert!(
            state
                .properties
                .read_only_properties
                .iter()
                .any(|item| item.key.name == name && item.reason == "numerator_inherited")
        );
        let mut path = field(&state.properties.fields, "Autonumbering").path;
        path[0].key.name = name.into();
        for result in [
            project
                .preview_property(
                    &id,
                    &state.properties.snapshot,
                    &path,
                    &PropertyChange::Text { value: "1".into() },
                )
                .map(|_| ()),
            project
                .update_property(
                    &id,
                    &state.properties.snapshot,
                    &path,
                    &PropertyChange::Text { value: "1".into() },
                )
                .map(|_| ()),
        ] {
            assert!(matches!(
                result,
                Err(PropertyEditError::Edit(EditError::UnsupportedValue))
            ));
        }
    }
    assert_eq!(fs::read_to_string(&file).unwrap(), original);
    let numerator = field(&state.properties.fields, "Numerator");
    project
        .update_property(
            &id,
            &state.properties.snapshot,
            &numerator.path,
            &PropertyChange::Text {
                value: String::new(),
            },
        )
        .unwrap();
    let state = project.property_editing(&id).unwrap();
    assert!(matches!(
        field(&state.properties.fields, "NumberLength").schema,
        ScalarSchema::Integer { min: 0, max: 50 }
    ));
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        original.replace("DocumentNumerator.Shared", "")
    );
    project
        .undo_property(&id, &state.properties.snapshot, true)
        .unwrap();
    let state = project.property_editing(&id).unwrap();
    assert!(
        !state
            .properties
            .fields
            .iter()
            .any(|field| field.path[0].key.name == "NumberLength")
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), original);
}
