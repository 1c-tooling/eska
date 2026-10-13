//! Dialog registration is structural; bounded reads keep absence distinct from current failures.

use std::{fs, path::PathBuf};

use crate::{
    project_core::source_format::{FileRole, SourcePath, SourceReader, import_designer},
    test_support::TestDir,
};

/// Import two declared common forms; one has a dialog and the other deliberately has none.
fn source() -> (TestDir, PathBuf) {
    let directory = TestDir::new();
    let native = directory.0.join("native");
    fs::create_dir_all(native.join("CommonForms/Reader/Ext")).unwrap();
    fs::write(native.join("Configuration.xml"), r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20"><Configuration uuid="10000000-0000-4000-8000-000000000001"><Properties><Name>FormProbe</Name></Properties><ChildObjects><CommonForm>Reader</CommonForm><CommonForm>Absent</CommonForm></ChildObjects></Configuration></MetaDataObject>"#).unwrap();
    for (name, suffix) in [("Reader", 1), ("Absent", 2)] {
        fs::write(native.join(format!("CommonForms/{name}.xml")), format!(r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20"><CommonForm uuid="20000000-0000-4000-8000-{suffix:012}"><Properties><Name>{name}</Name></Properties></CommonForm></MetaDataObject>"#)).unwrap();
    }
    fs::write(
        native.join("CommonForms/Reader/Ext/Form.xml"),
        "\u{feff}<Form/>\r\n",
    )
    .unwrap();
    let source = directory.0.join("source");
    import_designer(&native, &source).unwrap();
    (directory, source)
}

/// Use validated source paths for both manifest identities and deliberately undeclared resources.
fn path(text: &str) -> SourcePath {
    SourcePath::parse(text).unwrap()
}

/// Root opening preserves a declared resource identity without activating broken or missing payloads.
#[test]
fn dialogs_bind_only_to_declared_common_forms_and_do_not_require_readable_owner_payloads() {
    let (_directory, root) = source();
    let owner = path("CommonForms/Reader/object.yaml");
    let resource = path("CommonForms/Reader/Ext/Form.xml");
    let reader = SourceReader::open(&root).unwrap();
    assert_eq!(reader.form_dialog_paths().collect::<Vec<_>>(), [&resource]);
    assert_eq!(reader.form_dialog_path(&owner), Some(&resource));
    assert_eq!(reader.form_dialog_owner(&resource), Some(&owner));
    assert!(
        reader
            .form_dialog_path(&path("CommonForms/Absent/object.yaml"))
            .is_none()
    );
    assert!(
        reader
            .form_dialog_path(&path("configuration.yaml"))
            .is_none()
    );
    let mut manifest = reader.manifest().clone();
    let entry = manifest
        .files
        .iter()
        .find(|file| file.path == resource)
        .unwrap()
        .clone();
    for name in [
        "CommonForms/Reader/Extra/Ext/Form.xml",
        "CommonForms/Undeclared/Ext/Form.xml",
        "Catalogs/Items/Ext/Form.xml",
    ] {
        let mut stray = entry.clone();
        stray.path = path(name);
        manifest.files.push(stray);
        assert!(reader.form_dialog_owner(&path(name)).is_none());
        assert_eq!(
            reader
                .read_form_dialog_text(&path(name), 1024)
                .unwrap_err()
                .code,
            "source_form_dialog_undeclared"
        );
    }
    fs::write(
        root.join("source.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    fs::write(root.join(owner.as_path()), "document: [").unwrap();
    fs::remove_file(root.join(resource.as_path())).unwrap();
    let reader = SourceReader::open(&root).unwrap();
    assert_eq!(reader.form_dialog_path(&owner), Some(&resource));
    assert_eq!(reader.form_dialog_paths().count(), 1);
    assert_eq!(
        reader
            .read_form_dialog_text(&resource, 1024)
            .unwrap_err()
            .code,
        "source_file_missing"
    );
}

/// Current byte reads preserve BOM/CRLF and distinguish malformed XML, size, encoding and deletion.
#[test]
fn dialogs_text_reads_are_current_verbatim_and_bounded() {
    let (_directory, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let resource = path("CommonForms/Reader/Ext/Form.xml");
    assert_eq!(
        reader.read_form_dialog_text(&resource, 1024).unwrap(),
        "\u{feff}<Form/>\r\n"
    );
    assert_eq!(
        reader.read_form_dialog_text(&resource, 1).unwrap_err().code,
        "source_document_too_large"
    );
    fs::write(root.join(resource.as_path()), "<Form>").unwrap();
    assert_eq!(
        reader.read_form_dialog_text(&resource, 1024).unwrap(),
        "<Form>"
    );
    fs::write(root.join(resource.as_path()), [0xff, 0xfe]).unwrap();
    assert_eq!(
        reader
            .read_form_dialog_text(&resource, 1024)
            .unwrap_err()
            .code,
        "source_encoding_unsupported"
    );
    fs::remove_file(root.join(resource.as_path())).unwrap();
    assert_eq!(
        reader
            .read_form_dialog_text(&resource, 1024)
            .unwrap_err()
            .code,
        "source_file_missing"
    );
}

/// Addressed layout reads share the declared identity and accept unsaved text without reading sibling payloads.
#[test]
fn dialog_layout_reads_are_lazy_addressed_and_overlay_aware() {
    let (_directory, root) = source();
    let owner = path("CommonForms/Reader/object.yaml");
    let resource = path("CommonForms/Reader/Ext/Form.xml");
    let xml = "\u{feff}<Form xmlns=\"http://v8.1c.ru/8.3/xcf/logform\" version=\"2.20\"><AutoCommandBar name=\"\" id=\"-1\"/><ChildItems><InputField name=\"Current\" id=\"1\"/></ChildItems></Form>\r\n";
    fs::write(root.join(resource.as_path()), xml).unwrap();
    fs::write(root.join(owner.as_path()), "document: [").unwrap();
    let reader = SourceReader::open(&root).unwrap();
    let saved = reader.read_form_layout(&resource, 1024).unwrap();
    assert_eq!(reader.parse_form_layout(&resource, xml).unwrap(), saved);
    assert_eq!(reader.read_form_dialog_text(&resource, 1024).unwrap(), xml);
    assert_eq!(
        reader.read_form_layout(&resource, 1).unwrap_err().code,
        "source_document_too_large"
    );
    fs::remove_file(root.join(resource.as_path())).unwrap();
    let overlay = xml.replace("Current", "Unsaved");
    let current = reader.parse_form_layout(&resource, &overlay).unwrap();
    assert_eq!(
        current.elements[&crate::project_core::source_format::FormId::parse("1").unwrap()].name,
        "Unsaved"
    );
    assert_eq!(
        reader.read_form_layout(&resource, 1024).unwrap_err().code,
        "source_file_missing"
    );
    assert_eq!(
        reader
            .parse_form_layout(&path("CommonForms/Absent/Ext/Form.xml"), &overlay)
            .unwrap_err()
            .code,
        "source_form_dialog_undeclared"
    );
}

/// Native ASCII spelling variations retain the exact declared path while physical aliases stay unsafe.
#[test]
fn dialogs_spelling_variations_keep_the_declared_path_and_owner() {
    let (_directory, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let original = path("CommonForms/Reader/Ext/Form.xml");
    let resource = path("CommonForms/Reader/ext/fOrM.XmL");
    let mut manifest = reader.manifest().clone();
    let entry = manifest
        .files
        .iter_mut()
        .find(|file| file.path == original)
        .unwrap();
    assert_eq!(entry.role, FileRole::Preserved);
    entry.path = resource.clone();
    fs::create_dir_all(root.join("CommonForms/Reader/ext")).unwrap();
    fs::rename(root.join(original.as_path()), root.join(resource.as_path())).unwrap();
    fs::write(
        root.join("source.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let reader = SourceReader::open(&root).unwrap();
    assert_eq!(
        reader.form_dialog_path(&path("CommonForms/Reader/object.yaml")),
        Some(&resource)
    );
    assert_eq!(
        reader.read_form_dialog_text(&resource, 1024).unwrap(),
        "\u{feff}<Form/>\r\n"
    );
    assert!(reader.form_dialog_owner(&original).is_none());
}

/// A mapped symlink cannot redirect a dialog read outside the source snapshot's actual files.
#[cfg(unix)]
#[test]
fn dialogs_reads_reject_filesystem_aliases() {
    let (directory, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let resource = path("CommonForms/Reader/Ext/Form.xml");
    fs::remove_file(root.join(resource.as_path())).unwrap();
    let outside = directory.0.join("outside.xml");
    fs::write(&outside, "<Form/>").unwrap();
    std::os::unix::fs::symlink(&outside, root.join(resource.as_path())).unwrap();
    assert_eq!(
        reader
            .read_form_dialog_text(&resource, 1024)
            .unwrap_err()
            .code,
        "source_entry_unsupported"
    );
}

/// Separate objects deliberately declare the same form name, alongside absent and binary dialogs.
fn nested_source() -> (TestDir, PathBuf) {
    let (directory, _source) = source();
    let native = directory.0.join("native");
    let configuration = fs::read_to_string(native.join("Configuration.xml")).unwrap();
    fs::write(
        native.join("Configuration.xml"),
        configuration.replace(
            "</ChildObjects>",
            "<Catalog>Items</Catalog><Catalog>Orders</Catalog></ChildObjects>",
        ),
    )
    .unwrap();
    fs::create_dir(native.join("Catalogs")).unwrap();
    for (name, suffix) in [("Items", 1), ("Orders", 2)] {
        let extra = if suffix == 1 {
            "<Form>Absent</Form><Form>Binary</Form>"
        } else {
            ""
        };
        fs::write(native.join(format!("Catalogs/{name}.xml")), format!(r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20"><Catalog uuid="30000000-0000-4000-8000-{suffix:012}"><Properties><Name>{name}</Name></Properties><ChildObjects><Form>Shared</Form>{extra}</ChildObjects></Catalog></MetaDataObject>"#)).unwrap();
        let directory = native.join(format!("Catalogs/{name}/Forms/Shared/Ext/Form"));
        fs::create_dir_all(&directory).unwrap();
        fs::write(native.join(format!("Catalogs/{name}/Forms/Shared.xml")), format!(r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20"><Form uuid="40000000-0000-4000-8000-{suffix:012}"><Properties><Name>Shared</Name><FormType>Managed</FormType></Properties></Form></MetaDataObject>"#)).unwrap();
        fs::write(directory.join("Module.bsl"), "// Current form body.\r\n").unwrap();
        fs::write(
            directory.parent().unwrap().join("Form.xml"),
            format!("\u{feff}<Form><!-- {name} Я😀 --></Form>\r\n"),
        )
        .unwrap();
    }
    for (name, suffix, kind) in [("Absent", 1, "Managed"), ("Binary", 2, "Ordinary")] {
        fs::write(native.join(format!("Catalogs/Items/Forms/{name}.xml")), format!(r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20"><Form uuid="50000000-0000-4000-8000-{suffix:012}"><Properties><Name>{name}</Name><FormType>{kind}</FormType></Properties></Form></MetaDataObject>"#)).unwrap();
    }
    fs::create_dir_all(native.join("Catalogs/Items/Forms/Binary/Ext")).unwrap();
    fs::write(
        native.join("Catalogs/Items/Forms/Binary/Ext/Form.bin"),
        [0xff, 0x00],
    )
    .unwrap();
    for stray in ["Catalogs/Items/Forms/Stray/Ext", "Catalogs/Items/Ext"] {
        fs::create_dir_all(native.join(stray)).unwrap();
        fs::write(native.join(stray).join("Form.xml"), "<Form/>").unwrap();
    }
    let source = directory.0.join("nested-source");
    import_designer(&native, &source).unwrap();
    (directory, source)
}

/// Equal short names remain distinct through descriptor, YAML owner, UUID and module backlink.
#[test]
fn nested_forms_keep_full_identity_and_link_only_declared_dialogs() {
    use crate::project_core::{MetadataKind, metadata_parser::PropertiesMode};
    let (_directory, root) = nested_source();
    let reader = SourceReader::open(&root).unwrap();
    assert_eq!(reader.form_paths().count(), 6);
    assert_eq!(reader.form_dialog_paths().count(), 3);
    for name in ["Items", "Orders"] {
        let descriptor = path(&format!("Catalogs/{name}/Forms/Shared.xml"));
        let owner = path(&format!("Catalogs/{name}/object.yaml"));
        let resource = path(&format!("Catalogs/{name}/Forms/Shared/Ext/Form.xml"));
        let form = reader.declared_form(&descriptor).unwrap();
        assert_eq!(form.name, "Shared");
        assert_eq!(form.kind, MetadataKind::Form);
        assert_eq!(form.owner, &owner);
        assert_eq!(form.native_descriptor, &descriptor);
        assert_eq!(form.dialog, Some(&resource));
        assert_eq!(reader.form_for_dialog(&resource), Some(form));
        assert_eq!(reader.form_dialog_owner(&resource), Some(&owner));
        assert!(reader.form_dialog_path(&owner).is_none());
        let snapshot = reader.read_metadata(&owner, PropertiesMode::All).unwrap();
        let current = snapshot
            .documents
            .iter()
            .find(|document| document.path == descriptor)
            .unwrap();
        assert_eq!(current.descriptor.objects[0].metadata.kind(), form.kind);
        assert_eq!(current.descriptor.objects[0].metadata.name(), form.name);
        let module = path(&format!("Catalogs/{name}/Forms/Shared/Ext/Form/Module.bsl"));
        let body = reader.declared_module(&module).unwrap();
        assert_eq!(body.owner, form.owner);
        assert_eq!(body.native_descriptor, form.native_descriptor);
        let text = reader.read_form_dialog_text(&resource, 1024).unwrap();
        assert!(text.contains(name));
        assert!(text.starts_with('\u{feff}') && text.ends_with("\r\n"));
    }
    let items = reader
        .declared_form(&path("Catalogs/Items/Forms/Shared.xml"))
        .unwrap();
    let orders = reader
        .declared_form(&path("Catalogs/Orders/Forms/Shared.xml"))
        .unwrap();
    assert_ne!(items.uuid, orders.uuid);
    for name in ["Absent", "Binary"] {
        let native = path(&format!("Catalogs/Items/Forms/{name}.xml"));
        assert!(reader.declared_form(&native).unwrap().dialog.is_none());
    }
    for stray in [
        "Catalogs/Items/Forms/Stray/Ext/Form.xml",
        "Catalogs/Items/Ext/Form.xml",
        "Catalogs/Items/Forms/Binary/Ext/Form.bin",
    ] {
        let stray = path(stray);
        assert!(reader.form_for_dialog(&stray).is_none());
        assert_eq!(
            reader.read_form_dialog_text(&stray, 1024).unwrap_err().code,
            "source_form_dialog_undeclared"
        );
    }
}

/// Registration needs no nested payload; bounded reads still use the exact current resource path.
#[test]
fn nested_identity_survives_missing_dialog_and_invalid_owner_text() {
    let (_directory, root) = nested_source();
    let owner = path("Catalogs/Items/object.yaml");
    let native = path("Catalogs/Items/Forms/Shared.xml");
    let resource = path("Catalogs/Items/Forms/Shared/Ext/Form.xml");
    let reader = SourceReader::open(&root).unwrap();
    let original = reader.declared_form(&native).unwrap().uuid.clone();
    assert_eq!(
        reader.read_form_dialog_text(&resource, 1).unwrap_err().code,
        "source_document_too_large"
    );
    fs::write(root.join(resource.as_path()), [0xff]).unwrap();
    assert_eq!(
        reader
            .read_form_dialog_text(&resource, 1024)
            .unwrap_err()
            .code,
        "source_encoding_unsupported"
    );
    fs::write(root.join(owner.as_path()), "document: [").unwrap();
    fs::remove_file(root.join(resource.as_path())).unwrap();
    let reader = SourceReader::open(&root).unwrap();
    let form = reader.declared_form(&native).unwrap();
    assert_eq!(form.uuid, &original);
    assert_eq!(form.dialog, Some(&resource));
    assert_eq!(
        reader
            .read_form_dialog_text(&resource, 1024)
            .unwrap_err()
            .code,
        "source_file_missing"
    );
    fs::write(
        root.join(resource.as_path()),
        "<Form><!-- Current --></Form>\r\n",
    )
    .unwrap();
    assert_eq!(
        reader.read_form_dialog_text(&resource, 1024).unwrap(),
        "<Form><!-- Current --></Form>\r\n"
    );
}

/// A nested dialog's case variant changes only its physical resource identity, not its owner/form.
#[test]
fn nested_dialog_case_variants_retain_full_descriptor_identity() {
    let (_directory, root) = nested_source();
    let reader = SourceReader::open(&root).unwrap();
    let old = path("Catalogs/Items/Forms/Shared/Ext/Form.xml");
    let resource = path("Catalogs/Items/Forms/Shared/ext/fOrM.XmL");
    let native = path("Catalogs/Items/Forms/Shared.xml");
    let mut manifest = reader.manifest().clone();
    manifest
        .files
        .iter_mut()
        .find(|file| file.path == old)
        .unwrap()
        .path = resource.clone();
    fs::create_dir_all(root.join("Catalogs/Items/Forms/Shared/ext")).unwrap();
    fs::rename(root.join(old.as_path()), root.join(resource.as_path())).unwrap();
    fs::write(
        root.join("source.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let reader = SourceReader::open(&root).unwrap();
    let form = reader.declared_form(&native).unwrap();
    assert_eq!(form.dialog, Some(&resource));
    assert_eq!(reader.form_for_dialog(&resource), Some(form));
    assert!(reader.form_for_dialog(&old).is_none());
    assert!(
        reader
            .read_form_dialog_text(&resource, 1024)
            .unwrap()
            .contains("Items")
    );
}
