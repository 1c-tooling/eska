//! Standard fields use the same public editing workflow, byte patches and history as ordinary attributes.

use super::*;
use eska::project::metadata_edit::{EditableField, ScalarSchema};
use eska::project::metadata_model::PropertyKey;

const XS: &str = "http://www.w3.org/2001/XMLSchema";

/// Existing standard attributes include a system-managed field which must remain outside the editor.
fn standard_fixture() -> (TestDir, MetadataWorkspace, ObjectId, PathBuf, String) {
    let (directory, workspace, _, _, _) = selector_fixture();
    drop(workspace);
    let file = directory.0.join("src/Catalogs/Goods.xml");
    let input = fs::read_to_string(&file).unwrap().replace("<DefaultObjectForm/>", "<DefaultObjectForm/><CodeLength>3</CodeLength><CodeType>String</CodeType><DescriptionLength>5</DescriptionLength><Hierarchical>true</Hierarchical><StandardAttributes xmlns:r='http://v8.1c.ru/8.3/xcf/readable' xmlns:s='http://www.w3.org/2001/XMLSchema-instance' xmlns:x='http://www.w3.org/2001/XMLSchema'>\r\n<r:StandardAttribute name='Code'><r:FillValue s:nil='true'/></r:StandardAttribute>\r\n<r:StandardAttribute name='Description'><r:FillValue s:type='x:string'>Hello</r:FillValue></r:StandardAttribute>\r\n<r:StandardAttribute name='Parent'><r:FillValue s:nil='true'/><r:ChoiceForm/></r:StandardAttribute>\r\n<r:StandardAttribute name='Ref'><r:FillValue s:nil='true'/></r:StandardAttribute>\r\n</StandardAttributes>");
    fs::write(&file, &input).unwrap();
    let workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    (
        directory,
        workspace,
        object(MetadataKind::Catalog, "Goods", None),
        file,
        input,
    )
}

/// Occurrences select the existing standard record, without exposing its name attribute as editable XML.
fn field(fields: &[EditableField], occurrence: usize, name: &str) -> EditableField {
    fields
        .iter()
        .find(|field| {
            field.path.len() == 3
                && field.path[1].occurrence == occurrence
                && field.path.last().unwrap().key.name == name
        })
        .unwrap()
        .clone()
}

/// Setting a standard code touches only its annotation/content and history restores the exact original bytes.
#[test]
fn standard_value_autosave_and_undo_are_byte_minimal() {
    let (_directory, mut workspace, id, file, input) = standard_fixture();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let state = project.property_editing(&id).unwrap();
    assert!(
        !state
            .properties
            .fields
            .iter()
            .any(|field| field.path.len() == 3 && field.path[1].occurrence == 3)
    );
    let code = field(&state.properties.fields, 0, "FillValue");
    let change = PropertyChange::Value {
        key: Some(PropertyKey {
            namespace: Some(XS.into()),
            name: "string".into(),
        }),
        value: "ABC".into(),
    };
    project
        .update_property(&id, &state.properties.snapshot, &code.path, &change)
        .unwrap();
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        input.replacen(
            "<r:FillValue s:nil='true'/>",
            "<r:FillValue s:type=\"x:string\">ABC</r:FillValue>",
            1
        )
    );
    let state = project.property_editing(&id).unwrap();
    project
        .undo_property(&id, &state.properties.snapshot, true)
        .unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), input);
}

/// Changing the implicit code type requires explicitly clearing its incompatible saved value first.
#[test]
fn owner_type_changes_require_manual_clear_of_standard_values() {
    let (_directory, mut workspace, id, file, _) = standard_fixture();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let state = project.property_editing(&id).unwrap();
    let code = field(&state.properties.fields, 0, "FillValue");
    project
        .update_property(
            &id,
            &state.properties.snapshot,
            &code.path,
            &PropertyChange::Value {
                key: Some(PropertyKey {
                    namespace: Some(XS.into()),
                    name: "string".into(),
                }),
                value: "ABC".into(),
            },
        )
        .unwrap();
    let state = project.property_editing(&id).unwrap();
    let code_type = state
        .properties
        .fields
        .iter()
        .find(|field| field.path[0].key.name == "CodeType")
        .unwrap();
    let before = fs::read_to_string(&file).unwrap();
    let change = PropertyChange::Text {
        value: "Number".into(),
    };
    assert!(matches!(
        project.update_property(&id, &state.properties.snapshot, &code_type.path, &change),
        Err(PropertyEditError::Edit(EditError::IncompatibleProperty(_)))
    ));
    assert_eq!(fs::read_to_string(&file).unwrap(), before);
    project
        .update_property(
            &id,
            &state.properties.snapshot,
            &code.path,
            &PropertyChange::Value {
                key: None,
                value: String::new(),
            },
        )
        .unwrap();
    let cleared = project.property_editing(&id).unwrap();
    project
        .update_property(&id, &cleared.properties.snapshot, &code_type.path, &change)
        .unwrap();
}

/// Parent values and forms stay bound to the same metadata owner and are checked before changing hierarchy.
#[test]
fn standard_parent_selectors_validate_membership_and_type_dependencies() {
    let (_directory, mut workspace, id, file, _) = standard_fixture();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let state = project.property_editing(&id).unwrap();
    let parent = field(&state.properties.fields, 2, "FillValue");
    let ScalarSchema::Value { types, .. } = parent.schema else {
        panic!("value");
    };
    let key = types[0].key.clone();
    let choices = project
        .property_value_choices(&id, &parent.path, &key)
        .unwrap();
    assert!(
        choices
            .iter()
            .any(|choice| choice.value == "Catalog.Goods.EmptyRef")
    );
    assert!(
        project
            .update_property(
                &id,
                &state.properties.snapshot,
                &parent.path,
                &PropertyChange::Value {
                    key: Some(key.clone()),
                    value: "Catalog.Other.EmptyRef".into()
                }
            )
            .is_err()
    );
    let form = field(&state.properties.fields, 2, "ChoiceForm");
    let choices = project.property_reference_choices(&id, &form.path).unwrap();
    assert!(
        choices
            .iter()
            .any(|choice| choice.value == "Catalog.Goods.Form.Object")
    );
    assert!(!choices.iter().any(|choice| choice.value.contains("Other")));
    project
        .update_property(
            &id,
            &state.properties.snapshot,
            &form.path,
            &PropertyChange::Text {
                value: "Catalog.Goods.Form.Object".into(),
            },
        )
        .unwrap();
    let before = fs::read_to_string(&file).unwrap();
    let state = project.property_editing(&id).unwrap();
    let hierarchy = state
        .properties
        .fields
        .iter()
        .find(|field| field.path[0].key.name == "Hierarchical")
        .unwrap();
    assert!(matches!(
        project.update_property(
            &id,
            &state.properties.snapshot,
            &hierarchy.path,
            &PropertyChange::Text {
                value: "false".into()
            }
        ),
        Err(PropertyEditError::Edit(EditError::IncompatibleProperty(_)))
    ));
    assert_eq!(fs::read_to_string(file).unwrap(), before);
}
