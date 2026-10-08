use super::*;
use crate::{project_core::source_format::import_designer, test_support::TestDir};

/// Import only the small public corpus into an exclusively owned playground directory.
fn source() -> (TestDir, PathBuf) {
    let test = TestDir::new();
    let root = test.0.join("source");
    import_designer(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eska-format/designer"),
        &root,
    )
    .unwrap();
    (test, root)
}

/// Address the corpus owner without deriving active metadata from a filesystem scan.
fn catalog() -> SourcePath {
    SourcePath::parse("Catalogs/Sample/object.yaml").unwrap()
}

/// Persist adversarial manifests only in this test's own source copy.
fn write_manifest(root: &Path, manifest: &SourceManifest) {
    fs::write(
        root.join(files::MANIFEST),
        serde_json::to_vec_pretty(manifest).unwrap(),
    )
    .unwrap();
}

#[test]
fn opening_reads_only_root_and_manifest_and_defers_missing_owner_errors() {
    let (_test, root) = source();
    fs::remove_file(root.join(catalog().as_path())).unwrap();
    fs::write(root.join("CommonModules/Tools/object.yaml"), b"broken: [").unwrap();
    fs::remove_dir_all(root.join("Catalogs/Sample/Forms")).unwrap();
    fs::remove_dir_all(root.join("CommonModules/Tools/Ext")).unwrap();
    let reader = SourceReader::open(&root).unwrap();
    assert_eq!(reader.owner_paths().count(), 3);
    assert_eq!(reader.configuration().declarations.len(), 1);
    assert_eq!(
        reader.configuration().declarations[0].kind,
        MetadataKind::Configuration
    );
    assert_eq!(
        reader.read_owner(&catalog()).unwrap_err().code,
        "source_file_missing"
    );
    assert_eq!(
        reader
            .read_owner(&SourcePath::parse("CommonModules/Tools/object.yaml").unwrap())
            .unwrap_err()
            .code,
        "source_yaml_invalid"
    );
}

#[test]
fn typed_declarations_keep_inline_and_external_parents_and_native_identity() {
    let (_test, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let snapshot = reader.read_owner(&catalog()).unwrap();
    assert_eq!(
        snapshot
            .declarations
            .iter()
            .map(|node| node.name.as_str())
            .collect::<Vec<_>>(),
        ["Sample", "Description", "Items", "Count", "Main"]
    );
    let nodes = &snapshot.declarations;
    assert_eq!(
        nodes[0].parent.as_ref(),
        Some(&reader.configuration().declarations[0].uuid)
    );
    assert_eq!(nodes[1].parent.as_ref(), Some(&nodes[0].uuid));
    assert_eq!(nodes[2].parent.as_ref(), Some(&nodes[0].uuid));
    assert_eq!(nodes[3].parent.as_ref(), Some(&nodes[2].uuid));
    assert_eq!(nodes[4].parent.as_ref(), Some(&nodes[0].uuid));
    assert_eq!(
        nodes[4].native_path.as_str(),
        "Catalogs/Sample/Forms/Main.xml"
    );
    assert_eq!(nodes[3].native_path.as_str(), "Catalogs/Sample.xml");
    let text = fs::read_to_string(root.join(catalog().as_path())).unwrap();
    assert_eq!(
        snapshot.source_sha256,
        files::hex(&Sha256::digest(text.as_bytes()))
    );
    for node in nodes {
        assert_eq!(
            text.get(node.name_range.clone()),
            Some(serde_json::to_string(&node.name).unwrap().as_str())
        );
        assert!(
            text.get(node.range.clone())
                .unwrap()
                .contains(node.uuid.as_str())
        );
    }
}

#[test]
fn coordinates_follow_unicode_crlf_comments_and_actual_in_memory_bytes() {
    let (_test, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let original = reader.read_owner(&catalog()).unwrap();
    let mut family = original.source.clone();
    let super::super::Section::Children(children) =
        &mut family.document.descriptor.object.sections[2]
    else {
        panic!("children")
    };
    let super::super::Child::Inline { object } = &mut children[0] else {
        panic!("inline attribute")
    };
    let super::super::Section::Properties(properties) = &mut object.sections[0] else {
        panic!("properties")
    };
    properties[0] = super::super::Property::Typed {
        name: PropertyName::Name,
        value: PropertyValue::Text("ОписаниеЁ🦀".into()),
    };
    let text = format!(
        "# Count: «не объявление»\r\n{}",
        family.to_yaml().unwrap().replace('\n', "\r\n")
    );
    let overlay = reader.parse_owner(&catalog(), &text).unwrap();
    assert_ne!(overlay.source_sha256, original.source_sha256);
    assert_eq!(overlay.manifest_sha256, original.manifest_sha256);
    assert_eq!(overlay.declarations[1].name, "ОписаниеЁ🦀");
    for declaration in &overlay.declarations {
        assert_eq!(
            text.get(declaration.name_range.clone()),
            Some(serde_json::to_string(&declaration.name).unwrap().as_str())
        );
    }
    assert_eq!(reader.read_owner(&catalog()).unwrap(), original);
}

#[test]
fn each_requested_owner_read_observes_changes_without_generating_xml() {
    let (_test, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let before = reader.read_owner(&catalog()).unwrap();
    let path = root.join(catalog().as_path());
    let changed = format!(
        "# unrelated formatting\n{}",
        fs::read_to_string(&path).unwrap()
    );
    fs::write(path, &changed).unwrap();
    let after = reader.read_owner(&catalog()).unwrap();
    assert_ne!(before.source_sha256, after.source_sha256);
    assert_eq!(before.source, after.source);
    assert_eq!(
        after.declarations[0].range.start,
        before.declarations[0].range.start + "# unrelated formatting\n".len()
    );
    assert!(!root.join("Configuration.xml").exists());
}

#[test]
fn coordinates_also_resolve_flow_mappings_with_different_field_order() {
    let (_test, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let original = reader.read_owner(&catalog()).unwrap();
    let text = serde_json::to_string(&original.source).unwrap();
    let snapshot = reader.parse_owner(&catalog(), &text).unwrap();
    assert_eq!(snapshot.source, original.source);
    for node in &snapshot.declarations {
        assert_eq!(
            text.get(node.name_range.clone()),
            Some(serde_json::to_string(&node.name).unwrap().as_str())
        );
        assert!(
            text.get(node.range.clone())
                .unwrap()
                .contains(node.uuid.as_str())
        );
    }
}

#[test]
fn changed_root_composition_requires_reopening_its_snapshot() {
    let (_test, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let mut changed = reader.configuration().source.clone();
    let super::super::Section::Children(children) =
        &mut changed.document.descriptor.object.sections[1]
    else {
        panic!("children")
    };
    children.pop();
    let path = SourcePath::parse("configuration.yaml").unwrap();
    assert_eq!(
        reader
            .parse_owner(&path, &changed.to_yaml().unwrap())
            .unwrap_err()
            .code,
        "source_declaration_mismatch"
    );
    fs::write(root.join(path.as_path()), changed.to_yaml().unwrap()).unwrap();
    assert_eq!(
        SourceReader::open(&root).err().unwrap().code,
        "source_descriptor_undeclared"
    );
}

#[test]
fn malformed_current_yaml_has_no_fallback_to_native_xml_or_old_owner() {
    let (_test, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    reader.read_owner(&catalog()).unwrap();
    fs::write(root.join("Catalogs/Sample.xml"), b"cached native XML").unwrap();
    fs::write(
        root.join(catalog().as_path()),
        b"document: null\ndocument: null\n",
    )
    .unwrap();
    let error = reader.read_owner(&catalog()).unwrap_err();
    assert_eq!(error.code, "source_yaml_key_duplicate");
    assert_eq!(error.location[0], catalog().as_str());
    assert_eq!(
        reader
            .parse_owner(&catalog(), "\u{feff}document: null\n")
            .unwrap_err()
            .code,
        "source_yaml_bom_forbidden"
    );
}

#[test]
fn source_map_identities_and_nested_declarations_are_required_for_each_read() {
    let (_test, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let original = reader.read_owner(&catalog()).unwrap().source;
    let mut changed = original.clone();
    changed.document.descriptor.object.uuid =
        NativeUuid::parse("90000000-0000-4000-8000-000000000001").unwrap();
    assert_eq!(
        reader
            .parse_owner(&catalog(), &changed.to_yaml().unwrap())
            .unwrap_err()
            .code,
        "source_identity_mismatch"
    );
    changed = original.clone();
    changed.nested.clear();
    assert_eq!(
        reader
            .parse_owner(&catalog(), &changed.to_yaml().unwrap())
            .unwrap_err()
            .code,
        "source_identity_mismatch"
    );
    changed = original;
    let super::super::Section::Properties(properties) =
        &mut changed.nested[0].descriptor.object.sections[0]
    else {
        panic!("properties")
    };
    properties[0] = super::super::Property::Typed {
        name: PropertyName::Name,
        value: PropertyValue::Text("WrongForm".into()),
    };
    assert_eq!(
        reader
            .parse_owner(&catalog(), &changed.to_yaml().unwrap())
            .unwrap_err()
            .code,
        "source_declaration_mismatch"
    );
    let unknown = SourcePath::parse("Catalogs/Undeclared/object.yaml").unwrap();
    assert_eq!(
        reader.parse_owner(&unknown, "broken: [").unwrap_err().code,
        "source_descriptor_undeclared"
    );
}

#[test]
fn reopening_binds_a_new_manifest_without_mislabeling_old_snapshots() {
    let (_test, root) = source();
    let before = SourceReader::open(&root).unwrap();
    fs::write(
        root.join(files::MANIFEST),
        format!(
            " \n{}",
            fs::read_to_string(root.join(files::MANIFEST)).unwrap()
        ),
    )
    .unwrap();
    let after = SourceReader::open(&root).unwrap();
    assert_ne!(
        before.configuration().manifest_sha256,
        after.configuration().manifest_sha256
    );
    assert_eq!(
        before.read_owner(&catalog()).unwrap().manifest_sha256,
        before.configuration().manifest_sha256
    );
    assert_eq!(
        after.read_owner(&catalog()).unwrap().manifest_sha256,
        after.configuration().manifest_sha256
    );
}

#[test]
fn undeclared_manifest_owners_and_missing_references_cannot_activate_metadata() {
    for missing in [true, false] {
        let (_test, root) = source();
        let reader = SourceReader::open(&root).unwrap();
        let mut manifest = reader.manifest().clone();
        if missing {
            manifest.files.retain(|file| !matches!(&file.role, FileRole::Descriptor { source, .. } if source == &catalog()));
        } else {
            let file = manifest
                .files
                .iter_mut()
                .find(|file| file.path.as_str() == "outside.xml")
                .unwrap();
            file.role = FileRole::Descriptor {
                source: SourcePath::parse("Catalogs/Undeclared/object.yaml").unwrap(),
                uuid: NativeUuid::parse("90000000-0000-4000-8000-000000000001").unwrap(),
            };
        }
        write_manifest(&root, &manifest);
        let error = SourceReader::open(&root).err().unwrap();
        assert_eq!(
            error.code,
            if missing {
                "source_descriptor_missing"
            } else {
                "source_descriptor_undeclared"
            }
        );
    }
}

#[cfg(unix)]
#[test]
fn root_manifest_owner_and_owner_parent_aliases_are_rejected() {
    use std::os::unix::fs::symlink;
    for selected in ["root", "manifest", "owner", "parent"] {
        let (test, root) = source();
        let reader = SourceReader::open(&root).unwrap();
        let error = if selected == "root" {
            let alias = test.0.join("alias");
            symlink(&root, &alias).unwrap();
            SourceReader::open(&alias).err().unwrap()
        } else if selected == "manifest" {
            let original = root.join(files::MANIFEST);
            let actual = root.join("manifest.actual");
            fs::rename(&original, &actual).unwrap();
            symlink(&actual, &original).unwrap();
            SourceReader::open(&root).err().unwrap()
        } else {
            let original = if selected == "owner" {
                root.join(catalog().as_path())
            } else {
                root.join("Catalogs/Sample")
            };
            let actual = test.0.join("actual");
            fs::rename(&original, &actual).unwrap();
            symlink(&actual, &original).unwrap();
            reader.read_owner(&catalog()).unwrap_err()
        };
        assert_eq!(
            error.code,
            if selected == "root" {
                "source_directory_required"
            } else {
                "source_entry_unsupported"
            }
        );
    }
}
