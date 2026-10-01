use super::*;
use eska::project::metadata_edit::{EditableField, ScalarSchema, TextDomain};

/// Real inline attributes retain BOM, CRLF, comments and two existing link records.
fn fixture(standard: bool) -> (TestDir, MetadataWorkspace, ObjectId, PathBuf, String) {
    let (directory, workspace, _, _, _) = selector_fixture();
    drop(workspace);
    let file = directory.0.join("src/Catalogs/Goods.xml");
    let list = "<ChoiceParameterLinks xmlns:r='http://v8.1c.ru/8.3/xcf/readable'>\r\n<r:Link><r:Name>Отбор.Source</r:Name><r:DataPath>Catalog.Goods.Attribute.Source</r:DataPath><r:ValueChange>Clear</r:ValueChange></r:Link>\r\n<!-- retained --><r:Link><r:Name>Other</r:Name><r:DataPath>Catalog.Goods.StandardAttribute.Code</r:DataPath><r:ValueChange>DontChange</r:ValueChange></r:Link>\r\n</ChoiceParameterLinks>";
    let mut input = fs::read_to_string(&file).unwrap();
    let catalog = object(MetadataKind::Catalog, "Goods", None);
    let id = if standard {
        input = input.replace("<DefaultObjectForm/>", &format!("<DefaultObjectForm/><StandardAttributes xmlns:r='http://v8.1c.ru/8.3/xcf/readable'><r:StandardAttribute name='Parent'>{}</r:StandardAttribute></StandardAttributes>", list.replace("<ChoiceParameterLinks", "<r:ChoiceParameterLinks").replace("</ChoiceParameterLinks>", "</r:ChoiceParameterLinks>")));
        catalog
    } else {
        input = input.replace("</ChildObjects>", &format!("<Attribute uuid='22222222-2222-2222-2222-222222222222'><Properties><Name>Field</Name>{list}</Properties></Attribute></ChildObjects>"));
        object(MetadataKind::Attribute, "Field", Some(catalog))
    };
    let input = format!("\u{feff}{input}\r\n");
    fs::write(&file, &input).unwrap();
    let workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    (directory, workspace, id, file, input)
}

/// Repeated records are addressed by occurrence, independently of their editable names.
fn field(fields: &[EditableField], name: &str, occurrence: usize) -> EditableField {
    fields
        .iter()
        .find(|field| {
            field.path.last().unwrap().key.name == name
                && field
                    .path
                    .iter()
                    .any(|step| step.key.name == "Link" && step.occurrence == occurrence)
        })
        .unwrap()
        .clone()
}

/// Existing ordinary and standard links share exact preview, publication and byte-for-byte history.
#[test]
fn choice_link_name_and_mode_are_narrow_existing_scalar_edits() {
    for standard in [false, true] {
        let (_directory, mut workspace, id, file, original) = fixture(standard);
        let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
        let state = project.property_editing(&id).unwrap();
        assert!(
            !state
                .properties
                .fields
                .iter()
                .any(|field| field.path.len() == 1 && field.path[0].key.name == "Name")
        );
        let name = field(&state.properties.fields, "Name", 0);
        assert!(matches!(
            name.schema,
            ScalarSchema::Text {
                domain: Some(TextDomain::ChoiceParameterName)
            }
        ));
        assert_eq!(
            serde_json::to_value(&name.schema).unwrap(),
            serde_json::json!({"kind":"text","domain":"choiceParameterName"})
        );
        for (field, value, expected) in [
            (
                name,
                "Отбор.Получатель",
                original.replace(">Отбор.Source<", ">Отбор.Получатель<"),
            ),
            (
                field(&state.properties.fields, "ValueChange", 1),
                "Clear",
                original
                    .replace(">Отбор.Source<", ">Отбор.Получатель<")
                    .replace(">DontChange<", ">Clear<"),
            ),
        ] {
            let state = project.property_editing(&id).unwrap();
            let change = PropertyChange::Text {
                value: value.into(),
            };
            let plan = project
                .preview_property(&id, &state.properties.snapshot, None, &field.path, &change)
                .unwrap();
            assert_eq!(plan.output(), expected);
            assert_eq!(
                fs::read_to_string(&file).unwrap(),
                if value == "Clear" {
                    original.replace(">Отбор.Source<", ">Отбор.Получатель<")
                } else {
                    original.clone()
                }
            );
            project
                .update_property(&id, &state.properties.snapshot, None, &field.path, &change)
                .unwrap();
            assert_eq!(fs::read_to_string(&file).unwrap(), expected);
        }
        for _ in 0..2 {
            let state = project.property_editing(&id).unwrap();
            project
                .undo_property(&id, &state.properties.snapshot, None, true)
                .unwrap();
        }
        assert_eq!(fs::read_to_string(&file).unwrap(), original);
    }
}

/// Invalid syntax and case-insensitive list conflicts cannot pass either public planning entry point.
#[test]
fn choice_links_reject_conflicting_names_and_unknown_modes_without_writes() {
    let (_directory, mut workspace, id, file, original) = fixture(false);
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let state = project.property_editing(&id).unwrap();
    let name = field(&state.properties.fields, "Name", 0);
    for value in [
        "",
        ".A",
        "A.",
        "A..B",
        "A.B.C",
        " A",
        "A B",
        "A-B",
        "A²",
        "A\u{0345}",
        "𐐀",
        "other",
        "OTHER",
        "Other.Child",
    ] {
        let change = PropertyChange::Text {
            value: value.into(),
        };
        assert!(
            project
                .preview_property(&id, &state.properties.snapshot, None, &name.path, &change)
                .is_err(),
            "{value}"
        );
        assert!(
            project
                .update_property(&id, &state.properties.snapshot, None, &name.path, &change)
                .is_err(),
            "{value}"
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), original);
    }
    let other = field(&state.properties.fields, "Name", 1);
    for value in ["ОТБОР", "отбор.source"] {
        assert!(
            project
                .update_property(
                    &id,
                    &state.properties.snapshot,
                    None,
                    &other.path,
                    &PropertyChange::Text {
                        value: value.into()
                    }
                )
                .is_err()
        );
    }
    for value in ["1Name", "Отбор.Другое", "_._", "Имя١"] {
        assert!(
            project
                .preview_property(
                    &id,
                    &state.properties.snapshot,
                    None,
                    &name.path,
                    &PropertyChange::Text {
                        value: value.into()
                    }
                )
                .is_ok(),
            "{value}"
        );
    }
    let mode = field(&state.properties.fields, "ValueChange", 0);
    assert!(
        project
            .update_property(
                &id,
                &state.properties.snapshot,
                None,
                &mode.path,
                &PropertyChange::Text {
                    value: "Preserve".into()
                }
            )
            .is_err()
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), original);
}

/// Unknown writer shapes cannot gain a writable Name merely through a matching local XML tag.
#[test]
fn choice_link_wrapper_requires_exact_namespace_and_record_fields() {
    for replacement in [
        "<r:Link xmlns:r='urn:foreign'>",
        "<r:Link><r:Unknown/>",
        "<r:Link><r:Name>Extra</r:Name>",
    ] {
        let (directory, workspace, id, file, original) = fixture(false);
        drop(workspace);
        fs::write(&file, original.replace("<r:Link>", replacement)).unwrap();
        let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
        let state = workspace
            .project_mut(&ProjectScope::Standalone)
            .unwrap()
            .property_editing(&id)
            .unwrap();
        assert!(state.properties.fields.is_empty());
    }
}
