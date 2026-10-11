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
