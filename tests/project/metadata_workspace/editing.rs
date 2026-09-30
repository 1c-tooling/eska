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
        assert!(editing.undo);
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
        assert!(editing.redo);
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
