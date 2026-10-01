use super::*;
use eska::project::metadata_edit::{EditableField, ScalarSchema, ValueDomain};
use eska::project::metadata_model::PropertyKey;

const XS: &str = "http://www.w3.org/2001/XMLSchema";
const CFG: &str = "http://v8.1c.ru/8.1/data/enterprise/current-config";

/// Existing scalar, array and unset parameters exercise both ordinary and standard attributes.
fn fixture(standard: bool) -> (TestDir, MetadataWorkspace, ObjectId, PathBuf, String) {
    let (directory, workspace, _, _, _) = selector_fixture();
    drop(workspace);
    let file = directory.0.join("src/Catalogs/Goods.xml");
    let list = "<ChoiceParameters xmlns:a='http://v8.1c.ru/8.2/managed-application/core' xmlns:v='http://v8.1c.ru/8.1/data/core' xmlns:x='http://www.w3.org/2001/XMLSchema' xmlns:s='http://www.w3.org/2001/XMLSchema-instance'>\r\n<a:item name='Отбор.First'><a:value s:type='x:boolean'>true</a:value></a:item>\r\n<!--keep--><a:item name='List'><a:value s:type='v:FixedArray'><v:Value s:type='x:string'>one</v:Value><v:Value s:type='x:decimal'>2.5</v:Value></a:value></a:item><a:item name='Unset'><a:value s:nil='true'/></a:item></ChoiceParameters>";
    let mut input = fs::read_to_string(&file).unwrap();
    let catalog = object(MetadataKind::Catalog, "Goods", None);
    let id = if standard {
        input = input.replace("<DefaultObjectForm/>", &format!("<DefaultObjectForm/><StandardAttributes xmlns:r='http://v8.1c.ru/8.3/xcf/readable'><r:StandardAttribute name='Parent'>{}</r:StandardAttribute></StandardAttributes>", list.replace("<ChoiceParameters", "<r:ChoiceParameters").replace("</ChoiceParameters>", "</r:ChoiceParameters>")));
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

/// Select by current scalar content only within this controlled fixture, never in the production API.
fn field(fields: &[EditableField], value: &str) -> EditableField {
    fields
        .iter()
        .find(|field| field.value == value)
        .unwrap()
        .clone()
}

/// Typed changes carry semantic identities instead of client-generated XML annotations.
fn value(name: &str, value: &str) -> PropertyChange {
    PropertyChange::Value {
        key: Some(PropertyKey {
            namespace: Some(XS.into()),
            name: name.into(),
        }),
        value: value.into(),
    }
}

/// Both writer contexts preserve byte layout, collection cardinality and exact undo across several edits.
#[test]
fn choice_parameters_edit_names_scalar_and_array_values_with_exact_history() {
    for standard in [false, true] {
        let (_directory, mut workspace, id, file, original) = fixture(standard);
        let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
        let state = project.property_editing(&id).unwrap();
        let fields = &state.properties.fields;
        let parameter_fields = fields
            .iter()
            .filter(|field| {
                field
                    .path
                    .iter()
                    .any(|step| step.key.name == "ChoiceParameters")
            })
            .count();
        assert_eq!(parameter_fields, 7);
        assert!(matches!(
            field(fields, "2.5").schema,
            ScalarSchema::Value {
                domain: Some(ValueDomain::ChoiceParameter),
                ..
            }
        ));
        let mut expected = original.clone();
        for (field, change, before, after) in [
            (
                field(fields, "Отбор.First"),
                PropertyChange::Text {
                    value: "Отбор.Second".into(),
                },
                "name='Отбор.First'",
                "name='Отбор.Second'",
            ),
            (
                field(fields, "true"),
                value("boolean", "false"),
                ">true<",
                ">false<",
            ),
            (
                field(fields, "one"),
                value("string", "<&>"),
                ">one<",
                ">&lt;&amp;&gt;<",
            ),
            (
                field(fields, "2.5"),
                value("decimal", "123456789012345678901234567890123456789.125"),
                ">2.5<",
                ">123456789012345678901234567890123456789.125<",
            ),
        ] {
            let state = project.property_editing(&id).unwrap();
            expected = expected.replace(before, after);
            let preview = project
                .preview_property(&id, &state.properties.snapshot, None, &field.path, &change)
                .unwrap();
            assert_eq!(preview.output(), expected);
            project
                .update_property(&id, &state.properties.snapshot, None, &field.path, &change)
                .unwrap();
            assert_eq!(fs::read_to_string(&file).unwrap(), expected);
        }
        for _ in 0..4 {
            let state = project.property_editing(&id).unwrap();
            project
                .undo_property(&id, &state.properties.snapshot, None, true)
                .unwrap();
        }
        assert_eq!(fs::read_to_string(&file).unwrap(), original);
    }
}

/// Invalid names, values and attempts to overwrite containers never reach disk.
#[test]
fn choice_parameters_reject_duplicate_names_invalid_values_and_structural_changes() {
    let (_directory, mut workspace, id, file, original) = fixture(false);
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let state = project.property_editing(&id).unwrap();
    let name = field(&state.properties.fields, "Отбор.First");
    for invalid in ["", "list", "LIST", "A.B.C", "A\"", "A&B", "A B"] {
        let change = PropertyChange::Text {
            value: invalid.into(),
        };
        assert!(
            project
                .update_property(&id, &state.properties.snapshot, None, &name.path, &change)
                .is_err()
        );
    }
    // Ancestor names are distinct parameters, unlike conflicting choice-link names.
    project
        .preview_property(
            &id,
            &state.properties.snapshot,
            None,
            &name.path,
            &PropertyChange::Text {
                value: "List.Child".into(),
            },
        )
        .unwrap();
    let scalar = field(&state.properties.fields, "true");
    for change in [
        value("boolean", "yes"),
        value("decimal", "1e2"),
        value("decimal", "NaN"),
        value("dateTime", "2026-02-30T00:00:00"),
        value("unknown", "anything"),
        PropertyChange::Text {
            value: "false".into(),
        },
    ] {
        assert!(
            project
                .update_property(&id, &state.properties.snapshot, None, &scalar.path, &change)
                .is_err()
        );
    }
    let array = field(&state.properties.fields, "one");
    assert!(
        project
            .update_property(
                &id,
                &state.properties.snapshot,
                None,
                &array.path[..array.path.len() - 1],
                &value("string", "replace array")
            )
            .is_err()
    );
    assert_eq!(fs::read_to_string(file).unwrap(), original);
}

/// Project references are loaded lazily and validated even when they were absent from inline type hints.
#[test]
fn choice_parameters_lazily_select_reference_types_and_reject_fabricated_targets() {
    let (_directory, mut workspace, id, file, original) = fixture(false);
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let state = project.property_editing(&id).unwrap();
    let scalar = field(&state.properties.fields, "true");
    let types = project.property_type_choices(&id, &scalar.path).unwrap();
    assert_eq!(types.iter().filter(|t| t.object.is_none()).count(), 4);
    let key = PropertyKey {
        namespace: Some(CFG.into()),
        name: "CatalogRef.Other".into(),
    };
    assert!(types.iter().any(|t| t.key == key));
    let choices = project
        .property_value_choices(&id, &scalar.path, &key)
        .unwrap();
    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0].value, "Catalog.Other.EmptyRef");
    for (key, value) in [
        (key.clone(), "Catalog.Other.Missing"),
        (key.clone(), "Catalog.Goods.EmptyRef"),
        (
            PropertyKey {
                namespace: Some(CFG.into()),
                name: "CatalogRef.Missing".into(),
            },
            "Catalog.Missing.EmptyRef",
        ),
    ] {
        assert!(
            project
                .update_property(
                    &id,
                    &state.properties.snapshot,
                    None,
                    &scalar.path,
                    &PropertyChange::Value {
                        key: Some(key),
                        value: value.into()
                    }
                )
                .is_err()
        );
    }
    assert_eq!(fs::read_to_string(&file).unwrap(), original);
    project
        .update_property(
            &id,
            &state.properties.snapshot,
            None,
            &scalar.path,
            &PropertyChange::Value {
                key: Some(key.clone()),
                value: choices[0].value.clone(),
            },
        )
        .unwrap();
    let state = project.property_editing(&id).unwrap();
    assert!(
        matches!(&field(&state.properties.fields,"Catalog.Other.EmptyRef").schema,ScalarSchema::Value {key:Some(current),..} if *current==key)
    );
    // Nil means undefined, rather than deleting the existing parameter or array slot.
    project
        .update_property(
            &id,
            &state.properties.snapshot,
            None,
            &scalar.path,
            &PropertyChange::Value {
                key: None,
                value: String::new(),
            },
        )
        .unwrap();
    let state = project.property_editing(&id).unwrap();
    project
        .update_property(
            &id,
            &state.properties.snapshot,
            None,
            &scalar.path,
            &value("string", "new"),
        )
        .unwrap();
    assert!(fs::read_to_string(file).unwrap().contains(">new</a:value>"));
}
