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

/// Add real source-field structure and the actual xsi:string writer annotation.
fn sources_fixture(standard: bool) -> (TestDir, MetadataWorkspace, ObjectId, PathBuf, String) {
    let (directory, workspace, id, file, input) = fixture(standard);
    drop(workspace);
    let attribute = |name: &str| {
        format!(
            "<Attribute uuid='22222222-2222-2222-2222-222222222222'><Properties><Name>{name}</Name></Properties></Attribute>"
        )
    };
    let input = input.replace("<r:DataPath>", "<r:DataPath xmlns:s='http://www.w3.org/2001/XMLSchema-instance' xmlns:x='http://www.w3.org/2001/XMLSchema' s:type='x:string'>")
        .replace("<DefaultObjectForm/>", "<DefaultObjectForm/><Hierarchical>true</Hierarchical><HierarchyType>HierarchyFoldersAndItems</HierarchyType><CodeLength>9</CodeLength><CodeType>String</CodeType><DescriptionLength>50</DescriptionLength>")
        .replace("<Form>Object</Form>", &format!("<Form>Object</Form>{}{}<TabularSection uuid='33333333-3333-3333-3333-333333333333'><Properties><Name>Rows</Name></Properties><ChildObjects>{}</ChildObjects></TabularSection>", attribute("Source"), attribute("Alternate"), attribute("RowSource")));
    fs::write(&file, &input).unwrap();
    let workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    (directory, workspace, id, file, input)
}

/// Source choices use same-owner fields and enabled standards, excluding self and other selected sources.
#[test]
fn choice_link_sources_validate_scope_and_preserve_exact_bytes() {
    for standard in [false, true] {
        let (_directory, mut workspace, id, file, input) = sources_fixture(standard);
        let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
        let state = project.property_editing(&id).unwrap();
        let field = field(&state.properties.fields, "DataPath", 0);
        assert!(
            matches!(field.schema, ScalarSchema::Reference {ref domain, nullable:false, ..} if domain == "ChoiceParameterField")
        );
        let choices = project
            .property_reference_choices(&id, &field.path)
            .unwrap();
        let values: Vec<_> = choices.iter().map(|choice| choice.value.as_str()).collect();
        assert!(values.contains(&"Catalog.Goods.Attribute.Alternate"));
        assert!(values.contains(&"Catalog.Goods.StandardAttribute.Description"));
        assert!(!values.contains(&"Catalog.Goods.StandardAttribute.Code"));
        assert!(!values.contains(&"Catalog.Goods.TabularSection.Rows.Attribute.RowSource"));
        assert!(!values.contains(&"Catalog.Goods.Attribute.Field"));
        if standard {
            assert!(!values.contains(&"Catalog.Goods.StandardAttribute.Parent"));
        }
        for value in [
            "",
            "Catalog.Other.Attribute.Alternate",
            "Catalog.Goods.Attribute.Field",
            "Catalog.Goods.StandardAttribute.Code",
            "Catalog.Goods.StandardAttribute.Invented",
            "Catalog.Goods.TabularSection.Rows.Attribute.RowSource",
        ] {
            assert!(
                project
                    .update_property(
                        &id,
                        &state.properties.snapshot,
                        None,
                        &field.path,
                        &PropertyChange::Text {
                            value: value.into()
                        }
                    )
                    .is_err(),
                "{value}"
            );
            assert_eq!(fs::read_to_string(&file).unwrap(), input);
        }
        for value in [
            "Catalog.Goods.Attribute.Alternate",
            "Catalog.Goods.StandardAttribute.Description",
        ] {
            project
                .update_property(
                    &id,
                    &state.properties.snapshot,
                    None,
                    &field.path,
                    &PropertyChange::Text {
                        value: value.into(),
                    },
                )
                .unwrap();
            assert_eq!(
                fs::read_to_string(&file).unwrap(),
                input.replace(">Catalog.Goods.Attribute.Source<", &format!(">{value}<"))
            );
            let latest = project.property_editing(&id).unwrap();
            project
                .undo_property(&id, &latest.properties.snapshot, None, true)
                .unwrap();
            assert_eq!(fs::read_to_string(&file).unwrap(), input);
        }
    }
}

/// Stale session caches cannot advertise deleted sources or standard fields disabled by external changes.
#[test]
fn choice_link_sources_reread_external_changes_and_repair_legacy_paths() {
    let (_directory, mut workspace, id, file, input) = sources_fixture(false);
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let state = project.property_editing(&id).unwrap();
    let field = field(&state.properties.fields, "DataPath", 0);
    let changed = input
        .replace("<Name>Alternate</Name>", "<Name>Renamed</Name>")
        .replace("<DescriptionLength>50", "<DescriptionLength>0")
        .replace(
            ">Catalog.Goods.Attribute.Source<",
            ">0:77e522ce-5a4b-434e-87cf-683c89759d19<",
        );
    fs::write(&file, &changed).unwrap();
    let choices = project
        .property_reference_choices(&id, &field.path)
        .unwrap();
    assert!(
        choices
            .iter()
            .any(|choice| choice.value == "Catalog.Goods.Attribute.Renamed")
    );
    assert!(!choices.iter().any(
        |choice| choice.value.ends_with(".Alternate") || choice.value.ends_with(".Description")
    ));
    let change = PropertyChange::Text {
        value: "Catalog.Goods.Attribute.Renamed".into(),
    };
    assert!(
        project
            .update_property(&id, &state.properties.snapshot, None, &field.path, &change)
            .is_err()
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), changed);
    let latest = project.property_editing(&id).unwrap();
    project
        .update_property(&id, &latest.properties.snapshot, None, &field.path, &change)
        .unwrap();
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        changed.replace(
            ">0:77e522ce-5a4b-434e-87cf-683c89759d19<",
            ">Catalog.Goods.Attribute.Renamed<"
        )
    );
}

/// A tabular field can reference root fields and its own row, never another table or itself.
#[test]
fn choice_link_tabular_sources_include_only_the_current_row_and_owner() {
    let (directory, workspace, _, file, input) = sources_fixture(false);
    drop(workspace);
    let begin = input.find("<ChoiceParameterLinks").unwrap();
    let end = input[begin..].find("</ChoiceParameterLinks>").unwrap()
        + begin
        + "</ChoiceParameterLinks>".len();
    let links = &input[begin..end];
    let input = input.replace(
        "<Name>RowSource</Name>",
        &format!("<Name>RowSource</Name>{links}"),
    );
    fs::write(&file, &input).unwrap();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let catalog = object(MetadataKind::Catalog, "Goods", None);
    let table = object(MetadataKind::TabularSection, "Rows", Some(catalog));
    let id = object(MetadataKind::Attribute, "RowSource", Some(table));
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let state = project.property_editing(&id).unwrap();
    let field = field(&state.properties.fields, "DataPath", 0);
    let choices = project
        .property_reference_choices(&id, &field.path)
        .unwrap();
    for value in [
        "Catalog.Goods.Attribute.Field",
        "Catalog.Goods.Attribute.Alternate",
        "Catalog.Goods.StandardAttribute.Description",
        "Catalog.Goods.TabularSection.Rows.StandardAttribute.LineNumber",
    ] {
        assert!(
            choices.iter().any(|choice| choice.value == value),
            "{value}"
        );
        project
            .preview_property(
                &id,
                &state.properties.snapshot,
                None,
                &field.path,
                &PropertyChange::Text {
                    value: value.into(),
                },
            )
            .unwrap();
    }
    assert!(
        !choices
            .iter()
            .any(|choice| choice.value.ends_with(".Attribute.RowSource"))
    );
    let value = "Catalog.Goods.TabularSection.Rows.StandardAttribute.LineNumber";
    project
        .update_property(
            &id,
            &state.properties.snapshot,
            None,
            &field.path,
            &PropertyChange::Text {
                value: value.into(),
            },
        )
        .unwrap();
    let split = input.find("<Name>RowSource</Name>").unwrap();
    let expected = format!(
        "{}{}",
        &input[..split],
        input[split..].replacen(">Catalog.Goods.Attribute.Source<", &format!(">{value}<"), 1)
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), expected);
}

/// Annotation namespace and exact record shape remain part of the writer contract.
#[test]
fn choice_link_sources_reject_foreign_annotations() {
    for (old, new) in [
        ("s:type='x:string'", "s:type='x:boolean'"),
        (
            "xmlns:x='http://www.w3.org/2001/XMLSchema'",
            "xmlns:x='urn:foreign'",
        ),
        ("s:type='x:string'", "s:type='x:string' extra='value'"),
    ] {
        let (directory, workspace, id, file, input) = sources_fixture(false);
        drop(workspace);
        fs::write(&file, input.replace(old, new)).unwrap();
        let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
        let fields = workspace
            .project_mut(&ProjectScope::Standalone)
            .unwrap()
            .property_editing(&id)
            .unwrap()
            .properties
            .fields;
        assert!(
            !fields
                .iter()
                .any(|field| field.path.last().unwrap().key.name == "DataPath")
        );
    }
}

/// Constants select only other declared constants, preserving existing link cardinality.
#[test]
fn choice_link_constants_reject_undeclared_and_duplicate_sources() {
    let (directory, workspace, _, _, _) = selector_fixture();
    drop(workspace);
    let source = directory.0.join("src");
    let config = source.join("Configuration.xml");
    fs::write(&config, fs::read_to_string(&config).unwrap().replace("<ChildObjects>", "<ChildObjects><Constant>Current</Constant><Constant>One</Constant><Constant>Two</Constant><Constant>Three</Constant>")).unwrap();
    let links = "<ChoiceParameterLinks xmlns:r='http://v8.1c.ru/8.3/xcf/readable' xmlns:s='http://www.w3.org/2001/XMLSchema-instance' xmlns:x='http://www.w3.org/2001/XMLSchema'><r:Link><r:Name>First</r:Name><r:DataPath s:type='x:string'>Constant.One</r:DataPath><r:ValueChange>Clear</r:ValueChange></r:Link><r:Link><r:Name>Second</r:Name><r:DataPath s:type='x:string'>Constant.Two</r:DataPath><r:ValueChange>Clear</r:ValueChange></r:Link></ChoiceParameterLinks>";
    for name in ["Current", "One", "Two", "Three", "Undeclared"] {
        selector_file(
            &source,
            &format!("Constants/{name}.xml"),
            "Constant",
            name,
            links,
            "",
        );
    }
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let id = object(MetadataKind::Constant, "Current", None);
    let state = project.property_editing(&id).unwrap();
    let field = field(&state.properties.fields, "DataPath", 0);
    let choices = project
        .property_reference_choices(&id, &field.path)
        .unwrap();
    assert_eq!(
        choices
            .iter()
            .map(|choice| choice.value.as_str())
            .collect::<Vec<_>>(),
        ["Constant.One", "Constant.Three"]
    );
    let file = source.join("Constants/Current.xml");
    let original = fs::read_to_string(&file).unwrap();
    for value in [
        "Constant.Current",
        "Constant.Two",
        "Constant.Undeclared",
        "Catalog.Goods",
    ] {
        assert!(
            project
                .update_property(
                    &id,
                    &state.properties.snapshot,
                    None,
                    &field.path,
                    &PropertyChange::Text {
                        value: value.into()
                    }
                )
                .is_err()
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), original);
    }
    project
        .update_property(
            &id,
            &state.properties.snapshot,
            None,
            &field.path,
            &PropertyChange::Text {
                value: "Constant.Three".into(),
            },
        )
        .unwrap();
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        original.replace(">Constant.One<", ">Constant.Three<")
    );
    fs::write(
        &config,
        fs::read_to_string(&config)
            .unwrap()
            .replace("<Constant>One</Constant>", ""),
    )
    .unwrap();
    let choices = project
        .property_reference_choices(&id, &field.path)
        .unwrap();
    assert!(!choices.iter().any(|choice| choice.value == "Constant.One"));
    let state = project.property_editing(&id).unwrap();
    assert!(
        project
            .update_property(
                &id,
                &state.properties.snapshot,
                None,
                &field.path,
                &PropertyChange::Text {
                    value: "Constant.One".into()
                }
            )
            .is_err()
    );
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        original.replace(">Constant.One<", ">Constant.Three<")
    );
}

/// Disabling a source through owner properties requires manually changing dependent links first.
#[test]
fn choice_link_sources_prevent_disabling_referenced_standard_fields() {
    for (property, before, after, reference) in [
        ("CodeLength", "9", "0", "Code"),
        ("Hierarchical", "true", "false", "Parent"),
    ] {
        let (directory, workspace, _, file, input) = sources_fixture(false);
        drop(workspace);
        let input = input
            .replace(
                "<DefaultObjectForm/>",
                "<DefaultObjectForm/><InputByString/>",
            )
            .replace(
                ">Catalog.Goods.StandardAttribute.Code<",
                &format!(">Catalog.Goods.StandardAttribute.{reference}<"),
            );
        fs::write(&file, &input).unwrap();
        let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
        let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
        let id = object(MetadataKind::Catalog, "Goods", None);
        let state = project.property_editing(&id).unwrap();
        let field = state
            .properties
            .fields
            .iter()
            .find(|field| field.path.len() == 1 && field.path[0].key.name == property)
            .unwrap();
        assert_eq!(field.value, before);
        let result = project.update_property(
            &id,
            &state.properties.snapshot,
            None,
            &field.path,
            &PropertyChange::Text {
                value: after.into(),
            },
        );
        assert!(
            matches!(result, Err(PropertyEditError::Edit(EditError::IncompatibleProperty(ref key))) if key.name == "ChoiceParameterLinks"),
            "{result:?}"
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), input);
    }
}
