use super::*;
use eska::project::metadata_edit::{EditableField, ScalarSchema};

/// Both actual writer contexts start with an empty singular link, never a missing collection item.
fn fixture(standard: bool) -> (TestDir, MetadataWorkspace, ObjectId, PathBuf, String) {
    let (directory, workspace, _, _, _) = selector_fixture();
    drop(workspace);
    let file = directory.0.join("src/Catalogs/Goods.xml");
    let mut input = fs::read_to_string(&file).unwrap();
    let catalog = object(MetadataKind::Catalog, "Goods", None);
    input = input.replace(
        "<DefaultObjectForm/>",
        "<DefaultObjectForm/><InputByString/><CodeType>String</CodeType><CodeLength>9</CodeLength>",
    );
    input=input.replace("</ChildObjects>","<Attribute uuid='22222222-2222-2222-2222-222222222222'><Properties><Name>Source</Name></Properties></Attribute><Attribute uuid='33333333-3333-3333-3333-333333333333'><Properties><Name>Alternate</Name></Properties></Attribute></ChildObjects>");
    let id = if standard {
        input=input.replace("<DefaultObjectForm/>","<DefaultObjectForm/><StandardAttributes xmlns:r='http://v8.1c.ru/8.3/xcf/readable'><r:StandardAttribute name='Code'><r:LinkByType/></r:StandardAttribute></StandardAttributes>");
        catalog
    } else {
        input = input.replace(
            "<Name>Source</Name>",
            "<Name>Source</Name><LinkByType xmlns:r='http://v8.1c.ru/8.3/xcf/readable'/>",
        );
        object(MetadataKind::Attribute, "Source", Some(catalog))
    };
    let original = format!("\u{feff}{input}\r\n");
    fs::write(&file, &original).unwrap();
    let workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    (directory, workspace, id, file, original)
}

/// Wrapper and index addresses remain distinct before and after clearing a type link.
fn field(fields: &[EditableField], name: &str) -> EditableField {
    fields
        .iter()
        .find(|f| f.path.last().unwrap().key.name == name)
        .unwrap()
        .clone()
}

/// Setting, changing the index, changing the source and clearing all preserve exact reversible bytes.
#[test]
fn type_links_set_replace_clear_and_undo_both_writer_contexts() {
    for standard in [false, true] {
        let (_directory, mut workspace, id, file, original) = fixture(standard);
        let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
        let state = project.property_editing(&id).unwrap();
        let source = field(&state.properties.fields, "LinkByType");
        assert!(matches!(
            source.schema,
            ScalarSchema::Reference { nullable: true, .. }
        ));
        assert!(
            !state
                .properties
                .fields
                .iter()
                .any(|f| f.path.last().unwrap().key.name == "LinkItem")
        );
        let choices = project
            .property_reference_choices(&id, &source.path)
            .unwrap();
        assert!(
            choices
                .iter()
                .any(|c| c.value == "Catalog.Goods.Attribute.Alternate")
        );
        assert!(!choices.iter().any(|c| c.value
            == if standard {
                "Catalog.Goods.StandardAttribute.Code"
            } else {
                "Catalog.Goods.Attribute.Source"
            }));
        let mut history = vec![original.clone()];
        for (name, value) in [
            ("LinkByType", "Catalog.Goods.Attribute.Alternate"),
            ("LinkItem", "4294967295"),
            ("LinkByType", "Catalog.Goods.StandardAttribute.Ref"),
            ("LinkByType", ""),
        ] {
            let state = project.property_editing(&id).unwrap();
            let field = field(&state.properties.fields, name);
            let change = PropertyChange::Text {
                value: value.into(),
            };
            let preview = project
                .preview_property(&id, &state.properties.snapshot, None, &field.path, &change)
                .unwrap();
            project
                .update_property(&id, &state.properties.snapshot, None, &field.path, &change)
                .unwrap();
            let output = fs::read_to_string(&file).unwrap();
            assert_eq!(output, preview.output());
            assert!(output.starts_with('\u{feff}'));
            assert!(output.ends_with("\r\n"));
            if name == "LinkItem" {
                assert_eq!(
                    output,
                    history
                        .last()
                        .unwrap()
                        .replace(">0</r:LinkItem>", ">4294967295</r:LinkItem>")
                );
            }
            if value.ends_with("StandardAttribute.Ref") {
                assert_eq!(
                    output,
                    history.last().unwrap().replace(
                        "Catalog.Goods.Attribute.Alternate",
                        "Catalog.Goods.StandardAttribute.Ref"
                    )
                );
            }
            history.push(output);
        }
        for previous in history[..history.len() - 1].iter().rev() {
            let state = project.property_editing(&id).unwrap();
            project
                .undo_property(&id, &state.properties.snapshot, None, true)
                .unwrap();
            assert_eq!(fs::read_to_string(&file).unwrap(), *previous);
        }
        assert_eq!(fs::read_to_string(file).unwrap(), original);
    }
}

/// A singular type link accepts neither another owner nor an out-of-range unsigned index.
#[test]
fn type_links_reject_invalid_sources_indices_and_incompatible_standard_field_removal() {
    let (_directory, mut workspace, id, file, original) = fixture(false);
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let state = project.property_editing(&id).unwrap();
    let source = field(&state.properties.fields, "LinkByType");
    for value in [
        "Catalog.Goods.Attribute.Source",
        "Catalog.Other.StandardAttribute.Code",
        "Catalog.Goods.Attribute.Missing",
        "0:invented",
    ] {
        assert!(
            project
                .update_property(
                    &id,
                    &state.properties.snapshot,
                    None,
                    &source.path,
                    &PropertyChange::Text {
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
            &source.path,
            &PropertyChange::Text {
                value: "Catalog.Goods.StandardAttribute.Code".into(),
            },
        )
        .unwrap();
    let current = fs::read_to_string(&file).unwrap();
    let state = project.property_editing(&id).unwrap();
    let index = field(&state.properties.fields, "LinkItem");
    for value in ["-1", "4294967296", "1.5", "NaN"] {
        assert!(
            project
                .update_property(
                    &id,
                    &state.properties.snapshot,
                    None,
                    &index.path,
                    &PropertyChange::Text {
                        value: value.into()
                    }
                )
                .is_err()
        );
    }
    let catalog = object(MetadataKind::Catalog, "Goods", None);
    let state = project.property_editing(&catalog).unwrap();
    let length = field(&state.properties.fields, "CodeLength");
    let result = project.update_property(
        &catalog,
        &state.properties.snapshot,
        None,
        &length.path,
        &PropertyChange::Text { value: "0".into() },
    );
    assert!(
        matches!(result, Err(PropertyEditError::Edit(EditError::IncompatibleProperty(ref key))) if key.name == "LinkByType"),
        "{result:?}"
    );
    assert_eq!(fs::read_to_string(file).unwrap(), current);
}

/// Custom namespace prefixes, comments and surrounding whitespace survive the value replacement.
#[test]
fn type_links_preserve_wrapper_bytes_and_reject_unreviewed_shapes() {
    let (directory, workspace, id, file, original) = fixture(false);
    drop(workspace);
    let empty = "<LinkByType xmlns:r='http://v8.1c.ru/8.3/xcf/readable'/>";
    let known = "\r\n\t<LinkByType xmlns:DataPathPrefix='http://v8.1c.ru/8.3/xcf/readable'>\r\n\t<!-- retained -->\r\n\t</LinkByType>";
    let input = original.replace(empty, known);
    fs::write(&file, &input).unwrap();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let state = project.property_editing(&id).unwrap();
    let source = field(&state.properties.fields, "LinkByType");
    project
        .update_property(
            &id,
            &state.properties.snapshot,
            None,
            &source.path,
            &PropertyChange::Text {
                value: "Catalog.Goods.Attribute.Alternate".into(),
            },
        )
        .unwrap();
    let expected = input.replace("\r\n\t</LinkByType>", "\r\n\t\t<DataPathPrefix:DataPath>Catalog.Goods.Attribute.Alternate</DataPathPrefix:DataPath>\r\n\t\t<DataPathPrefix:LinkItem>0</DataPathPrefix:LinkItem>\r\n\t</LinkByType>");
    assert_eq!(fs::read_to_string(&file).unwrap(), expected);

    for wrapper in [
        "<LinkByType extra='unknown'/>",
        "<LinkByType><?unknown instruction?></LinkByType>",
        "<LinkByType><r:DataPath>Catalog.Goods.Attribute.Alternate</r:DataPath><r:LinkItem>-1</r:LinkItem></LinkByType>",
        "<LinkByType><r:DataPath>Catalog.Goods.Attribute.Alternate</r:DataPath><r:LinkItem>4294967296</r:LinkItem></LinkByType>",
        "<LinkByType><r:DataPath>Catalog.Goods.Attribute.Alternate</r:DataPath><r:LinkItem>0</r:LinkItem><r:Extra/></LinkByType>",
        "<LinkByType><r:LinkItem>0</r:LinkItem><r:DataPath>Catalog.Goods.Attribute.Alternate</r:DataPath></LinkByType>",
        "<LinkByType><r:DataPath xmlns:r='urn:foreign'>Catalog.Goods.Attribute.Alternate</r:DataPath><r:LinkItem>0</r:LinkItem></LinkByType>",
    ] {
        let wrapper = wrapper.replacen(
            "<LinkByType",
            "<LinkByType xmlns:r='http://v8.1c.ru/8.3/xcf/readable'",
            1,
        );
        fs::write(&file, original.replace(empty, &wrapper)).unwrap();
        let state = project.property_editing(&id).unwrap();
        assert!(
            !state
                .properties
                .fields
                .iter()
                .any(|field| field.path[0].key.name == "LinkByType"),
            "{wrapper}"
        );
    }
}

/// Constants share the source inventory but a source already used by a choice link remains available.
#[test]
fn type_link_constants_allow_shared_sources_and_recheck_fresh_descriptors() {
    let (directory, workspace, _, _, _) = fixture(false);
    drop(workspace);
    let configuration = directory.0.join("src/Configuration.xml");
    let input = fs::read_to_string(&configuration).unwrap().replace(
        "</ChildObjects>",
        "<Constant>First</Constant><Constant>Second</Constant></ChildObjects>",
    );
    fs::write(configuration, input).unwrap();
    let constants = directory.0.join("src/Constants");
    fs::create_dir(&constants).unwrap();
    for (name, uuid) in [
        ("First", "44444444-4444-4444-4444-444444444444"),
        ("Second", "55555555-5555-5555-5555-555555555555"),
    ] {
        fs::write(constants.join(format!("{name}.xml")),format!("<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses' xmlns:r='http://v8.1c.ru/8.3/xcf/readable'><Constant uuid='{uuid}'><Properties><Name>{name}</Name><LinkByType/><ChoiceParameterLinks><r:Link><r:Name>Filter.Source</r:Name><r:DataPath xmlns:s='http://www.w3.org/2001/XMLSchema-instance' xmlns:x='http://www.w3.org/2001/XMLSchema' s:type='x:string'>Constant.Second</r:DataPath><r:ValueChange>Clear</r:ValueChange></r:Link></ChoiceParameterLinks></Properties></Constant></MetaDataObject>")).unwrap();
    }
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let id = object(MetadataKind::Constant, "First", None);
    let state = project.property_editing(&id).unwrap();
    let source = field(&state.properties.fields, "LinkByType");
    let choices = project
        .property_reference_choices(&id, &source.path)
        .unwrap();
    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0].value, "Constant.Second");
    project
        .update_property(
            &id,
            &state.properties.snapshot,
            None,
            &source.path,
            &PropertyChange::Text {
                value: choices[0].value.clone(),
            },
        )
        .unwrap();
    let source_file = constants.join("Second.xml");
    fs::write(
        &source_file,
        fs::read_to_string(&source_file)
            .unwrap()
            .replace("<Name>Second</Name>", "<Name>Renamed</Name>"),
    )
    .unwrap();
    assert!(
        project
            .property_reference_choices(&id, &source.path)
            .is_err()
    );
}
