use std::{
    fs,
    path::{Path, PathBuf},
};

use eska::project::source_format::{
    AcceptedBase, Child, Descriptor, MetadataNode, ObjectSource, PermissionReason, Property,
    PropertyName, PropertyValue, Section, accept_source, export_designer, export_protected,
    import_designer,
};

use super::baseline::{fixture, tree, uuid};
use crate::support::TestDir;

/// Exercise each adverse edit in an isolated source tree and an independently accepted base.
fn prepare(root: &Path) -> (PathBuf, AcceptedBase) {
    let source = root.join("source");
    import_designer(&fixture(root), &source).unwrap();
    let base = root.join("accepted");
    let reference = accept_source(&source, &base, &[]).unwrap();
    (source, AcceptedBase::open(&base, &reference).unwrap())
}

/// Use the documented writer for valid edits; raw identity corruption is tested separately.
fn edit(root: &Path, path: &str, change: impl FnOnce(&mut ObjectSource)) {
    let path = root.join(path);
    let mut object = ObjectSource::from_yaml(&fs::read_to_string(&path).unwrap()).unwrap();
    change(&mut object);
    fs::write(path, object.to_yaml().unwrap()).unwrap();
}

/// Locate an inline part by its fixture UUID, never by a localized metadata caption.
fn node_mut(node: &mut MetadataNode, target: char) -> Option<&mut MetadataNode> {
    if node.uuid == uuid(target) {
        return Some(node);
    }
    for section in &mut node.sections {
        if let Section::Children(children) = section {
            for child in children {
                if let Child::Inline { object } = child
                    && let Some(node) = node_mut(object, target)
                {
                    return Some(node);
                }
            }
        }
    }
    None
}

/// Change one existing typed property without constructing unsupported metadata structure.
fn property(node: &mut MetadataNode, key: PropertyName, text: &str) {
    for section in &mut node.sections {
        if let Section::Properties(properties) = section {
            for entry in properties {
                if let Property::Typed { name, value } = entry
                    && *name == key
                {
                    *value = PropertyValue::Text(text.into());
                    return;
                }
            }
        }
    }
    panic!("fixture property missing");
}

/// Parse native output to assert behavior, rather than merely checking that a directory exists.
fn descriptor(root: &Path, path: &str) -> Descriptor {
    Descriptor::from_xml(&fs::read_to_string(root.join(path)).unwrap()).unwrap()
}

#[test]
fn clean_protected_output_matches_native_export_and_does_not_rewrite_a_generation() {
    let test = TestDir::new();
    let (source, base) = prepare(&test.0);
    let before = tree(&source);
    let control = test.0.join("control");
    export_designer(&source, &control).unwrap();
    let output = test.0.join("generations");
    let first = export_protected(&source, &base, &output, true).unwrap();
    assert!(first.diagnostics.is_empty());
    assert_eq!(tree(&first.xml), tree(&control));
    let modified = fs::metadata(first.xml.join("Catalogs/Sample.xml"))
        .unwrap()
        .modified()
        .unwrap();
    let second = export_protected(&source, &base, &output, true).unwrap();
    assert_eq!(first.generation, second.generation);
    assert_eq!(
        modified,
        fs::metadata(second.xml.join("Catalogs/Sample.xml"))
            .unwrap()
            .modified()
            .unwrap()
    );
    assert_eq!(tree(&source), before);
}

#[test]
fn mixed_permissions_restore_the_parent_and_apply_allowed_inline_and_module_edits() {
    let test = TestDir::new();
    let (source, base) = prepare(&test.0);
    edit(&source, "Catalogs/Sample/object.yaml", |family| {
        let node = &mut family.document.descriptor.object;
        property(node, PropertyName::Comment, "Forbidden parent comment");
        property(
            node_mut(node, '3').unwrap(),
            PropertyName::Comment,
            "Allowed inline comment",
        );
        property(
            node_mut(node, '4').unwrap(),
            PropertyName::Comment,
            "Support removed comment",
        );
    });
    let module = source.join("CommonModules/Tools/Ext/Module.bsl");
    let mut bytes = fs::read(&module).unwrap();
    bytes.extend_from_slice(b"\r\n// allowed change\r\n");
    fs::write(&module, &bytes).unwrap();
    let before = tree(&source);
    let report = export_protected(&source, &base, &test.0.join("generations"), false).unwrap();
    let mut native = descriptor(&report.xml, "Catalogs/Sample.xml");
    assert_eq!(
        native.object.property(PropertyName::Comment),
        Some(&PropertyValue::Empty)
    );
    assert_eq!(
        node_mut(&mut native.object, '3')
            .unwrap()
            .property(PropertyName::Comment),
        Some(&PropertyValue::Text("Allowed inline comment".into()))
    );
    assert_eq!(
        node_mut(&mut native.object, '4')
            .unwrap()
            .property(PropertyName::Comment),
        Some(&PropertyValue::Text("Support removed comment".into()))
    );
    assert_eq!(
        fs::read(report.xml.join("CommonModules/Tools/Ext/Module.bsl")).unwrap(),
        bytes
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|item| item.uuid == Some(uuid('2'))
                && item.reason == PermissionReason::NativeRestriction)
    );
    assert_eq!(tree(&source), before);
}

#[test]
fn deleted_or_physically_moved_parents_are_rebuilt_without_losing_other_allowed_edits() {
    for fault in ["delete", "rename", "move", "empty"] {
        let test = TestDir::new();
        let (source, base) = prepare(&test.0);
        let control = test.0.join("control");
        export_designer(&source, &control).unwrap();
        if fault == "empty" {
            fs::remove_dir_all(&source).unwrap();
            fs::create_dir(&source).unwrap();
        } else {
            let path = source.join("Catalogs/Sample");
            match fault {
                "delete" => fs::remove_dir_all(path).unwrap(),
                "rename" => fs::rename(path, source.join("Catalogs/Renamed")).unwrap(),
                _ => fs::rename(path, source.join("MovedCatalog")).unwrap(),
            }
            edit(&source, "CommonModules/Tools/object.yaml", |family| {
                property(
                    &mut family.document.descriptor.object,
                    PropertyName::Comment,
                    "Retained elsewhere",
                );
            });
        }
        let before = tree(&source);
        let report = export_protected(&source, &base, &test.0.join("generations"), false).unwrap();
        assert_eq!(
            fs::read(report.xml.join("Catalogs/Sample.xml")).unwrap(),
            fs::read(control.join("Catalogs/Sample.xml")).unwrap()
        );
        assert_eq!(
            fs::read(report.xml.join("Catalogs/Sample/Forms/Main/Ext/Form.xml")).unwrap(),
            fs::read(control.join("Catalogs/Sample/Forms/Main/Ext/Form.xml")).unwrap()
        );
        if fault == "empty" {
            assert_eq!(tree(&report.xml), tree(&control));
        } else {
            assert_eq!(
                descriptor(&report.xml, "CommonModules/Tools.xml")
                    .object
                    .property(PropertyName::Comment),
                Some(&PropertyValue::Text("Retained elsewhere".into()))
            );
        }
        assert!(!report.diagnostics.is_empty());
        assert_eq!(tree(&source), before);
    }
}

#[test]
fn working_identity_never_unlocks_a_part_and_logical_name_is_restored() {
    for identity in ["60000000-0000-4000-8000-000000000001", "invalid-uuid"] {
        let test = TestDir::new();
        let (source, base) = prepare(&test.0);
        edit(&source, "Catalogs/Sample/object.yaml", |family| {
            property(
                &mut family.document.descriptor.object,
                PropertyName::Name,
                "Renamed",
            );
            property(
                &mut family.document.descriptor.object,
                PropertyName::Comment,
                "Forbidden",
            );
            property(
                node_mut(&mut family.document.descriptor.object, '3').unwrap(),
                PropertyName::Comment,
                "Retained child",
            );
        });
        let path = source.join("Catalogs/Sample/object.yaml");
        let content = fs::read_to_string(&path)
            .unwrap()
            .replacen(uuid('2').as_str(), identity, 1);
        fs::write(&path, content).unwrap();
        let report = export_protected(&source, &base, &test.0.join("generations"), false).unwrap();
        let mut native = descriptor(&report.xml, "Catalogs/Sample.xml");
        assert_eq!(native.object.uuid, uuid('2'));
        assert_eq!(
            native.object.property(PropertyName::Name),
            Some(&PropertyValue::Text("Sample".into()))
        );
        assert_eq!(
            native.object.property(PropertyName::Comment),
            Some(&PropertyValue::Empty)
        );
        assert_eq!(
            node_mut(&mut native.object, '3')
                .unwrap()
                .property(PropertyName::Comment),
            Some(&PropertyValue::Text("Retained child".into()))
        );
    }
}

#[test]
fn strict_exclusion_and_invalid_allowed_values_preserve_previous_complete_generation() {
    let test = TestDir::new();
    let (source, base) = prepare(&test.0);
    let output = test.0.join("generations");
    let first = export_protected(&source, &base, &output, true).unwrap();
    let before = tree(&output);
    edit(&source, "Catalogs/Sample/object.yaml", |family| {
        property(
            &mut family.document.descriptor.object,
            PropertyName::Comment,
            "Forbidden",
        );
    });
    let failure = export_protected(&source, &base, &output, true).unwrap_err();
    assert_eq!(failure.error.code, "source_changes_excluded");
    assert!(!failure.diagnostics.is_empty());
    assert_eq!(tree(&output), before);
    let path = source.join("CommonModules/Tools/object.yaml");
    let content = fs::read_to_string(&path).unwrap();
    let changed = content.replace("\"Module <fixture>\"", "null");
    assert_ne!(content, changed);
    fs::write(path, changed).unwrap();
    assert_eq!(
        export_protected(&source, &base, &output, false)
            .unwrap_err()
            .error
            .code,
        "source_property_invalid"
    );
    assert_eq!(tree(&output), before);
    assert!(first.xml.is_dir());
}

#[test]
fn raw_xml_resource_errors_and_policy_edits_cannot_publish_broken_output() {
    let test = TestDir::new();
    let (source, base) = prepare(&test.0);
    let output = test.0.join("generations");
    export_protected(&source, &base, &output, true).unwrap();
    let before = tree(&output);
    fs::write(
        source.join("Ext/ParentConfigurations.bin"),
        b"arbitrary replacement",
    )
    .unwrap();
    fs::remove_file(source.join("Ext/ParentConfigurations/SampleConfiguration.cf")).unwrap();
    let protected = export_protected(&source, &base, &output, false).unwrap();
    assert!(
        protected
            .xml
            .join("Ext/ParentConfigurations/SampleConfiguration.cf")
            .is_file()
    );
    let complete = tree(&output);
    fs::write(
        source.join("Catalogs/Sample/Forms/Main/Ext/Form.xml"),
        "<Form><broken></Form>",
    )
    .unwrap();
    assert_eq!(
        export_protected(&source, &base, &output, false)
            .unwrap_err()
            .error
            .code,
        "source_resource_xml_invalid"
    );
    assert_eq!(tree(&output), complete);
    assert!(complete.len() > before.len());
}

/// ESKA freeze restricts editable descendants and external form payloads without changing support.
#[test]
fn frozen_descendants_restore_values_and_resources_from_the_accepted_base() {
    let test = TestDir::new();
    let (source, _) = prepare(&test.0);
    let accepted = test.0.join("frozen");
    let reference = accept_source(&source, &accepted, &[uuid('2')]).unwrap();
    let base = AcceptedBase::open(&accepted, &reference).unwrap();
    let form = "Catalogs/Sample/Forms/Main/Ext/Form.xml";
    let original = fs::read(source.join(form)).unwrap();
    edit(&source, "Catalogs/Sample/object.yaml", |family| {
        property(
            node_mut(&mut family.document.descriptor.object, '3').unwrap(),
            PropertyName::Comment,
            "Frozen child",
        );
    });
    fs::write(source.join(form), "invalid manual edit of frozen XML").unwrap();
    let report = export_protected(&source, &base, &test.0.join("generations"), false).unwrap();
    assert_eq!(fs::read(report.xml.join(form)).unwrap(), original);
    assert!(report.diagnostics.iter().any(|item| item.uuid == Some(uuid('3')) && item.reason == PermissionReason::EskaFrozen));
    assert!(report.diagnostics.iter().any(|item| item.uuid == Some(uuid('7')) && item.reason == PermissionReason::EskaFrozen));
}

/// Concurrent consumers keep reading the immutable old generation while a new one is built.
#[test]
fn readers_keep_the_previous_generation_during_a_permitted_update() {
    let test = TestDir::new();
    let (source, base) = prepare(&test.0);
    let output = test.0.join("generations");
    let first = export_protected(&source, &base, &output, true).unwrap();
    let path = first.xml.join("CommonModules/Tools.xml");
    let expected = fs::read(&path).unwrap();
    edit(&source, "CommonModules/Tools/object.yaml", |family| {
        property(
            &mut family.document.descriptor.object,
            PropertyName::Comment,
            "New generation",
        );
    });
    std::thread::scope(|scope| {
        let reader = scope.spawn(|| {
            for _ in 0..100 {
                assert_eq!(fs::read(&path).unwrap(), expected);
            }
        });
        let next = export_protected(&source, &base, &output, true).unwrap();
        assert_ne!(first.generation, next.generation);
        assert_eq!(
            descriptor(&next.xml, "CommonModules/Tools.xml")
                .object
                .property(PropertyName::Comment),
            Some(&PropertyValue::Text("New generation".into()))
        );
        reader.join().unwrap();
    });
    assert_eq!(fs::read(path).unwrap(), expected);
}

/// Preserved binary code is not a plaintext module merely because its owner is editable.
#[test]
fn opaque_binary_modules_are_restored_and_unmanaged_output_is_not_claimed() {
    let test = TestDir::new();
    let input = fixture(&test.0);
    fs::write(
        input.join("CommonModules/Tools/Ext/Module.bin"),
        b"accepted opaque binary code",
    )
    .unwrap();
    let source = test.0.join("source");
    import_designer(&input, &source).unwrap();
    let accepted = test.0.join("accepted");
    let reference = accept_source(&source, &accepted, &[]).unwrap();
    let base = AcceptedBase::open(&accepted, &reference).unwrap();
    fs::write(
        source.join("CommonModules/Tools/Ext/Module.bin"),
        b"unsupported opaque edit",
    )
    .unwrap();
    let unrelated = test.0.join("unrelated");
    fs::create_dir(&unrelated).unwrap();
    fs::write(unrelated.join("keep.txt"), b"user content").unwrap();
    let before = tree(&unrelated);
    assert_eq!(
        export_protected(&source, &base, &unrelated, false)
            .unwrap_err()
            .error
            .code,
        "source_generation_store_invalid"
    );
    assert_eq!(tree(&unrelated), before);
    let report = export_protected(&source, &base, &test.0.join("generations"), false).unwrap();
    assert_eq!(
        fs::read(report.xml.join("CommonModules/Tools/Ext/Module.bin")).unwrap(),
        b"accepted opaque binary code"
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|item| item.reason == PermissionReason::UnsupportedOperation)
    );
}

/// A user-edited cache is an error; it cannot become the authoritative replacement source.
#[test]
fn corrupted_materialized_xml_is_refused_without_replacing_it_or_the_source() {
    let test = TestDir::new();
    let (source, base) = prepare(&test.0);
    let output = test.0.join("generations");
    let report = export_protected(&source, &base, &output, true).unwrap();
    let before = tree(&source);
    fs::write(report.xml.join("Configuration.xml"), b"corrupt cached XML").unwrap();
    assert_eq!(
        export_protected(&source, &base, &output, true)
            .unwrap_err()
            .error
            .code,
        "source_generation_corrupt"
    );
    assert_eq!(
        fs::read(report.xml.join("Configuration.xml")).unwrap(),
        b"corrupt cached XML"
    );
    assert_eq!(tree(&source), before);
}

/// Repeated external descriptions have no deterministic accepted counterpart for permitted edits.
#[test]
fn ambiguous_external_descriptions_fail_before_publishing_any_generation() {
    let test = TestDir::new();
    let (source, base) = prepare(&test.0);
    let path = source.join("Catalogs/Sample/object.yaml");
    let mut family = ObjectSource::from_yaml(&fs::read_to_string(&path).unwrap()).unwrap();
    family.nested.push(family.nested[0].clone());
    fs::write(path, serde_json::to_string(&family).unwrap()).unwrap();
    let output = test.0.join("generations");
    assert_eq!(
        export_protected(&source, &base, &output, false)
            .unwrap_err()
            .error
            .code,
        "source_identity_ambiguous"
    );
    assert!(!output.exists());
}

/// Compare complete native output against the earlier independently verified EF-04 snapshot.
#[test]
#[ignore = "Real protected export; requires explicit playground inputs, base pin and new store"]
fn real_playground_protected_export() {
    let root = PathBuf::from(std::env::var_os("ESKA_TEST_ROOT").unwrap())
        .canonicalize()
        .unwrap();
    let source = PathBuf::from(std::env::var_os("ESKA_FORMAT_INPUT").unwrap())
        .canonicalize()
        .unwrap();
    let base_path = PathBuf::from(std::env::var_os("ESKA_FORMAT_BASE").unwrap())
        .canonicalize()
        .unwrap();
    let control = PathBuf::from(std::env::var_os("ESKA_FORMAT_ORIGINAL").unwrap())
        .canonicalize()
        .unwrap();
    let output = PathBuf::from(std::env::var_os("ESKA_FORMAT_OUTPUT").unwrap());
    for path in [&source, &base_path, &control] {
        assert!(path.starts_with(&root));
    }
    assert!(
        output
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(&root)
    );
    let reference = eska::project::source_format::AcceptedRef {
        sha256: std::env::var("ESKA_FORMAT_REFERENCE").unwrap(),
    };
    let base = AcceptedBase::open(&base_path, &reference).unwrap();
    let report = export_protected(&source, &base, &output, true).unwrap();
    assert!(report.diagnostics.is_empty());
    let mut pending = vec![PathBuf::new()];
    let mut count = 0;
    while let Some(parent) = pending.pop() {
        let mut names = std::collections::BTreeSet::new();
        for entry in fs::read_dir(control.join(&parent)).unwrap() {
            let entry = entry.unwrap();
            names.insert(entry.file_name());
            let path = parent.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                pending.push(path);
            } else {
                assert_eq!(
                    fs::read(control.join(&path)).unwrap(),
                    fs::read(report.xml.join(&path)).unwrap()
                );
                count += 1;
            }
        }
        assert_eq!(
            names,
            fs::read_dir(report.xml.join(parent))
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect()
        );
    }
    println!(
        "{}",
        serde_json::json!({"files":count,"exclusions":report.diagnostics.len(),"generation":report.generation,"xml":report.xml})
    );
}
