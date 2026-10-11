//! Predefined-data identities survive unavailable payloads; bounded reads retain exact current bytes.

use std::{fmt::Write as _, fs, path::PathBuf};

use crate::{
    project_core::{
        MetadataKind,
        source_format::{SourcePath, SourceReader, import_designer},
    },
    test_support::TestDir,
};

const TEXT: &str = "\u{feff}<PredefinedData xmlns=\"http://v8.1c.ru/8.3/xcf/predef\"><!-- Я😀 --><Item id=\"10000000-0000-4000-8000-000000000010\"><Name>Saved</Name></Item></PredefinedData>\r\n";

/// The fixture owns all four supported families plus declared absent and unrelated resources.
fn source() -> (TestDir, PathBuf) {
    let directory = TestDir::new();
    let native = directory.0.join("native");
    fs::create_dir(&native).unwrap();
    let mut children = String::new();
    for (index, kind) in [
        MetadataKind::Catalog,
        MetadataKind::ChartOfAccounts,
        MetadataKind::ChartOfCalculationTypes,
        MetadataKind::ChartOfCharacteristicTypes,
        MetadataKind::Document,
    ]
    .iter()
    .enumerate()
    {
        let tag = kind.designer_tag();
        let folder = kind.collection_folder().unwrap();
        write!(children, "<{tag}>Sample</{tag}>").unwrap();
        fs::create_dir_all(native.join(format!("{folder}/Sample/Ext"))).unwrap();
        fs::write(native.join(format!("{folder}/Sample.xml")), format!("<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><{tag} uuid=\"20000000-0000-4000-8000-{:012}\"><Properties><Name>Sample</Name></Properties></{tag}></MetaDataObject>", index+1)).unwrap();
        fs::write(
            native.join(format!("{folder}/Sample/Ext/Predefined.xml")),
            TEXT,
        )
        .unwrap();
    }
    fs::write(native.join("Catalogs/Absent.xml"), "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Catalog uuid=\"20000000-0000-4000-8000-000000000006\"><Properties><Name>Absent</Name></Properties></Catalog></MetaDataObject>").unwrap();
    children.push_str("<Catalog>Absent</Catalog>");
    fs::write(native.join("Configuration.xml"), format!("<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Configuration uuid=\"10000000-0000-4000-8000-000000000001\"><Properties><Name>PredefinedProbe</Name></Properties><ChildObjects>{children}</ChildObjects></Configuration></MetaDataObject>")).unwrap();
    fs::create_dir_all(native.join("Catalogs/Stray/Ext")).unwrap();
    fs::write(native.join("Catalogs/Stray/Ext/Predefined.xml"), TEXT).unwrap();
    let root = directory.0.join("source");
    import_designer(&native, &root).unwrap();
    (directory, root)
}

/// Use validated source paths without deriving identities from payload or filesystem contents.
fn path(text: &str) -> SourcePath {
    SourcePath::parse(text).unwrap()
}

/// Only supported declared owners activate sidecars, independently of YAML or XML readiness.
#[test]
fn predefined_registration_uses_composition_and_excludes_stray_and_other_families() {
    let (_directory, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    assert_eq!(reader.predefined_paths().count(), 4);
    for resource in reader.predefined_paths() {
        let owner = reader.predefined_owner(resource).unwrap();
        assert_eq!(reader.predefined_path(owner), Some(resource));
        assert_eq!(reader.read_predefined_text(resource, 1024).unwrap(), TEXT);
    }
    for owner in [
        "configuration.yaml",
        "Catalogs/Absent/object.yaml",
        "Documents/Sample/object.yaml",
    ] {
        assert!(reader.predefined_path(&path(owner)).is_none());
    }
    for resource in [
        "Catalogs/Stray/Ext/Predefined.xml",
        "Catalogs/Sample/Extra/Ext/Predefined.xml",
        "Documents/Sample/Ext/Predefined.xml",
    ] {
        assert!(reader.predefined_owner(&path(resource)).is_none());
        assert_eq!(
            reader
                .read_predefined_text(&path(resource), 1024)
                .unwrap_err()
                .code,
            "source_predefined_undeclared"
        );
    }
    let resource = path("Catalogs/Sample/Ext/Predefined.xml");
    let owner = path("Catalogs/Sample/object.yaml");
    fs::write(root.join(owner.as_path()), "document: [").unwrap();
    fs::remove_file(root.join(resource.as_path())).unwrap();
    let reader = SourceReader::open(&root).unwrap();
    assert_eq!(reader.predefined_path(&owner), Some(&resource));
    assert_eq!(
        reader
            .read_predefined_text(&resource, 1024)
            .unwrap_err()
            .code,
        "source_file_missing"
    );
}

/// Invalid XML remains current text; byte size, encoding and deletion failures remain explicit.
#[test]
fn predefined_reads_are_verbatim_current_and_bounded() {
    let (_directory, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let resource = path("Catalogs/Sample/Ext/Predefined.xml");
    assert_eq!(
        reader
            .read_predefined_text(&resource, TEXT.len() as u64)
            .unwrap(),
        TEXT
    );
    assert_eq!(
        reader
            .read_predefined_text(&resource, TEXT.len() as u64 - 1)
            .unwrap_err()
            .code,
        "source_document_too_large"
    );
    fs::write(root.join(resource.as_path()), "<PredefinedData>").unwrap();
    assert_eq!(
        reader.read_predefined_text(&resource, 1024).unwrap(),
        "<PredefinedData>"
    );
    fs::write(root.join(resource.as_path()), [0xff]).unwrap();
    assert_eq!(
        reader
            .read_predefined_text(&resource, 1024)
            .unwrap_err()
            .code,
        "source_encoding_unsupported"
    );
    fs::remove_file(root.join(resource.as_path())).unwrap();
    assert_eq!(
        reader
            .read_predefined_text(&resource, 1024)
            .unwrap_err()
            .code,
        "source_file_missing"
    );
}

/// The exact manifest spelling remains authoritative after native ASCII case folding.
#[test]
fn predefined_spelling_variations_preserve_the_declared_identity() {
    let (_directory, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let mut manifest = reader.manifest().clone();
    let original = path("Catalogs/Sample/Ext/Predefined.xml");
    let resource = path("Catalogs/Sample/ext/pReDeFiNeD.XmL");
    manifest
        .files
        .iter_mut()
        .find(|file| file.path == original)
        .unwrap()
        .path = resource.clone();
    fs::create_dir(root.join("Catalogs/Sample/ext")).unwrap();
    fs::rename(root.join(original.as_path()), root.join(resource.as_path())).unwrap();
    fs::write(
        root.join("source.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let reader = SourceReader::open(&root).unwrap();
    assert_eq!(
        reader.predefined_path(&path("Catalogs/Sample/object.yaml")),
        Some(&resource)
    );
    assert!(reader.predefined_owner(&original).is_none());
    assert_eq!(reader.read_predefined_text(&resource, 1024).unwrap(), TEXT);
}

/// A resource symlink cannot redirect a current read outside this test-owned source tree.
#[cfg(unix)]
#[test]
fn predefined_reads_reject_filesystem_aliases() {
    let (directory, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let resource = path("Catalogs/Sample/Ext/Predefined.xml");
    let outside = directory.0.join("outside.xml");
    fs::write(&outside, TEXT).unwrap();
    fs::remove_file(root.join(resource.as_path())).unwrap();
    std::os::unix::fs::symlink(&outside, root.join(resource.as_path())).unwrap();
    assert_eq!(
        reader
            .read_predefined_text(&resource, 1024)
            .unwrap_err()
            .code,
        "source_entry_unsupported"
    );
}
