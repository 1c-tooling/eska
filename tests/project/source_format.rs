use std::{
    fs,
    path::{Path, PathBuf},
};

use eska::project::{
    metadata_model::MetadataKind,
    source_format::{
        Child, Descriptor, FileRole, ObjectSource, PropertyName, PropertyValue, Section,
        SourcePath, export_designer, import_designer,
    },
};

use crate::support::TestDir;

mod baseline;
mod creation;
mod deletion;
mod edit;
mod fill_value;
mod layout;
mod measure;
mod order;
mod platform;
mod protected;
mod readable;
mod reference_clear;
mod references;
mod rename;
mod scalars;
mod table_deletion;
mod table_rename;
mod type_targets;
mod types;

/// Work on a private copy so adverse input tests cannot modify the shared fixture corpus.
fn fixture(root: &Path) -> PathBuf {
    let input = root.join("xml");
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eska-format/designer"),
        &input,
    );
    input
}

/// Copy the small repository fixture, preserving payload bytes without native-platform claims.
fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &destination.join(entry.file_name()));
        } else {
            fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
        }
    }
}

#[test]
fn import_follows_declarations_and_preserves_every_other_file() {
    let test = TestDir::new();
    let input = fixture(&test.0);
    let first = test.0.join("first");
    let second = test.0.join("second");
    let manifest = import_designer(&input, &first).unwrap();
    assert_eq!(manifest, import_designer(&input, &second).unwrap());
    assert_eq!(manifest.files.len(), 9);
    assert_eq!(
        manifest
            .files
            .iter()
            .filter(|file| matches!(file.role, FileRole::Descriptor { .. }))
            .count(),
        4
    );
    assert_eq!(
        fs::read(first.join("source.json")).unwrap(),
        fs::read(second.join("source.json")).unwrap()
    );
    for file in &manifest.files {
        match &file.role {
            FileRole::Preserved | FileRole::Relocated { .. } => {
                let original = fs::read(input.join(file.path.as_path())).unwrap();
                assert_eq!(original, fs::read(first.join(file.path.as_path())).unwrap());
                assert_eq!(
                    original,
                    fs::read(second.join(file.path.as_path())).unwrap()
                );
            }
            FileRole::Descriptor { source, .. } | FileRole::ManagedForm { source, .. } => {
                assert!(!first.join(file.path.as_path()).exists());
                assert_eq!(
                    fs::read(first.join(source.as_path())).unwrap(),
                    fs::read(second.join(source.as_path())).unwrap()
                );
            }
        }
    }
    let catalog = ObjectSource::from_yaml(
        &fs::read_to_string(first.join("Catalogs/Sample/object.yaml")).unwrap(),
    )
    .unwrap();
    assert_eq!(catalog.nested.len(), 1);
    assert_eq!(catalog.nested[0].descriptor.object.kind, MetadataKind::Form);
    assert_eq!(
        catalog.nested[0].path.as_str(),
        "Catalogs/Sample/Forms/Main.xml"
    );
    let configuration =
        ObjectSource::from_yaml(&fs::read_to_string(first.join("configuration.yaml")).unwrap())
            .unwrap();
    let Section::Children(children) = &configuration.document.descriptor.object.sections[1] else {
        panic!("children")
    };
    assert!(matches!(
        &children[..],
        [
            Child::Reference {
                metadata_kind: MetadataKind::CommonModule,
                ..
            },
            Child::Reference {
                metadata_kind: MetadataKind::Catalog,
                ..
            }
        ]
    ));
    assert_eq!(
        configuration
            .document
            .descriptor
            .object
            .property(PropertyName::Version),
        Some(&PropertyValue::Text("1.2.3.004".into()))
    );
}

#[test]
fn undeclared_metadata_is_not_parsed_or_activated() {
    let test = TestDir::new();
    let input = fixture(&test.0);
    fs::write(input.join("outside.xml"), b"<unsupported-and-broken").unwrap();
    let destination = test.0.join("eska");
    let manifest = import_designer(&input, &destination).unwrap();
    assert!(manifest.files.iter().any(
        |file| file.path.as_str() == "outside.xml" && matches!(file.role, FileRole::Preserved)
    ));
    assert_eq!(
        fs::read(destination.join("outside.xml")).unwrap(),
        b"<unsupported-and-broken"
    );
}

#[test]
fn unsupported_profile_and_missing_declared_files_do_not_create_output() {
    for (change, code) in [
        ("profile", "source_profile_unsupported"),
        ("missing", "source_descriptor_missing"),
        ("extension", "source_profile_unsupported"),
    ] {
        let test = TestDir::new();
        let input = fixture(&test.0);
        let root = input.join("Configuration.xml");
        if change == "missing" {
            fs::remove_file(input.join("Catalogs/Sample.xml")).unwrap();
        } else {
            let xml = fs::read_to_string(&root).unwrap();
            fs::write(&root, if change == "profile" { xml.replace("version=\"2.20\"", "version=\"9.99\"") }
                else { xml.replace("<Properties>", "<Properties><ConfigurationExtensionPurpose>Patch</ConfigurationExtensionPurpose>") }).unwrap();
        }
        let destination = test.0.join("eska");
        assert_eq!(
            import_designer(&input, &destination).unwrap_err().code,
            code
        );
        assert!(!destination.exists());
    }
}

#[test]
fn duplicate_native_uuid_across_files_is_refused() {
    let test = TestDir::new();
    let input = fixture(&test.0);
    let path = input.join("CommonModules/Tools.xml");
    let xml = fs::read_to_string(&path).unwrap().replace(
        "60000000-0000-4000-8000-000000000001",
        "10000000-0000-4000-8000-000000000001",
    );
    fs::write(path, xml).unwrap();
    let destination = test.0.join("eska");
    assert_eq!(
        import_designer(&input, &destination).unwrap_err().code,
        "native_uuid_duplicate"
    );
    assert!(!destination.exists());
}

#[test]
fn declaration_name_mismatch_and_unsafe_reference_are_refused() {
    for (name, code) in [
        ("Changed", "source_declaration_mismatch"),
        ("../Sample", "source_path_invalid"),
    ] {
        let test = TestDir::new();
        let input = fixture(&test.0);
        let path = if name.contains('/') {
            input.join("Configuration.xml")
        } else {
            input.join("Catalogs/Sample.xml")
        };
        let xml = fs::read_to_string(&path).unwrap().replace(
            if name.contains('/') {
                ">Sample</Catalog>"
            } else {
                ">Sample</Name>"
            },
            &format!(
                ">{name}</{}>",
                if name.contains('/') {
                    "Catalog"
                } else {
                    "Name"
                }
            ),
        );
        fs::write(path, xml).unwrap();
        let destination = test.0.join("eska");
        assert_eq!(
            import_designer(&input, &destination).unwrap_err().code,
            code
        );
        assert!(!destination.exists());
    }
}

#[test]
fn generated_yaml_collision_is_detected_before_writing() {
    let test = TestDir::new();
    let input = fixture(&test.0);
    fs::write(input.join("Catalogs/Sample/object.yaml"), "user payload").unwrap();
    let destination = test.0.join("eska");
    assert_eq!(
        import_designer(&input, &destination).unwrap_err().code,
        "source_path_collision"
    );
    assert!(!destination.exists());
    assert_eq!(
        fs::read_to_string(input.join("Catalogs/Sample/object.yaml")).unwrap(),
        "user payload"
    );
}

#[cfg(unix)]
#[test]
fn generated_owner_directory_cannot_alias_a_preserved_resource_directory() {
    let test = TestDir::new();
    let input = fixture(&test.0);
    fs::rename(
        input.join("CommonModules/Tools"),
        input.join("CommonModules/tools"),
    )
    .unwrap();
    let destination = test.0.join("eska");
    assert_eq!(
        import_designer(&input, &destination).unwrap_err().code,
        "source_path_collision"
    );
    assert!(!destination.exists());
}

#[test]
fn existing_destination_and_source_overlap_preserve_user_files() {
    let test = TestDir::new();
    let input = fixture(&test.0);
    let destination = test.0.join("eska");
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("user.txt"), "keep").unwrap();
    assert_eq!(
        import_designer(&input, &destination).unwrap_err().code,
        "source_destination_exists"
    );
    assert_eq!(
        fs::read_to_string(destination.join("user.txt")).unwrap(),
        "keep"
    );
    assert_eq!(
        import_designer(&input, &input.join("output"))
            .unwrap_err()
            .code,
        "source_tree_overlap"
    );
    assert!(!input.join("output").exists());
}

#[cfg(unix)]
#[test]
fn symlink_and_case_only_directory_aliases_are_refused() {
    let test = TestDir::new();
    let input = fixture(&test.0);
    std::os::unix::fs::symlink(test.0.join("outside"), input.join("link")).unwrap();
    let destination = test.0.join("eska");
    assert_eq!(
        import_designer(&input, &destination).unwrap_err().code,
        "source_entry_unsupported"
    );
    fs::remove_file(input.join("link")).unwrap();
    fs::create_dir(input.join("catalogs")).unwrap();
    assert_eq!(
        import_designer(&input, &destination).unwrap_err().code,
        "source_path_collision"
    );
    assert!(!destination.exists());
}

#[test]
fn native_import_retains_unknown_xml_context_and_inline_identity() {
    let input = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/eska-format/designer/Catalogs/Sample.xml"),
    )
    .unwrap();
    let source = Descriptor::from_xml(&input).unwrap();
    assert_eq!(
        source,
        Descriptor::from_yaml(&source.to_yaml().unwrap()).unwrap()
    );
    assert!(
        source
            .to_yaml()
            .unwrap()
            .contains("123456789012345678901234567890.0001")
    );
    assert!(source.to_yaml().unwrap().contains("local:FutureType"));
    assert!(
        source
            .to_yaml()
            .unwrap()
            .contains("30000000-0000-4000-8000-000000000001")
    );
    assert!(source.object.property(PropertyName::Synonym).is_some_and(
        |value| matches!(value, PropertyValue::Localized(values) if values.len() == 2)
    ));
    for path in [
        "../escape",
        "/absolute",
        "a//b",
        "a/./b",
        "a\\b",
        "c:drive",
        "trailing.",
        "AUX.xml",
        "a/COM1.bsl",
    ] {
        assert!(SourcePath::parse(path).is_err());
    }
    assert!(SourcePath::parse("Forms/Имя.с.точкой.xml").is_ok());
}

#[test]
fn default_namespace_reset_inside_opaque_values_is_retained() {
    let md = eska::project::source_format::MD_NAMESPACE;
    let input = format!(
        "<MetaDataObject xmlns=\"{md}\" version=\"2.20\"><Configuration uuid=\"10000000-0000-4000-8000-000000000001\"><m:Properties xmlns:m=\"{md}\" xmlns=\"\"><m:Name>Sample</m:Name><Future>value</Future></m:Properties></Configuration></MetaDataObject>"
    );
    let source = Descriptor::from_xml(&input).unwrap();
    let Section::Properties(values) = &source.object.sections[0] else {
        panic!("properties")
    };
    let Some(eska::project::source_format::Property::Preserved { fragment }) = values.last() else {
        panic!("preserved")
    };
    assert_eq!(fragment.xml, "<Future>value</Future>");
    assert!(
        fragment
            .namespaces
            .iter()
            .any(|binding| binding.prefix.is_none() && binding.uri.is_empty())
    );
    assert_eq!(
        source,
        Descriptor::from_yaml(&source.to_yaml().unwrap()).unwrap()
    );
    let native = source.to_xml().unwrap();
    assert!(native.contains("xmlns=\"\""));
    let restored = Descriptor::from_xml(&native).unwrap();
    let Section::Properties(values) = &restored.object.sections[0] else {
        panic!("properties")
    };
    let Some(eska::project::source_format::Property::Preserved { fragment }) = values.last() else {
        panic!("preserved")
    };
    assert!(
        fragment
            .namespaces
            .iter()
            .any(|binding| binding.prefix.is_none() && binding.uri.is_empty())
    );
}

#[test]
fn native_writer_preserves_fixture_values_identity_order_and_opaque_bytes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eska-format/designer");
    for path in [
        "Configuration.xml",
        "Catalogs/Sample.xml",
        "CommonModules/Tools.xml",
        "Catalogs/Sample/Forms/Main.xml",
    ] {
        let original = Descriptor::from_xml(&fs::read_to_string(root.join(path)).unwrap()).unwrap();
        let xml = original.to_xml().unwrap();
        assert!(xml.starts_with("\u{feff}<?xml"));
        assert!(xml.ends_with("\r\n"));
        let restored = Descriptor::from_xml(&xml).unwrap();
        assert_eq!(original, restored, "{path}");
        assert_eq!(xml, restored.to_xml().unwrap());
    }
}

#[test]
#[ignore = "Read-only real corpus import; requires explicit playground input and a new output"]
fn real_playground_import() {
    let root = PathBuf::from(std::env::var_os("ESKA_TEST_ROOT").expect("ESKA_TEST_ROOT"))
        .canonicalize()
        .unwrap();
    let input = PathBuf::from(std::env::var_os("ESKA_FORMAT_INPUT").expect("ESKA_FORMAT_INPUT"))
        .canonicalize()
        .unwrap();
    let output = PathBuf::from(std::env::var_os("ESKA_FORMAT_OUTPUT").expect("ESKA_FORMAT_OUTPUT"));
    assert!(input.starts_with(&root));
    assert!(
        output
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(&root)
    );
    let manifest = import_designer(&input, &output).unwrap();
    let sources: std::collections::BTreeSet<_> = manifest
        .files
        .iter()
        .filter_map(|file| {
            if let FileRole::Descriptor { source, .. } = &file.role {
                Some(source)
            } else {
                None
            }
        })
        .collect();
    for path in &sources {
        ObjectSource::from_yaml(&fs::read_to_string(output.join(path.as_path())).unwrap()).unwrap();
    }
    println!(
        "{}",
        serde_json::json!({"files": manifest.files.len(), "descriptors": manifest.files.iter().filter(|file| matches!(file.role, FileRole::Descriptor { .. })).count(), "source_documents_read":sources.len(), "bytes": manifest.files.iter().map(|file| file.bytes).sum::<u64>()})
    );
}

/// A file round trip is stronger than descriptor-only checks but still does not prove 1C loading.
#[test]
fn native_export_restores_every_file_and_empty_directory_without_rewrites() {
    let test = TestDir::new();
    let input = fixture(&test.0);
    fs::create_dir_all(input.join("Empty/Deep")).unwrap();
    let source = test.0.join("eska");
    let native = test.0.join("native");
    let manifest = import_designer(&input, &source).unwrap();
    export_designer(&source, &native).unwrap();
    assert!(native.join("Empty/Deep").is_dir());
    let before: Vec<_> = manifest
        .files
        .iter()
        .map(|file| {
            let output = native.join(file.path.as_path());
            match &file.role {
                FileRole::Descriptor { .. } => {
                    assert_eq!(
                        Descriptor::from_xml(
                            &fs::read_to_string(input.join(file.path.as_path())).unwrap()
                        )
                        .unwrap(),
                        Descriptor::from_xml(&fs::read_to_string(&output).unwrap()).unwrap()
                    );
                }
                FileRole::Preserved | FileRole::Relocated { .. } | FileRole::ManagedForm { .. } => {
                    assert_eq!(
                        fs::read(input.join(file.path.as_path())).unwrap(),
                        fs::read(&output).unwrap()
                    );
                }
            }
            (
                output.clone(),
                fs::read(&output).unwrap(),
                fs::metadata(output).unwrap().modified().unwrap(),
            )
        })
        .collect();
    export_designer(&source, &native).unwrap();
    for (path, bytes, mtime) in before {
        assert_eq!(bytes, fs::read(&path).unwrap());
        assert_eq!(mtime, fs::metadata(path).unwrap().modified().unwrap());
    }
    assert_eq!(manifest.files.len(), count_files(&native));
    let reimport = test.0.join("reimport");
    let restored = import_designer(&native, &reimport).unwrap();
    assert_eq!(
        manifest
            .files
            .iter()
            .filter(|file| matches!(file.role, FileRole::Descriptor { .. }))
            .map(|file| &file.path)
            .collect::<Vec<_>>(),
        restored
            .files
            .iter()
            .filter(|file| matches!(file.role, FileRole::Descriptor { .. }))
            .map(|file| &file.path)
            .collect::<Vec<_>>()
    );
}

/// Count native outputs without accidentally admitting a generated ready marker as source data.
fn count_files(root: &Path) -> usize {
    fs::read_dir(root)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                count_files(&entry.path())
            } else {
                1
            }
        })
        .sum()
}

/// A mismatching destination must survive so later protected publication can retain prior output.
#[test]
fn changed_export_destination_is_preserved() {
    let test = TestDir::new();
    let input = fixture(&test.0);
    let source = test.0.join("eska");
    let native = test.0.join("native");
    import_designer(&input, &source).unwrap();
    export_designer(&source, &native).unwrap();
    fs::write(native.join("user.txt"), b"keep").unwrap();
    assert_eq!(
        export_designer(&source, &native).unwrap_err().code,
        "source_export_destination_mismatch"
    );
    assert_eq!(fs::read(native.join("user.txt")).unwrap(), b"keep");
}

/// Extra working files require an explicit file-map change instead of disappearing at export.
#[test]
fn source_file_map_mismatch_does_not_create_export() {
    let test = TestDir::new();
    let input = fixture(&test.0);
    let source = test.0.join("eska");
    let native = test.0.join("native");
    import_designer(&input, &source).unwrap();
    fs::write(source.join("unmapped.bsl"), b"user module").unwrap();
    assert_eq!(
        export_designer(&source, &native).unwrap_err().code,
        "source_file_map_mismatch"
    );
    assert!(!native.exists());
}

/// A working UUID edit cannot redirect an imported file-map entry to a different native identity.
#[test]
fn changed_descriptor_identity_does_not_create_export() {
    let test = TestDir::new();
    let input = fixture(&test.0);
    let source = test.0.join("eska");
    let native = test.0.join("native");
    import_designer(&input, &source).unwrap();
    let path = source.join("Catalogs/Sample/object.yaml");
    let yaml = fs::read_to_string(&path).unwrap().replace(
        "20000000-0000-4000-8000-000000000001",
        "20000000-0000-4000-8000-000000000002",
    );
    fs::write(path, yaml).unwrap();
    assert_eq!(
        export_designer(&source, &native).unwrap_err().code,
        "source_identity_mismatch"
    );
    assert!(!native.exists());
}

/// Native XML cannot distinguish a zero-length text node from the Empty state.
#[test]
fn empty_text_requires_the_explicit_empty_state() {
    let input = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/eska-format/designer/Configuration.xml"),
    )
    .unwrap();
    let mut source = Descriptor::from_xml(&input).unwrap();
    let Section::Properties(properties) = &mut source.object.sections[0] else {
        panic!("properties")
    };
    for property in properties {
        if let eska::project::source_format::Property::Typed {
            name: PropertyName::Comment,
            value,
        } = property
        {
            *value = PropertyValue::Text(String::new());
        }
    }
    assert_eq!(
        source.to_xml().unwrap_err().code,
        "source_property_type_invalid"
    );
}

/// A full real native snapshot compares data and identity independently of byte-identical XML.
#[test]
#[ignore = "Read-only real ESKA export; requires explicit original XML and new playground output"]
fn real_playground_export() {
    let root = PathBuf::from(std::env::var_os("ESKA_TEST_ROOT").expect("ESKA_TEST_ROOT"))
        .canonicalize()
        .unwrap();
    let source = PathBuf::from(std::env::var_os("ESKA_FORMAT_INPUT").expect("ESKA_FORMAT_INPUT"))
        .canonicalize()
        .unwrap();
    let original =
        PathBuf::from(std::env::var_os("ESKA_FORMAT_ORIGINAL").expect("ESKA_FORMAT_ORIGINAL"))
            .canonicalize()
            .unwrap();
    let output = PathBuf::from(std::env::var_os("ESKA_FORMAT_OUTPUT").expect("ESKA_FORMAT_OUTPUT"));
    assert!(source.starts_with(&root) && original.starts_with(&root));
    assert!(
        output
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(&root)
    );
    let manifest: eska::project::source_format::SourceManifest =
        serde_json::from_str(&fs::read_to_string(source.join("source.json")).unwrap()).unwrap();
    export_designer(&source, &output).unwrap();
    let mut differences = 0;
    for file in &manifest.files {
        if matches!(file.role, FileRole::Descriptor { .. }) {
            let one = Descriptor::from_xml(
                &fs::read_to_string(original.join(file.path.as_path())).unwrap(),
            )
            .unwrap();
            let two = Descriptor::from_xml(
                &fs::read_to_string(output.join(file.path.as_path())).unwrap(),
            )
            .unwrap();
            if one != two {
                differences += 1;
            }
        } else {
            assert_eq!(
                fs::read(original.join(file.path.as_path())).unwrap(),
                fs::read(output.join(file.path.as_path())).unwrap()
            );
        }
    }
    assert_eq!(count_files(&output), manifest.files.len());
    println!(
        "{}",
        serde_json::json!({"files":manifest.files.len(), "descriptor_model_differences":differences})
    );
    assert_eq!(differences, 0);
}
