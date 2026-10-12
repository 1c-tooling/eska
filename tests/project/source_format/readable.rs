use std::{fs, path::Path};

use crate::support::TestDir;
use eska::project::source_format::{
    AcceptedBase, InputAuthority, LayoutPublication, ObjectSource, Operation, OriginTexts,
    PropertyName, PropertyValue, SourcePath, SourceReader, accept_source, export_designer,
    import_designer, import_designer_readable, prepare_accepted_readable_layout,
    prepare_property_edit, prepare_protected, prepare_readable_layout,
};

/// Compare complete native trees including resource and supplier bytes without platform claims.
fn equal_native(left: &Path, right: &Path) {
    assert_eq!(
        fs::read_dir(left).unwrap().count(),
        fs::read_dir(right).unwrap().count()
    );
    for entry in fs::read_dir(left).unwrap() {
        let entry = entry.unwrap();
        let path = right.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            equal_native(&entry.path(), &path);
        } else {
            assert_eq!(fs::read(entry.path()).unwrap(), fs::read(path).unwrap());
        }
    }
}

/// Working/accepted migrations use the same envelope, preserve policy, and bind a new explicit pin.
#[test]
fn readable_source_and_pinned_migration_preserve_native_bytes_and_policy() {
    let test = TestDir::new();
    let native = super::baseline::fixture(&test.0);
    let legacy = test.0.join("legacy");
    import_designer(&native, &legacy).unwrap();
    let frozen = super::baseline::uuid('4');
    let storage = test.0.join("accepted");
    let reference = accept_source(&legacy, &storage, &[frozen]).unwrap();
    let original = AcceptedBase::open(&storage, &reference).unwrap();
    let old_native = test.0.join("old-native");
    export_designer(&legacy, &old_native).unwrap();
    let plan = prepare_readable_layout(&legacy).unwrap();
    let source = test.0.join("readable");
    assert_eq!(plan.publish(&source).unwrap(), LayoutPublication::Created);
    assert_eq!(plan.publish(&source).unwrap(), LayoutPublication::Unchanged);
    assert!(prepare_readable_layout(&source).unwrap().is_empty());
    let accepted_plan = prepare_accepted_readable_layout(&original).unwrap();
    let accepted = test.0.join("new-accepted");
    accepted_plan.publish(&accepted).unwrap();
    let base = AcceptedBase::open(&accepted, accepted_plan.accepted_reference()).unwrap();
    assert_ne!(accepted_plan.accepted_reference(), &reference);
    assert!(prepare_accepted_readable_layout(&base).unwrap().is_empty());
    for id in ['1', '2', '3', '4', '5', '7'] {
        for operation in [
            Operation::Property(PropertyName::Comment),
            Operation::Module,
            Operation::Create,
        ] {
            let old = original.permission(&super::baseline::uuid(id), operation);
            let new = base.permission(&super::baseline::uuid(id), operation);
            assert_eq!(
                (old.allowed, old.reason, old.state),
                (new.allowed, new.reason, new.state)
            );
        }
    }
    let new_native = test.0.join("new-native");
    export_designer(&source, &new_native).unwrap();
    equal_native(&old_native, &new_native);
    let fresh = test.0.join("fresh");
    import_designer_readable(&native, &fresh).unwrap();
    let fresh_native = test.0.join("fresh-native");
    export_designer(&fresh, &fresh_native).unwrap();
    equal_native(&old_native, &fresh_native);
    let unchanged_source = test.0.join("original-accepted-source");
    original.restore(&unchanged_source).unwrap();
    assert_eq!(
        super::baseline::tree(&legacy),
        super::baseline::tree(&unchanged_source)
    );
}

/// Shared editing and source coordinates select direct/inline properties under independently pinned permissions.
#[test]
fn readable_edits_and_origins_preserve_comments_neighbors_and_support() {
    let test = TestDir::new();
    let native = super::baseline::fixture(&test.0);
    let source = test.0.join("source");
    import_designer_readable(&native, &source).unwrap();
    let storage = test.0.join("accepted");
    let reference = accept_source(&source, &storage, &[super::baseline::uuid('4')]).unwrap();
    let base = AcceptedBase::open(&storage, &reference).unwrap();
    let owner = SourcePath::parse("Catalogs/Sample/object.yaml").unwrap();
    let file = source.join(owner.as_path());
    let original = fs::read_to_string(&file)
        .unwrap()
        .replace(
            "\"Comment\": \"Editable part\"",
            "\"Comment\": 'Editable part' # editable",
        )
        .replace('\n', "\r\n");
    fs::write(&file, &original).unwrap();
    let uuid = super::baseline::uuid('3');
    let edit = prepare_property_edit(
        &source,
        &base,
        &uuid,
        PropertyName::Comment,
        PropertyValue::Text("Changed part".into()),
    )
    .unwrap();
    assert_eq!(
        edit.updated(),
        original.replace("'Editable part'", "'Changed part'")
    );
    edit.publish(&base).unwrap();
    for id in ['2', '4', '5'] {
        assert!(
            prepare_property_edit(
                &source,
                &base,
                &super::baseline::uuid(id),
                PropertyName::Comment,
                PropertyValue::Text("Forbidden".into())
            )
            .is_err()
        );
    }
    let reader = SourceReader::open(&source).unwrap();
    let snapshot = reader.read_owner(&owner).unwrap();
    assert!(
        snapshot.declarations.iter().any(|item| item.uuid == uuid
            && &edit.updated()[item.name_range.clone()] == "\"Description\"")
    );
    let generation = prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
    assert_eq!(generation.source_map().source_schema_version, 2);
    let native_path = SourcePath::parse("Catalogs/Sample.xml").unwrap();
    let bytes = fs::read(generation.xml().join(native_path.as_path())).unwrap();
    let native_text = std::str::from_utf8(&bytes).unwrap();
    let start = native_text.find("Changed part").unwrap();
    let accepted_source = test.0.join("accepted-source");
    base.restore(&accepted_source).unwrap();
    let accepted_text = fs::read_to_string(accepted_source.join(owner.as_path())).unwrap();
    let working = fs::read_to_string(&file).unwrap();
    let location = generation
        .source_map()
        .file(&native_path)
        .unwrap()
        .locate(
            &bytes,
            start..start + 12,
            Some(OriginTexts {
                accepted: &accepted_text,
                working: Some(&working),
            }),
        )
        .unwrap();
    assert_eq!(location.authority, InputAuthority::Working);
    assert_eq!(&working[location.range.unwrap()], "'Changed part'");
    let preserved_start = native_text.find("FutureProperty").unwrap();
    let location = generation
        .source_map()
        .file(&native_path)
        .unwrap()
        .locate(
            &bytes,
            preserved_start..preserved_start + 14,
            Some(OriginTexts {
                accepted: &accepted_text,
                working: Some(&working),
            }),
        )
        .unwrap();
    assert_eq!(location.authority, InputAuthority::Accepted);
    assert!(accepted_text[location.range.unwrap()].contains("FutureProperty"));
}

/// Raw YAML values and identity edits cannot grant permission or bypass the accepted envelope.
#[test]
fn readable_raw_edits_restore_restrictions_and_report_the_native_property() {
    let directory = TestDir::new();
    let native = super::baseline::fixture(&directory.0);
    let source = directory.0.join("source");
    import_designer_readable(&native, &source).unwrap();
    let storage = directory.0.join("accepted");
    let reference = accept_source(&source, &storage, &[super::baseline::uuid('4')]).unwrap();
    let base = AcceptedBase::open(&storage, &reference).unwrap();
    let path = source.join("Catalogs/Sample/object.yaml");
    let mut wire: serde_json::Value =
        serde_json::to_value(ObjectSource::from_yaml(&fs::read_to_string(&path).unwrap()).unwrap())
            .unwrap();
    let object = &mut wire["document"]["descriptor"]["object"];
    object["properties"]["Comment"] = "Forbidden catalog".into();
    object["children"][1]["object"]["properties"]["Comment"] = "Forbidden frozen".into();
    object["uuid"] = "99000000-0000-4000-8000-000000000001".into();
    fs::write(&path, serde_json::to_string_pretty(&wire).unwrap()).unwrap();
    let restored =
        prepare_protected(&source, &base, &directory.0.join("generations"), false).unwrap();
    for id in ['2', '4'] {
        assert!(restored.diagnostics().iter().any(|item| item.uuid.as_ref()
            == Some(&super::baseline::uuid(id))
            && item.operation == Operation::Property(PropertyName::Comment)));
    }
    let result = fs::read_to_string(restored.xml().join("Catalogs/Sample.xml")).unwrap();
    assert!(!result.contains("Forbidden"));
    assert!(prepare_protected(&source, &base, &directory.0.join("strict"), true).is_err());
}

/// Both authorities migrate authored annotations while edits remain pinned to native support.
#[test]
fn annotated_source_and_accepted_migrations_keep_comments_and_supported_edits() {
    let directory = TestDir::new();
    let native = super::baseline::fixture(&directory.0);
    let legacy = directory.0.join("legacy");
    import_designer(&native, &legacy).unwrap();
    let owner = SourcePath::parse("Catalogs/Sample/object.yaml").unwrap();
    let path = legacy.join(owner.as_path());
    let saved = fs::read_to_string(&path).unwrap();
    let annotated = format!("# Family note\n{saved}# Footer note\n")
        .replace(
            "\"value\": \"Editable part\"",
            "\"value\": 'Editable part' # Attribute note",
        )
        .replace('\n', "\r\n");
    assert_ne!(saved, annotated);
    assert_eq!(
        ObjectSource::from_yaml(&saved).unwrap(),
        ObjectSource::from_yaml(&annotated).unwrap()
    );
    fs::write(&path, &annotated).unwrap();
    let before = super::baseline::tree(&legacy);
    let storage = directory.0.join("accepted");
    let reference = accept_source(&legacy, &storage, &[super::baseline::uuid('4')]).unwrap();
    let original = AcceptedBase::open(&storage, &reference).unwrap();
    let accepted_before = super::baseline::tree(&storage);
    let working_plan = prepare_readable_layout(&legacy).unwrap();
    let source = directory.0.join("readable");
    working_plan.publish(&source).unwrap();
    let accepted_plan = prepare_accepted_readable_layout(&original).unwrap();
    let accepted = directory.0.join("new-accepted");
    accepted_plan.publish(&accepted).unwrap();
    let base = AcceptedBase::open(&accepted, accepted_plan.accepted_reference()).unwrap();
    let restored = directory.0.join("restored");
    base.restore(&restored).unwrap();
    assert_eq!(
        super::baseline::tree(&source),
        super::baseline::tree(&restored)
    );
    let text = fs::read_to_string(source.join(owner.as_path())).unwrap();
    assert!(text.starts_with("# Family note\r\n"));
    assert!(text.contains("# Footer note\r\n"));
    assert!(text.contains("# Attribute note\r\n              \"Comment\": 'Editable part'"));
    assert!(!text.replace("\r\n", "").contains('\n'));
    assert!(prepare_readable_layout(&source).unwrap().is_empty());
    assert!(prepare_accepted_readable_layout(&base).unwrap().is_empty());
    let old_native = directory.0.join("old-native");
    let new_native = directory.0.join("new-native");
    export_designer(&legacy, &old_native).unwrap();
    export_designer(&source, &new_native).unwrap();
    equal_native(&old_native, &new_native);
    let edit = prepare_property_edit(
        &source,
        &base,
        &super::baseline::uuid('3'),
        PropertyName::Comment,
        PropertyValue::Text("Changed part".into()),
    )
    .unwrap();
    assert_eq!(
        edit.updated(),
        text.replace("'Editable part'", "'Changed part'")
    );
    edit.publish(&base).unwrap();
    let generation =
        prepare_protected(&source, &base, &directory.0.join("generations"), true).unwrap();
    assert!(generation.diagnostics().is_empty());
    assert_eq!(
        fs::read_to_string(generation.xml().join("Catalogs/Sample.xml")).unwrap(),
        fs::read_to_string(old_native.join("Catalogs/Sample.xml"))
            .unwrap()
            .replace(
                "<Comment>Editable part</Comment>",
                "<Comment>Changed part</Comment>"
            )
    );
    assert_eq!(super::baseline::tree(&legacy), before);
    assert_eq!(super::baseline::tree(&storage), accepted_before);
}

/// Unsafe transport fails during preparation, retaining both authored source and pinned blobs.
#[test]
fn annotated_migration_refuses_mixed_transport_before_publication() {
    let test = TestDir::new();
    let native = super::baseline::fixture(&test.0);
    let source = test.0.join("source");
    import_designer(&native, &source).unwrap();
    let path = source.join("Catalogs/Sample/object.yaml");
    let mixed = format!("# Mixed\r\n{}", fs::read_to_string(&path).unwrap());
    fs::write(&path, mixed).unwrap();
    let storage = test.0.join("accepted");
    let reference = accept_source(&source, &storage, &[]).unwrap();
    let base = AcceptedBase::open(&storage, &reference).unwrap();
    let source_before = super::baseline::tree(&source);
    let accepted_before = super::baseline::tree(&storage);
    assert_eq!(
        prepare_readable_layout(&source).unwrap_err().code,
        "source_layout_trivia_unsupported"
    );
    assert_eq!(
        prepare_accepted_readable_layout(&base).unwrap_err().code,
        "source_layout_trivia_unsupported"
    );
    assert_eq!(super::baseline::tree(&source), source_before);
    assert_eq!(super::baseline::tree(&storage), accepted_before);
}
