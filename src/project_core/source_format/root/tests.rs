//! External profiles keep a single editable authority and prove their independent protected cycle.

use super::*;
use crate::{
    project_core::{
        metadata_parser::PropertiesMode,
        source_format::{
            AcceptedBase, Descriptor, NativeQueryDocument, PropertyName, PropertyValue,
            SourceReader, accept_source, export_designer, files::Inventory, import_designer,
            import_designer_queries, prepare_protected, queries::test_support,
        },
    },
    test_support::TestDir,
};
use std::{fs, path::PathBuf};

const STEM: &str = "Передача.v1";

/// Reuse the verified codec corpus, keeping only the external object's owned subtree.
fn native(directory: &TestDir, kind: ExternalKind, direct_module: bool) -> PathBuf {
    let input = test_support::fixture(directory);
    let root = directory.0.join("external");
    fs::create_dir(&root).unwrap();
    let subtree = input.join("Catalogs/Sample");
    let inventory = Inventory::read(&subtree).unwrap();
    for path in &inventory.directories {
        fs::create_dir_all(root.join(STEM).join(path.as_path())).unwrap();
    }
    for path in inventory.files.keys() {
        fs::copy(
            subtree.join(path.as_path()),
            root.join(STEM).join(path.as_path()),
        )
        .unwrap();
    }
    let tag = match kind {
        ExternalKind::Processing => "ExternalDataProcessor",
        ExternalKind::Report => "ExternalReport",
    };
    let text = fs::read_to_string(input.join("Catalogs/Sample.xml"))
        .unwrap()
        .replace("<Catalog ", &format!("<{tag} "))
        .replace("</Catalog>", &format!("</{tag}>"))
        .replace(
            "<Name>Sample</Name>",
            &format!("<Name>Sample</Name><DefaultForm>{tag}.Sample.Form.Main</DefaultForm>"),
        );
    fs::write(root.join(format!("{STEM}.xml")), text).unwrap();
    let module = if direct_module {
        root.join("Ext")
    } else {
        root.join(STEM).join("Ext")
    };
    fs::create_dir_all(&module).unwrap();
    fs::write(
        module.join("ObjectModule.bsl"),
        b"\xef\xbb\xbf// unchanged\r\n",
    )
    .unwrap();
    root
}

/// Both external tags survive separated forms, plain queries and aliased native filenames.
#[test]
fn external_roots_round_trip_forms_queries_modules_and_logical_parents() {
    for kind in [ExternalKind::Processing, ExternalKind::Report] {
        for direct in [false, true] {
            let directory = TestDir::new();
            let input = native(&directory, kind, direct);
            let before = Inventory::read(&input).unwrap();
            let source = directory.0.join("source");
            let manifest = import_designer_queries(&input, &source).unwrap();
            assert_eq!(manifest.schema_version, EXTERNAL_LAYOUT_VERSION);
            assert_eq!(
                manifest.external_root.as_ref().unwrap().path.as_str(),
                format!("{STEM}.xml")
            );
            assert!(source.join(kind.source()).is_file());
            assert!(source.join("Forms/Main/form.yaml").is_file());
            assert!(!source.join("configuration.yaml").exists());
            assert!(!source.join(STEM).exists());
            let reader = SourceReader::open(&source).unwrap();
            assert_eq!(reader.project_type().unwrap(), kind.project_type());
            assert_eq!(reader.configuration().path.as_str(), kind.source());
            assert_eq!(reader.query_paths().count(), 4);
            assert!(
                reader
                    .declared_module(&test_support::path("ObjectModule.bsl"))
                    .is_some()
            );
            let owner = reader
                .read_metadata(
                    &test_support::path("Forms/Main/form.yaml"),
                    PropertiesMode::All,
                )
                .unwrap();
            let object = &owner.documents[0].descriptor.objects[0].metadata;
            assert_eq!(
                object.parent().unwrap().as_str(),
                format!("{}:Sample", kind.metadata_kind().as_str())
            );
            assert_eq!(
                reader
                    .configuration()
                    .source
                    .document
                    .descriptor
                    .object
                    .property(PropertyName::DefaultForm),
                Some(&PropertyValue::Reference(
                    super::super::MetadataReference::parse(&format!(
                        "{}.Sample.Form.Main",
                        if kind == ExternalKind::Processing {
                            "ExternalDataProcessor"
                        } else {
                            "ExternalReport"
                        }
                    ))
                    .unwrap()
                ))
            );
            let output = directory.0.join("output");
            export_designer(&source, &output).unwrap();
            let root = Descriptor::from_xml(
                &fs::read_to_string(output.join(format!("{STEM}.xml"))).unwrap(),
            )
            .unwrap();
            assert_eq!(root.profile, kind.profile());
            assert_eq!(
                fs::read(source.join("ObjectModule.bsl")).unwrap(),
                b"\xef\xbb\xbf// unchanged\r\n"
            );
            assert_eq!(
                NativeQueryDocument::from_xml(
                    &fs::read_to_string(
                        output.join(format!("{STEM}/Templates/QuerySchema/Ext/Template.xml"))
                    )
                    .unwrap()
                )
                .unwrap()
                .queries()
                .len(),
                2
            );
            let repeat = directory.0.join("repeat");
            import_designer_queries(&output, &repeat).unwrap();
            export_designer(&repeat, &directory.0.join("repeat-output")).unwrap();
            assert_eq!(
                Inventory::read(&output).unwrap(),
                Inventory::read(&directory.0.join("repeat-output")).unwrap()
            );
            assert_eq!(Inventory::read(&input).unwrap(), before);
        }
    }
}

/// Root freeze includes descendant forms, queries and either native object module placement.
#[test]
fn external_accepted_base_restores_frozen_module_and_query_without_changing_working_bytes() {
    for kind in [ExternalKind::Processing, ExternalKind::Report] {
        let directory = TestDir::new();
        let input = native(&directory, kind, false);
        let source = directory.0.join("source");
        import_designer_queries(&input, &source).unwrap();
        let reader = SourceReader::open(&source).unwrap();
        let uuid = &reader
            .configuration()
            .source
            .document
            .descriptor
            .object
            .uuid;
        let reference = accept_source(
            &source,
            &directory.0.join("base"),
            std::slice::from_ref(uuid),
        )
        .unwrap();
        let accepted = AcceptedBase::open(&directory.0.join("base"), &reference).unwrap();
        let path = source.join("ObjectModule.bsl");
        let original = fs::read(&path).unwrap();
        let edited = b"\xef\xbb\xbf// user edit\r\n";
        fs::write(&path, edited).unwrap();
        let query = reader.query_paths().next().unwrap();
        let original_query = fs::read(source.join(query.as_path())).unwrap();
        fs::write(source.join(query.as_path()), "SELECT 999").unwrap();
        let failure =
            prepare_protected(&source, &accepted, &directory.0.join("strict"), true).unwrap_err();
        assert_eq!(failure.error.code, "source_changes_excluded");
        let prepared =
            prepare_protected(&source, &accepted, &directory.0.join("generations"), false).unwrap();
        assert_eq!(
            prepared.source_map().schema_version,
            super::super::EXTERNAL_SOURCE_MAP_VERSION
        );
        prepared.validate().unwrap();
        let module = reader
            .declared_module(&test_support::path("ObjectModule.bsl"))
            .unwrap()
            .native_path
            .clone();
        assert_eq!(
            fs::read(prepared.xml().join(module.as_path())).unwrap(),
            original
        );
        assert_eq!(fs::read(path).unwrap(), edited);
        assert_ne!(
            fs::read(source.join(query.as_path())).unwrap(),
            original_query
        );
        assert_eq!(prepared.diagnostics().len(), 2);
    }
}

/// Older layouts refuse external roots before publication; ambiguous project roots are never chosen.
#[test]
fn external_import_rejects_legacy_layout_and_multiple_roots() {
    let directory = TestDir::new();
    let input = native(&directory, ExternalKind::Processing, false);
    let target = directory.0.join("refused");
    assert_eq!(
        import_designer(&input, &target).unwrap_err().code,
        "source_profile_unsupported"
    );
    assert!(!target.exists());
    fs::copy(
        input.join(format!("{STEM}.xml")),
        input.join("Duplicate.xml"),
    )
    .unwrap();
    assert_eq!(
        import_designer_queries(&input, &target).unwrap_err().code,
        "source_descriptor_duplicate"
    );
    assert!(!target.exists());
}

/// Persisted profiles cannot spoof a different root, downgrade their map or activate another YAML.
#[test]
fn external_reader_refuses_tampered_root_profile_and_manifest() {
    let directory = TestDir::new();
    let input = native(&directory, ExternalKind::Report, false);
    let source = directory.0.join("source");
    let manifest = import_designer_queries(&input, &source).unwrap();
    for (field, value) in [
        ("schema_version", serde_json::json!(5)),
        ("profile", serde_json::json!(super::super::PROFILE)),
        ("external_root", serde_json::Value::Null),
    ] {
        let mut changed = serde_json::to_value(&manifest).unwrap();
        changed[field] = value;
        fs::write(
            source.join("source.json"),
            serde_json::to_vec(&changed).unwrap(),
        )
        .unwrap();
        assert!(SourceReader::open(&source).is_err());
    }
    fs::write(
        source.join("source.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let path = source.join(ExternalKind::Report.source());
    let text = fs::read_to_string(&path)
        .unwrap()
        .replace(REPORT_PROFILE, super::super::PROFILE);
    fs::write(path, text).unwrap();
    assert_eq!(
        SourceReader::open(&source).err().unwrap().code,
        "source_profile_unsupported"
    );
}
