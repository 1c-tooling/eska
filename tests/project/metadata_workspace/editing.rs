use super::*;
use eska::project::{
    metadata_edit::{EditError, PropertyChange},
    metadata_workspace::PropertyEditError,
};

/// Create small real descriptors whose references are declared by the owning XML, never by filenames alone.
fn selector_file(
    source: &Path,
    relative: &str,
    tag: &str,
    name: &str,
    properties: &str,
    children: &str,
) {
    let file = source.join(relative);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file,format!("<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><{tag} uuid=\"11111111-1111-1111-1111-111111111111\"><Properties><Name>{name}</Name>{properties}</Properties><ChildObjects>{children}</ChildObjects></{tag}></MetaDataObject>")).unwrap();
}

/// Keep repeated role entries and nullable form selectors in byte-sensitive source text.
fn selector_fixture() -> (TestDir, MetadataWorkspace, ObjectId, PathBuf, String) {
    let (directory, workspace, id, path) = editable_fixture("configuration");
    drop(workspace);
    let source = directory.0.join("src");
    let input = fs::read_to_string(source.join(&path)).unwrap()
        .replace("</Properties>", "<DefaultReportForm/><DefaultRoles xmlns:xr=\"http://v8.1c.ru/8.3/xcf/readable\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"><xr:Item xsi:type=\"xr:MDObjectRef\">Role.One</xr:Item><xr:Item xsi:type=\"xr:MDObjectRef\">Role.Two</xr:Item></DefaultRoles></Properties>")
        .replace("<ChildObjects/>", "<ChildObjects><Role>One</Role><Role>Two</Role><Role>Three</Role><CommonForm>Report</CommonForm><Catalog>Goods</Catalog><Catalog>Other</Catalog></ChildObjects>");
    fs::write(source.join(&path), &input).unwrap();
    for name in ["One", "Two", "Three"] {
        selector_file(&source, &format!("Roles/{name}.xml"), "Role", name, "", "");
    }
    selector_file(
        &source,
        "CommonForms/Report.xml",
        "CommonForm",
        "Report",
        "<FormType>Managed</FormType>",
        "",
    );
    for name in ["Goods", "Other"] {
        selector_file(
            &source,
            &format!("Catalogs/{name}.xml"),
            "Catalog",
            name,
            "<DefaultObjectForm/>",
            "<Form>Object</Form>",
        );
        selector_file(
            &source,
            &format!("Catalogs/{name}/Forms/Object.xml"),
            "Form",
            "Object",
            "<FormType>Managed</FormType>",
            "",
        );
    }
    let workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    (directory, workspace, id, path, input)
}

/// Swapping one role preserves other entries; duplicates, wrong kinds and undeclared names cannot write.
#[test]
fn role_selectors_preserve_list_membership_and_exact_undo() {
    let (directory, mut workspace, id, path, input) = selector_fixture();
    let source = directory.0.join("src");
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let editing = project.property_editing(&id).unwrap();
    let role = editing
        .properties
        .fields
        .iter()
        .find(|field| field.value == "Role.One")
        .unwrap();
    let choices = project.property_reference_choices(&id, &role.path).unwrap();
    assert_eq!(
        choices
            .iter()
            .map(|choice| choice.value.as_str())
            .collect::<Vec<_>>(),
        ["Role.One", "Role.Three"]
    );
    for invalid in ["Role.Two", "Role.Missing", "CommonForm.Report", ""] {
        assert!(
            project
                .update_property(
                    &id,
                    &editing.properties.snapshot,
                    &role.path,
                    &PropertyChange::Text {
                        value: invalid.into()
                    }
                )
                .is_err()
        );
        assert_eq!(fs::read_to_string(source.join(&path)).unwrap(), input);
    }
    let change = PropertyChange::Text {
        value: "Role.Three".into(),
    };
    let plan = project
        .preview_property(&id, &editing.properties.snapshot, &role.path, &change)
        .unwrap();
    assert_eq!(plan.output(), input.replace(">Role.One<", ">Role.Three<"));
    project
        .update_property(&id, &editing.properties.snapshot, &role.path, &change)
        .unwrap();
    let state = project.property_editing(&id).unwrap();
    project
        .undo_property(&id, &state.properties.snapshot, true)
        .unwrap();
    assert_eq!(fs::read_to_string(source.join(&path)).unwrap(), input);
}

/// Scalar form selectors accept empty values, while object forms are scoped to their owner.
#[test]
fn form_selectors_validate_nullable_values_and_owner_scope() {
    let (_directory, mut workspace, id, _path, _input) = selector_fixture();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let editing = project.property_editing(&id).unwrap();
    let form = editing
        .properties
        .fields
        .iter()
        .find(|field| field.path[0].key.name == "DefaultReportForm")
        .unwrap();
    project
        .update_property(
            &id,
            &editing.properties.snapshot,
            &form.path,
            &PropertyChange::Text {
                value: "CommonForm.Report".into(),
            },
        )
        .unwrap();
    let state = project.property_editing(&id).unwrap();
    project
        .update_property(
            &id,
            &state.properties.snapshot,
            &form.path,
            &PropertyChange::Text {
                value: String::new(),
            },
        )
        .unwrap();
    let catalog = object(MetadataKind::Catalog, "Goods", None);
    let state = project.property_editing(&catalog).unwrap();
    let form = state
        .properties
        .fields
        .iter()
        .find(|field| field.path[0].key.name == "DefaultObjectForm")
        .unwrap();
    let choices = project
        .property_reference_choices(&catalog, &form.path)
        .unwrap();
    assert_eq!(
        choices
            .iter()
            .map(|choice| choice.value.as_str())
            .collect::<Vec<_>>(),
        ["Catalog.Goods.Form.Object"]
    );
    assert!(
        project
            .update_property(
                &catalog,
                &state.properties.snapshot,
                &form.path,
                &PropertyChange::Text {
                    value: "Catalog.Other.Form.Object".into()
                }
            )
            .is_err()
    );
    project
        .update_property(
            &catalog,
            &state.properties.snapshot,
            &form.path,
            &PropertyChange::Text {
                value: "Catalog.Goods.Form.Object".into(),
            },
        )
        .unwrap();
}

/// An empty collection cannot be converted to a scalar or gain an entry through a selector.
#[test]
fn empty_role_collections_do_not_expose_a_scalar_editor() {
    let (directory, workspace, id, path, input) = selector_fixture();
    drop(workspace);
    let start = input.find("<DefaultRoles").unwrap();
    let end = input.find("</DefaultRoles>").unwrap() + "</DefaultRoles>".len();
    let input = format!("{}<DefaultRoles/>{}", &input[..start], &input[end..]);
    fs::write(directory.0.join("src").join(path), input).unwrap();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let editing = project.property_editing(&id).unwrap();
    assert!(
        !editing
            .properties
            .fields
            .iter()
            .any(|field| field.path[0].key.name == "DefaultRoles")
    );
}

/// An attribute's choice form belongs to its sole reference type, including when the owner differs.
#[test]
fn choice_forms_follow_the_field_type_without_offering_unrelated_forms() {
    let (directory, workspace, _, _, _) = selector_fixture();
    drop(workspace);
    let source = directory.0.join("src");
    let file = source.join("Catalogs/Goods.xml");
    let original = fs::read_to_string(&file).unwrap().replace("</ChildObjects>", "<Attribute uuid='33333333-3333-3333-3333-333333333333'><Properties><Name>Target</Name><Type xmlns:v='http://v8.1c.ru/8.1/data/core' xmlns:c='http://v8.1c.ru/8.1/data/enterprise/current-config'><v:Type>c:CatalogRef.Other</v:Type></Type><ChoiceForm/></Properties></Attribute></ChildObjects>");
    fs::write(&file, &original).unwrap();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let id = object(
        MetadataKind::Attribute,
        "Target",
        Some(object(MetadataKind::Catalog, "Goods", None)),
    );
    project.reveal_declared_object(&id).unwrap();
    let editing = project.property_editing(&id).unwrap();
    let field = editing
        .properties
        .fields
        .iter()
        .find(|field| field.path[0].key.name == "ChoiceForm")
        .unwrap();
    let choices = project
        .property_reference_choices(&id, &field.path)
        .unwrap();
    assert_eq!(
        choices
            .iter()
            .map(|choice| choice.value.as_str())
            .collect::<Vec<_>>(),
        ["Catalog.Other.Form.Object"]
    );
    assert!(
        project
            .update_property(
                &id,
                &editing.properties.snapshot,
                &field.path,
                &PropertyChange::Text {
                    value: "Catalog.Goods.Form.Object".into()
                }
            )
            .is_err()
    );
    project
        .update_property(
            &id,
            &editing.properties.snapshot,
            &field.path,
            &PropertyChange::Text {
                value: choices[0].value.clone(),
            },
        )
        .unwrap();
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        original.replace(
            "<ChoiceForm/>",
            "<ChoiceForm>Catalog.Other.Form.Object</ChoiceForm>"
        )
    );
    let compound = original.replace(
        "</v:Type>",
        "</v:Type><v:Type xmlns:xs='http://www.w3.org/2001/XMLSchema'>xs:boolean</v:Type>",
    );
    fs::write(&file, compound).unwrap();
    let editing = project.property_editing(&id).unwrap();
    let field = editing
        .properties
        .fields
        .iter()
        .find(|field| field.path[0].key.name == "ChoiceForm")
        .unwrap();
    assert!(
        project
            .property_reference_choices(&id, &field.path)
            .unwrap()
            .is_empty()
    );
}

/// External descriptors keep the external Designer reference prefix despite sharing logical kinds.
#[test]
fn external_objects_choose_their_own_default_forms() {
    for (case, tag) in [
        ("processing", "ExternalDataProcessor"),
        ("report", "ExternalReport"),
    ] {
        let (directory, workspace, id, path) = editable_fixture(case);
        drop(workspace);
        let source = directory.0.join("src");
        let file = source.join(path);
        let original = fs::read_to_string(&file)
            .unwrap()
            .replace("</Properties>", "<DefaultForm/></Properties>")
            .replace(
                "<ChildObjects/>",
                "<ChildObjects><Form>Main</Form></ChildObjects>",
            );
        fs::write(&file, &original).unwrap();
        selector_file(
            &source,
            "Demo/Forms/Main.xml",
            "Form",
            "Main",
            "<FormType>Managed</FormType>",
            "",
        );
        let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
        let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
        let editing = project.property_editing(&id).unwrap();
        let field = editing
            .properties
            .fields
            .iter()
            .find(|field| field.path[0].key.name == "DefaultForm")
            .unwrap();
        let choices = project
            .property_reference_choices(&id, &field.path)
            .unwrap();
        assert_eq!(choices[0].value, format!("{tag}.Demo.Form.Main"));
        project
            .update_property(
                &id,
                &editing.properties.snapshot,
                &field.path,
                &PropertyChange::Text {
                    value: choices[0].value.clone(),
                },
            )
            .unwrap();
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            original.replace(
                "<DefaultForm/>",
                &format!("<DefaultForm>{tag}.Demo.Form.Main</DefaultForm>")
            )
        );
    }
}

/// Numeric bounds are nullable values, including the specialized standard-attribute namespace.
#[test]
fn numeric_bounds_edit_existing_nil_elements_and_undo_the_exact_source() {
    let (directory, workspace, _, _, _) = selector_fixture();
    drop(workspace);
    let source = directory.0.join("src");
    let file = source.join("Catalogs/Goods.xml");
    let original = fs::read_to_string(&file).unwrap().replace("<DefaultObjectForm/>", "<DefaultObjectForm/><StandardAttributes xmlns:xr='http://v8.1c.ru/8.3/xcf/readable' xmlns:s='http://www.w3.org/2001/XMLSchema-instance'><xr:StandardAttribute name='Code'><xr:MinValue s:nil='1'/></xr:StandardAttribute></StandardAttributes>");
    fs::write(&file, &original).unwrap();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let id = object(MetadataKind::Catalog, "Goods", None);
    let editing = project.property_editing(&id).unwrap();
    let field = editing
        .properties
        .fields
        .iter()
        .find(|field| field.path.last().unwrap().key.name == "MinValue")
        .unwrap();
    for value in ["undefined", "<XML/>", "1.2.3", "1e30"] {
        assert!(
            project
                .update_property(
                    &id,
                    &editing.properties.snapshot,
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
            &editing.properties.snapshot,
            &field.path,
            &PropertyChange::Text {
                value: "12.5".into(),
            },
        )
        .unwrap();
    let changed = fs::read_to_string(&file).unwrap();
    assert!(changed.contains(">12.5</xr:MinValue>"));
    assert!(changed.contains("name='Code'"));
    let editing = project.property_editing(&id).unwrap();
    project
        .undo_property(&id, &editing.properties.snapshot, true)
        .unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), original);
}

/// All value tests start from an existing nil element, preserving its surrounding descriptor bytes.
fn filling_fixture(type_xml: &str) -> (TestDir, MetadataWorkspace, ObjectId, PathBuf, String) {
    let (directory, workspace, _, _, _) = selector_fixture();
    drop(workspace);
    let source = directory.0.join("src");
    selector_file(
        &source,
        "Catalogs/Goods.xml",
        "Catalog",
        "Goods",
        "",
        &format!(
            "<Attribute uuid='33333333-3333-3333-3333-333333333333'><Properties xmlns:v='http://v8.1c.ru/8.1/data/core' xmlns:xs='http://www.w3.org/2001/XMLSchema' xmlns:s='http://www.w3.org/2001/XMLSchema-instance' xmlns:c='http://v8.1c.ru/8.1/data/enterprise/current-config'><Name>Target</Name><Type>{type_xml}</Type><FillValue s:nil='true'/><Comment>keep</Comment></Properties></Attribute>"
        ),
    );
    let file = source.join("Catalogs/Goods.xml");
    let input = fs::read_to_string(&file).unwrap();
    let workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let id = object(
        MetadataKind::Attribute,
        "Target",
        Some(object(MetadataKind::Catalog, "Goods", None)),
    );
    (directory, workspace, id, file, input)
}

/// Nil is different from an empty string; qualifiers and the backend's declared types constrain writes.
#[test]
fn filling_values_enforce_primitive_types_and_preserve_exact_undo() {
    use eska::project::metadata_model::PropertyKey;
    for (description, kind, valid, invalid) in [
        (
            "<v:Type>xs:string</v:Type><v:StringQualifiers><v:Length>3</v:Length><v:AllowedLength>Variable</v:AllowedLength></v:StringQualifiers>",
            "string",
            "",
            "long",
        ),
        (
            "<v:Type>xs:decimal</v:Type><v:NumberQualifiers><v:Digits>5</v:Digits><v:FractionDigits>2</v:FractionDigits><v:AllowedSign>Nonnegative</v:AllowedSign></v:NumberQualifiers>",
            "decimal",
            "12.50",
            "-12.50",
        ),
        ("<v:Type>xs:boolean</v:Type>", "boolean", "true", "yes"),
        (
            "<v:Type>xs:dateTime</v:Type><v:DateQualifiers><v:DateFractions>Date</v:DateFractions></v:DateQualifiers>",
            "dateTime",
            "2024-02-29T00:00:00",
            "2023-02-29T00:00:00",
        ),
    ] {
        let (_directory, mut workspace, id, file, input) = filling_fixture(description);
        let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
        let editing = project.property_editing(&id).unwrap();
        let field = editing
            .properties
            .fields
            .iter()
            .find(|field| field.path[0].key.name == "FillValue")
            .unwrap();
        let key = Some(PropertyKey {
            namespace: Some("http://www.w3.org/2001/XMLSchema".into()),
            name: kind.into(),
        });
        assert!(
            project
                .update_property(
                    &id,
                    &editing.properties.snapshot,
                    &field.path,
                    &PropertyChange::Value {
                        key: key.clone(),
                        value: invalid.into()
                    }
                )
                .is_err()
        );
        assert!(
            project
                .update_property(
                    &id,
                    &editing.properties.snapshot,
                    &field.path,
                    &PropertyChange::Text {
                        value: valid.into()
                    }
                )
                .is_err()
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), input);
        project
            .update_property(
                &id,
                &editing.properties.snapshot,
                &field.path,
                &PropertyChange::Value {
                    key,
                    value: valid.into(),
                },
            )
            .unwrap();
        let replacement = if valid.is_empty() {
            format!("<FillValue s:type=\"xs:{kind}\"/>")
        } else {
            format!("<FillValue s:type=\"xs:{kind}\">{valid}</FillValue>")
        };
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            input.replace("<FillValue s:nil='true'/>", &replacement)
        );
        let state = project.property_editing(&id).unwrap();
        project
            .undo_property(&id, &state.properties.snapshot, true)
            .unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), input);
    }
}

/// Design-time selectors never accept arbitrary runtime data or a value belonging to another type.
#[test]
fn filling_reference_values_reject_invented_targets() {
    use eska::project::metadata_model::PropertyKey;
    let (_directory, mut workspace, id, file, input) =
        filling_fixture("<v:Type>c:CatalogRef.Other</v:Type>");
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let editing = project.property_editing(&id).unwrap();
    let field = editing
        .properties
        .fields
        .iter()
        .find(|field| field.path[0].key.name == "FillValue")
        .unwrap();
    let key = PropertyKey {
        namespace: Some("http://v8.1c.ru/8.1/data/enterprise/current-config".into()),
        name: "CatalogRef.Other".into(),
    };
    let choices = project
        .property_value_choices(&id, &field.path, &key)
        .unwrap();
    assert_eq!(
        choices
            .iter()
            .map(|choice| choice.value.as_str())
            .collect::<Vec<_>>(),
        ["Catalog.Other.EmptyRef"]
    );
    for value in [
        "Catalog.Goods.EmptyRef",
        "Catalog.Other.Unknown",
        "arbitrary",
    ] {
        assert!(
            project
                .update_property(
                    &id,
                    &editing.properties.snapshot,
                    &field.path,
                    &PropertyChange::Value {
                        key: Some(key.clone()),
                        value: value.into()
                    }
                )
                .is_err()
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), input);
    }
    project
        .update_property(
            &id,
            &editing.properties.snapshot,
            &field.path,
            &PropertyChange::Value {
                key: Some(key),
                value: choices[0].value.clone(),
            },
        )
        .unwrap();
    assert!(
        fs::read_to_string(&file)
            .unwrap()
            .contains(">Catalog.Other.EmptyRef</FillValue>")
    );
    let state = project.property_editing(&id).unwrap();
    project
        .update_property(
            &id,
            &state.properties.snapshot,
            &field.path,
            &PropertyChange::Value {
                key: None,
                value: String::new(),
            },
        )
        .unwrap();
    assert!(
        fs::read_to_string(&file)
            .unwrap()
            .contains("s:nil=\"true\"")
    );
}

/// Enum values and nested predefined records come from declared objects, never free-form strings.
#[test]
fn filling_reference_choices_include_enum_and_unique_predefined_records() {
    use eska::project::metadata_model::PropertyKey;
    let (directory, workspace, id, _, _) =
        filling_fixture("<v:Type>c:CatalogRef.Other</v:Type><v:Type>c:EnumRef.Status</v:Type>");
    drop(workspace);
    let source = directory.0.join("src");
    let root = source.join("Configuration.xml");
    fs::write(
        &root,
        fs::read_to_string(&root)
            .unwrap()
            .replace("</ChildObjects>", "<Enum>Status</Enum></ChildObjects>"),
    )
    .unwrap();
    selector_file(
        &source,
        "Enums/Status.xml",
        "Enum",
        "Status",
        "",
        "<EnumValue uuid='22222222-2222-2222-2222-222222222222'><Properties><Name>Active</Name></Properties></EnumValue>",
    );
    fs::create_dir_all(source.join("Catalogs/Other/Ext")).unwrap();
    fs::write(source.join("Catalogs/Other/Ext/Predefined.xml"), "<PredefinedData xmlns='http://v8.1c.ru/8.3/xcf/predef' version='2.20'><Item id='one'><Name>Root</Name><ChildItems><Item id='two'><Name>Nested</Name></Item><Item id='three'><Name>Duplicate</Name></Item></ChildItems></Item><Item id='four'><Name>Duplicate</Name></Item></PredefinedData>").unwrap();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let editing = project.property_editing(&id).unwrap();
    let field = editing
        .properties
        .fields
        .iter()
        .find(|field| field.path[0].key.name == "FillValue")
        .unwrap();
    for (name, expected) in [
        (
            "CatalogRef.Other",
            vec![
                "Catalog.Other.EmptyRef",
                "Catalog.Other.Nested",
                "Catalog.Other.Root",
            ],
        ),
        (
            "EnumRef.Status",
            vec!["Enum.Status.EmptyRef", "Enum.Status.EnumValue.Active"],
        ),
    ] {
        let key = PropertyKey {
            namespace: Some("http://v8.1c.ru/8.1/data/enterprise/current-config".into()),
            name: name.into(),
        };
        let choices = project
            .property_value_choices(&id, &field.path, &key)
            .unwrap();
        assert_eq!(
            choices
                .iter()
                .map(|choice| choice.value.as_str())
                .collect::<Vec<_>>(),
            expected
        );
        for choice in choices {
            let state = project.property_editing(&id).unwrap();
            project
                .update_property(
                    &id,
                    &state.properties.snapshot,
                    &field.path,
                    &PropertyChange::Value {
                        key: Some(key.clone()),
                        value: choice.value,
                    },
                )
                .unwrap();
        }
    }
}

/// A type or qualifier cannot invalidate an existing dependent value; clearing is a separate user edit.
#[test]
fn type_changes_require_incompatible_values_to_be_cleared_first() {
    use eska::project::metadata_model::PropertyKey;
    let (_directory, mut workspace, id, file, _) = filling_fixture(
        "<v:Type>xs:string</v:Type><v:StringQualifiers><v:Length>10</v:Length><v:AllowedLength>Variable</v:AllowedLength></v:StringQualifiers>",
    );
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let editing = project.property_editing(&id).unwrap();
    let default_value = editing
        .properties
        .fields
        .iter()
        .find(|field| field.path[0].key.name == "FillValue")
        .unwrap();
    project
        .update_property(
            &id,
            &editing.properties.snapshot,
            &default_value.path,
            &PropertyChange::Value {
                key: Some(PropertyKey {
                    namespace: Some("http://www.w3.org/2001/XMLSchema".into()),
                    name: "string".into(),
                }),
                value: "four".into(),
            },
        )
        .unwrap();
    let input = fs::read_to_string(&file).unwrap();
    let editing = project.property_editing(&id).unwrap();
    let type_field = editing
        .properties
        .fields
        .iter()
        .find(|field| {
            matches!(
                field.schema,
                eska::project::metadata_edit::ScalarSchema::DataType { .. }
            )
        })
        .unwrap();
    let length = editing
        .properties
        .fields
        .iter()
        .find(|field| field.path.last().unwrap().key.name == "Length")
        .unwrap();
    let new_type = PropertyChange::DataType {
        key: PropertyKey {
            namespace: Some("http://www.w3.org/2001/XMLSchema".into()),
            name: "boolean".into(),
        },
    };
    for (path, change) in [
        (&type_field.path, &new_type),
        (&length.path, &PropertyChange::Text { value: "3".into() }),
    ] {
        assert!(
            matches!(project.update_property(&id, &editing.properties.snapshot, path, change), Err(PropertyEditError::Edit(EditError::IncompatibleProperty(property))) if property.name == "FillValue")
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), input);
    }
    project
        .update_property(
            &id,
            &editing.properties.snapshot,
            &default_value.path,
            &PropertyChange::Value {
                key: None,
                value: String::new(),
            },
        )
        .unwrap();
    let editing = project.property_editing(&id).unwrap();
    project
        .update_property(
            &id,
            &editing.properties.snapshot,
            &type_field.path,
            &new_type,
        )
        .unwrap();
}

/// Choice forms belong to the selected reference type, including when only qualifiers change.
#[test]
fn type_changes_cannot_retarget_an_existing_choice_form() {
    use eska::project::metadata_model::PropertyKey;
    let (directory, workspace, id, file, input) =
        filling_fixture("<v:Type>c:CatalogRef.Other</v:Type>");
    drop(workspace);
    fs::write(
        &file,
        input.replace(
            "<Comment>keep</Comment>",
            "<Comment>keep</Comment><ChoiceForm>Catalog.Other.Form.Object</ChoiceForm>",
        ),
    )
    .unwrap();
    let input = fs::read_to_string(&file).unwrap();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let editing = project.property_editing(&id).unwrap();
    let field = editing
        .properties
        .fields
        .iter()
        .find(|field| {
            matches!(
                field.schema,
                eska::project::metadata_edit::ScalarSchema::DataType { .. }
            )
        })
        .unwrap();
    let change = PropertyChange::DataType {
        key: PropertyKey {
            namespace: Some("http://v8.1c.ru/8.1/data/enterprise/current-config".into()),
            name: "CatalogRef.Goods".into(),
        },
    };
    assert!(
        matches!(project.preview_property(&id, &editing.properties.snapshot, &field.path, &change), Err(PropertyEditError::Edit(EditError::IncompatibleProperty(property))) if property.name == "ChoiceForm")
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), input);
    let choice = editing
        .properties
        .fields
        .iter()
        .find(|field| field.path[0].key.name == "ChoiceForm")
        .unwrap();
    project
        .update_property(
            &id,
            &editing.properties.snapshot,
            &choice.path,
            &PropertyChange::Text {
                value: String::new(),
            },
        )
        .unwrap();
    let editing = project.property_editing(&id).unwrap();
    project
        .update_property(&id, &editing.properties.snapshot, &field.path, &change)
        .unwrap();
}

/// Preserve byte-sensitive formatting in fixtures for every supported project root.
fn editable_fixture(case: &str) -> (TestDir, MetadataWorkspace, ObjectId, PathBuf) {
    let directory = TestDir::new();
    let source = directory.0.join("src");
    fs::create_dir_all(source.join("Ext")).unwrap();
    fs::write(
        directory.0.join("eska.toml"),
        format!("[project]\ntype='{case}'\n"),
    )
    .unwrap();
    let (tag, kind, filename) = match case {
        "processing" => (
            "ExternalDataProcessor",
            MetadataKind::DataProcessor,
            "Demo.xml",
        ),
        "report" => ("ExternalReport", MetadataKind::Report, "Demo.xml"),
        _ => (
            "Configuration",
            MetadataKind::Configuration,
            "Configuration.xml",
        ),
    };
    let extension = if case == "extension" {
        "<ConfigurationExtensionPurpose>Customization</ConfigurationExtensionPurpose>"
    } else {
        ""
    };
    let input = format!(
        "\u{feff}<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" xmlns:v=\"http://v8.1c.ru/8.1/data/core\" version=\"2.20\">\r\n\t<{tag} uuid=\"11111111-1111-1111-1111-111111111111\">\r\n\t\t<Properties><Name>Demo</Name>{extension}\r\n\t\t\t<Synonym><v:item><v:lang>ru</v:lang><v:content>До</v:content></v:item></Synonym>\r\n\t\t\t<Comment><!--keep-->A&amp;B</Comment>\r\n\t\t</Properties>\r\n\t\t<ChildObjects/>\r\n\t</{tag}>\r\n</MetaDataObject>\r\n"
    );
    fs::write(source.join(filename), input).unwrap();
    fs::write(source.join("Ext/ParentConfigurations.bin"), "{6,0,0,0,0,0}").unwrap();
    let workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    (
        directory,
        workspace,
        object(kind, "Demo", None),
        filename.into(),
    )
}

#[test]
fn existing_values_edit_minimal_bytes_and_undo_exact_lexical_source_in_all_project_types() {
    for case in ["configuration", "extension", "processing", "report"] {
        let (directory, mut workspace, id, path) = editable_fixture(case);
        let original = fs::read(directory.0.join("src").join(&path)).unwrap();
        let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
        let editing = project.property_editing(&id).unwrap();
        assert!(editing.writable, "{case}");
        assert!(
            !editing
                .properties
                .fields
                .iter()
                .any(|field| field.path[0].key.name == "Name")
        );
        let field = editing
            .properties
            .fields
            .iter()
            .find(|field| field.path[0].key.name == "Comment")
            .unwrap();
        project
            .update_property(
                &id,
                &editing.properties.snapshot,
                &field.path,
                &PropertyChange::Text {
                    value: "Новый <текст>".into(),
                },
            )
            .unwrap();
        let expected = String::from_utf8(original.clone())
            .unwrap()
            .replace("A&amp;B", "Новый &lt;текст&gt;");
        assert_eq!(
            fs::read(directory.0.join("src").join(&path)).unwrap(),
            expected.as_bytes()
        );
        let editing = project.property_editing(&id).unwrap();
        assert!(editing.undo.is_some());
        let generation = project.generation();
        project
            .update_property(
                &id,
                &editing.properties.snapshot,
                &field.path,
                &PropertyChange::Text {
                    value: "Новый <текст>".into(),
                },
            )
            .unwrap();
        assert_eq!(generation, project.generation());
        project
            .undo_property(&id, &editing.properties.snapshot, true)
            .unwrap();
        assert_eq!(
            fs::read(directory.0.join("src").join(&path)).unwrap(),
            original
        );
        let editing = project.property_editing(&id).unwrap();
        assert!(editing.redo.is_some());
        project
            .undo_property(&id, &editing.properties.snapshot, false)
            .unwrap();
        assert_eq!(
            fs::read(directory.0.join("src").join(&path)).unwrap(),
            expected.as_bytes()
        );
        let editing = project.property_editing(&id).unwrap();
        let synonym = editing
            .properties
            .fields
            .iter()
            .find(|field| field.language.as_deref() == Some("ru"))
            .unwrap();
        project
            .update_property(
                &id,
                &editing.properties.snapshot,
                &synonym.path,
                &PropertyChange::Text {
                    value: "После".into(),
                },
            )
            .unwrap();
        assert_eq!(
            fs::read_to_string(directory.0.join("src").join(&path)).unwrap(),
            expected.replace(">До<", ">После<")
        );
    }
}

#[test]
fn external_changes_and_missing_support_rules_reject_writes_without_losing_source() {
    let (directory, mut workspace, id, path) = editable_fixture("configuration");
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let editing = project.property_editing(&id).unwrap();
    let field = editing
        .properties
        .fields
        .iter()
        .find(|field| field.path[0].key.name == "Comment")
        .unwrap();
    let file = directory.0.join("src").join(path);
    let original = fs::read_to_string(&file).unwrap();
    let external = original.replace("A&amp;B", "external");
    fs::write(&file, &external).unwrap();
    assert!(matches!(
        project.update_property(
            &id,
            &editing.properties.snapshot,
            &field.path,
            &PropertyChange::Text {
                value: "mine".into()
            }
        ),
        Err(PropertyEditError::Edit(EditError::Conflict))
    ));
    assert_eq!(fs::read_to_string(&file).unwrap(), external);
    let editing = project.property_editing(&id).unwrap();
    fs::remove_file(directory.0.join("src/Ext/ParentConfigurations.bin")).unwrap();
    assert!(matches!(
        project.update_property(
            &id,
            &editing.properties.snapshot,
            &field.path,
            &PropertyChange::Text {
                value: "mine".into()
            }
        ),
        Err(PropertyEditError::Edit(EditError::ReadOnly))
    ));
    assert_eq!(fs::read_to_string(&file).unwrap(), external);
    assert!(!fs::read_dir(file.parent().unwrap()).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".eska-properties-")
    }));
}

#[test]
fn existing_attribute_type_uses_declared_choices_and_validates_qualifier_dependencies() {
    use eska::project::{metadata_edit::ScalarSchema, metadata_model::PropertyKey};
    let (directory, _, _, root_path) = editable_fixture("configuration");
    let source = directory.0.join("src");
    let root = source.join(root_path);
    let original = fs::read_to_string(&root).unwrap().replace(
        "<ChildObjects/>",
        "<ChildObjects><Catalog>Items</Catalog></ChildObjects>",
    );
    fs::write(root, original).unwrap();
    fs::create_dir(source.join("Catalogs")).unwrap();
    let path = source.join("Catalogs/Items.xml");
    let xml = "\u{feff}<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses' xmlns:v='http://v8.1c.ru/8.1/data/core' xmlns:xs='http://www.w3.org/2001/XMLSchema'><Catalog uuid='22222222-2222-2222-2222-222222222222'><Properties><Name>Items</Name></Properties><ChildObjects><Attribute uuid='33333333-3333-3333-3333-333333333333'><Properties><Name>Amount</Name><Type><v:Type>xs:decimal</v:Type><v:NumberQualifiers><v:Digits>12</v:Digits><v:FractionDigits>2</v:FractionDigits><v:AllowedSign>Any</v:AllowedSign></v:NumberQualifiers></Type></Properties></Attribute></ChildObjects></Catalog></MetaDataObject>\r\n";
    fs::write(&path, xml).unwrap();
    let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let catalog = object(MetadataKind::Catalog, "Items", None);
    let id = object(MetadataKind::Attribute, "Amount", Some(catalog));
    project.reveal_declared_object(&id).unwrap();
    let editing = project.property_editing(&id).unwrap();
    assert_eq!(editing.properties.fields.len(), 4);
    let field = editing
        .properties
        .fields
        .iter()
        .find(|field| matches!(field.schema, ScalarSchema::DataType { .. }))
        .unwrap();
    let choices = project.property_type_choices(&id, &field.path).unwrap();
    assert!(
        choices
            .iter()
            .any(|choice| choice.key.name == "CatalogRef.Items")
    );
    let reference = PropertyChange::DataType {
        key: PropertyKey {
            namespace: Some("http://v8.1c.ru/8.1/data/enterprise/current-config".into()),
            name: "CatalogRef.Items".into(),
        },
    };
    assert!(
        project
            .preview_property(&id, &editing.properties.snapshot, &field.path, &reference)
            .is_ok()
    );
    let missing = PropertyChange::DataType {
        key: PropertyKey {
            namespace: Some("http://v8.1c.ru/8.1/data/enterprise/current-config".into()),
            name: "CatalogRef.Missing".into(),
        },
    };
    assert!(
        project
            .preview_property(&id, &editing.properties.snapshot, &field.path, &missing)
            .is_err()
    );
    let scale = editing
        .properties
        .fields
        .iter()
        .find(|field| field.path.last().unwrap().key.name == "FractionDigits")
        .unwrap();
    assert!(matches!(
        project.preview_property(
            &id,
            &editing.properties.snapshot,
            &scale.path,
            &PropertyChange::Text { value: "13".into() }
        ),
        Err(PropertyEditError::Edit(EditError::InvalidValue))
    ));
    let change = PropertyChange::DataType {
        key: PropertyKey {
            namespace: Some("http://www.w3.org/2001/XMLSchema".into()),
            name: "string".into(),
        },
    };
    project
        .update_property(&id, &editing.properties.snapshot, &field.path, &change)
        .unwrap();
    project
        .changed_paths(&[PathBuf::from("Catalogs/Items.xml")])
        .unwrap();
    let current = project.property_editing(&id).unwrap();
    assert!(
        fs::read_to_string(&path)
            .unwrap()
            .contains("<v:StringQualifiers>")
    );
    project
        .undo_property(&id, &current.properties.snapshot, true)
        .unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), xml);
}

#[cfg(unix)]
#[test]
fn failed_publication_preserves_source_and_removes_its_temporary_file() {
    use std::os::unix::fs::PermissionsExt;
    let (directory, mut workspace, id, path) = editable_fixture("configuration");
    let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
    let state = project.property_editing(&id).unwrap();
    let field = state
        .properties
        .fields
        .iter()
        .find(|field| field.path[0].key.name == "Comment")
        .unwrap();
    let plan = project
        .preview_property(
            &id,
            &state.properties.snapshot,
            &field.path,
            &PropertyChange::Text {
                value: "new".into(),
            },
        )
        .unwrap();
    let source = directory.0.join("src");
    let original = fs::read(source.join(&path)).unwrap();
    // Replacing the destination with a directory must be rejected before staging.
    let target = source.join(&path);
    let backup = source.join("original.xml");
    fs::rename(&target, &backup).unwrap();
    fs::create_dir(&target).unwrap();
    assert!(plan.publish(&source, &path).is_err());
    fs::remove_dir(&target).unwrap();
    fs::rename(backup, &target).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o444)).unwrap();
    assert!(matches!(
        plan.publish(&source, &path),
        Err(EditError::ReadOnly)
    ));
    fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(fs::read(target).unwrap(), original);
    assert!(fs::read_dir(source).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".eska-properties-")
    }));
}

mod integers;
mod standard;

/// Disabling Global exposes the module's name as a property and must reject reserved name collisions.
#[test]
fn common_module_global_flag_obeys_the_same_name_domain_as_rename() {
    for (name, allowed) in [("Catalogs", false), ("WorkingDate", true)] {
        let directory = TestDir::new();
        fs::create_dir_all(directory.0.join("src/CommonModules")).unwrap();
        fs::create_dir_all(directory.0.join("src/Ext")).unwrap();
        fs::write(
            directory.0.join("eska.toml"),
            "[project]\ntype='configuration'\n",
        )
        .unwrap();
        fs::write(directory.0.join("src/Configuration.xml"), format!(r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"><Configuration uuid="root"><Properties><Name>Demo</Name></Properties><ChildObjects><CommonModule>{name}</CommonModule></ChildObjects></Configuration></MetaDataObject>"#)).unwrap();
        fs::write(
            directory.0.join("src/Ext/ParentConfigurations.bin"),
            "{6,0,0,0,0,0}",
        )
        .unwrap();
        let path = directory.0.join(format!("src/CommonModules/{name}.xml"));
        let input = format!(
            r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"><CommonModule uuid="module"><Properties><Name>{name}</Name><Global>true</Global></Properties></CommonModule></MetaDataObject>"#
        );
        fs::write(&path, &input).unwrap();
        let mut workspace = MetadataWorkspace::open(&directory.0, &[], false).unwrap();
        let project = workspace.project_mut(&ProjectScope::Standalone).unwrap();
        let id = object(MetadataKind::CommonModule, name, None);
        let state = project.property_editing(&id).unwrap();
        let field = state
            .properties
            .fields
            .iter()
            .find(|field| field.path[0].key.name == "Global")
            .unwrap();
        let result = project.update_property(
            &id,
            &state.properties.snapshot,
            &field.path,
            &PropertyChange::Text {
                value: "false".into(),
            },
        );
        if allowed {
            result.unwrap();
            assert_eq!(
                fs::read_to_string(&path).unwrap(),
                input.replace("<Global>true</Global>", "<Global>false</Global>")
            );
        } else {
            assert!(
                matches!(result, Err(PropertyEditError::Edit(EditError::IncompatibleProperty(key))) if key.name == "Name")
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), input);
        }
    }
}
