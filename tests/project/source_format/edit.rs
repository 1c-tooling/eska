use std::{
    fs,
    path::{Path, PathBuf},
};

use eska::project::source_format::{
    AcceptedBase, Child, Descriptor, LocalizedText, MetadataNode, PropertyName, PropertyValue,
    Section, accept_source, export_protected, import_designer, prepare_property_edit,
    prepare_property_edit_text,
};

use super::baseline::{fixture, tree, uuid};
use crate::support::TestDir;

/// Prepare a mixed-policy fixture with an independent policy pin.
fn prepare(root: &Path, frozen: &[char]) -> (PathBuf, AcceptedBase) {
    let source = root.join("source");
    import_designer(&fixture(root), &source).unwrap();
    let path = root.join("accepted");
    let frozen = frozen.iter().copied().map(uuid).collect::<Vec<_>>();
    let reference = accept_source(&source, &path, &frozen).unwrap();
    (source, AcceptedBase::open(&path, &reference).unwrap())
}

/// Editor previews use supplied bytes and the same policy without accessing a source file.
#[test]
fn editor_property_preview_preserves_current_text_and_never_publishes_it() {
    let test = TestDir::new();
    let (source, base) = prepare(&test.0, &['4']);
    let path = source.join("Catalogs/Sample/object.yaml");
    let saved = fs::read_to_string(&path).unwrap();
    let original = format!("# Я😀\r\n{}", saved.replace('\n', "\r\n"))
        .replace("\"Editable part\"", "'Current text' # keep");
    let preview = prepare_property_edit_text(
        &base,
        &uuid('3'),
        PropertyName::Comment,
        PropertyValue::Text("Changed text".into()),
        &original,
    )
    .unwrap();
    assert_eq!(preview.path().as_str(), "Catalogs/Sample/object.yaml");
    assert_eq!(
        preview.updated(),
        original.replace("'Current text'", "'Changed text'")
    );
    assert!(!preview.is_empty());
    assert_eq!(fs::read_to_string(&path).unwrap(), saved);
    fs::remove_file(&path).unwrap();
    let no_op = prepare_property_edit_text(
        &base,
        &uuid('3'),
        PropertyName::Comment,
        PropertyValue::Text("Current text".into()),
        &original,
    )
    .unwrap();
    assert!(no_op.is_empty());
    assert_eq!(no_op.updated(), original);
    for (id, property, expected) in [
        ('2', PropertyName::Comment, "source_edit_native_restricted"),
        ('4', PropertyName::Comment, "source_edit_frozen"),
        ('3', PropertyName::Name, "source_edit_operation_unsupported"),
    ] {
        assert_eq!(
            prepare_property_edit_text(
                &base,
                &uuid(id),
                property,
                PropertyValue::Text("Changed".into()),
                &original,
            )
            .unwrap_err()
            .code,
            expected
        );
    }
}

/// Inspect a native inline identity independently of its physical position.
fn node(node: &MetadataNode, id: char) -> Option<&MetadataNode> {
    if node.uuid == uuid(id) {
        return Some(node);
    }
    node.sections.iter().find_map(|section| match section {
        Section::Children(children) => children.iter().find_map(|child| match child {
            Child::Inline { object } => self::node(object, id),
            _ => None,
        }),
        _ => None,
    })
}

#[test]
fn scalar_edits_preserve_comments_quotes_eol_and_every_other_file() {
    for (eol, old, new) in [
        ("\n", "\"Editable part\"", "\"Changed part\""),
        ("\r\n", "'Editable part'", "'Changed part'"),
        ("\n", "Editable part", "Changed part"),
    ] {
        let test = TestDir::new();
        let (source, base) = prepare(&test.0, &[]);
        let path = source.join("Catalogs/Sample/object.yaml");
        let original = format!("# owner comment\n{}", fs::read_to_string(&path).unwrap())
            .replace("\"Editable part\"", &format!("{old} # trailing comment"))
            .replace('\n', eol);
        fs::write(&path, &original).unwrap();
        let before = tree(&source);
        let plan = prepare_property_edit(
            &source,
            &base,
            &uuid('3'),
            PropertyName::Comment,
            PropertyValue::Text("Changed part".into()),
        )
        .unwrap();
        assert_eq!(plan.path().as_str(), "Catalogs/Sample/object.yaml");
        assert_eq!(plan.updated(), original.replace(old, new));
        plan.publish(&base).unwrap();
        let mut expected = before;
        expected.insert(
            "Catalogs/Sample/object.yaml".into(),
            Some(plan.updated().as_bytes().to_vec()),
        );
        assert_eq!(tree(&source), expected);
        let output = export_protected(&source, &base, &test.0.join("generations"), true).unwrap();
        assert!(
            fs::read_to_string(output.xml.join("Catalogs/Sample.xml"))
                .unwrap()
                .contains("Changed part")
        );
    }
}

#[test]
fn no_op_keeps_original_bytes_and_timestamp() {
    let test = TestDir::new();
    let (source, base) = prepare(&test.0, &[]);
    let path = source.join("Catalogs/Sample/object.yaml");
    let original = fs::read_to_string(&path)
        .unwrap()
        .replace("\"Editable part\"", "'Editable part' # keep");
    fs::write(&path, &original).unwrap();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    let plan = prepare_property_edit(
        &source,
        &base,
        &uuid('3'),
        PropertyName::Comment,
        PropertyValue::Text("Editable part".into()),
    )
    .unwrap();
    assert!(plan.is_empty());
    assert_eq!(plan.updated(), original);
    plan.publish(&base).unwrap();
    assert_eq!(modified, fs::metadata(path).unwrap().modified().unwrap());
}

#[test]
fn empty_nil_and_text_transitions_retain_comments_and_native_state() {
    let test = TestDir::new();
    let (source, base) = prepare(&test.0, &[]);
    let path = source.join("Catalogs/Sample/object.yaml");
    for value in [
        PropertyValue::Text("A & B".into()),
        PropertyValue::Empty,
        PropertyValue::Nil,
        PropertyValue::Text("New text".into()),
    ] {
        let original = fs::read_to_string(&path).unwrap();
        fs::write(
            &path,
            original
                .replace("\"A & B\"", "\"A & B\" # value comment")
                .replace("value: A & B", "value: A & B # value comment")
                .replace("'A & B'", "'A & B' # value comment"),
        )
        .unwrap();
        let plan = prepare_property_edit(
            &source,
            &base,
            &uuid('4'),
            PropertyName::Comment,
            value.clone(),
        )
        .unwrap();
        plan.publish(&base).unwrap();
        let output = export_protected(&source, &base, &test.0.join("generations"), true).unwrap();
        let descriptor = Descriptor::from_xml(
            &fs::read_to_string(output.xml.join("Catalogs/Sample.xml")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            node(&descriptor.object, '4')
                .unwrap()
                .property(PropertyName::Comment),
            Some(&value)
        );
        if !matches!(value, PropertyValue::Text(ref s) if s == "A & B") {
            assert!(
                fs::read_to_string(&path)
                    .unwrap()
                    .contains("# value comment")
            );
        }
    }
}

#[test]
fn localized_edits_preserve_other_languages_and_refuse_composition_changes() {
    let test = TestDir::new();
    let input = fixture(&test.0);
    fs::remove_file(input.join("Ext/ParentConfigurations.bin")).unwrap();
    fs::remove_dir_all(input.join("Ext/ParentConfigurations")).unwrap();
    let source = test.0.join("source");
    import_designer(&input, &source).unwrap();
    let path = test.0.join("accepted");
    let reference = accept_source(&source, &path, &[]).unwrap();
    let base = AcceptedBase::open(&path, &reference).unwrap();
    let yaml = source.join("Catalogs/Sample/object.yaml");
    let original = fs::read_to_string(&yaml)
        .unwrap()
        .replace("\"Образец\"", "'Образец' # translation");
    fs::write(&yaml, &original).unwrap();
    let value = PropertyValue::Localized(vec![
        LocalizedText {
            language: "ru".into(),
            text: "Пример".into(),
        },
        LocalizedText {
            language: "en".into(),
            text: "Sample".into(),
        },
    ]);
    let plan =
        prepare_property_edit(&source, &base, &uuid('2'), PropertyName::Synonym, value).unwrap();
    assert_eq!(plan.updated(), original.replace("'Образец'", "'Пример'"));
    plan.publish(&base).unwrap();
    let before = tree(&source);
    assert_eq!(
        prepare_property_edit(
            &source,
            &base,
            &uuid('2'),
            PropertyName::Synonym,
            PropertyValue::Localized(vec![LocalizedText {
                language: "de".into(),
                text: "Beispiel".into()
            }])
        )
        .unwrap_err()
        .code,
        "source_edit_value_unsupported"
    );
    assert_eq!(tree(&source), before);
}

#[test]
fn restrictions_freeze_unknown_objects_and_name_edits_fail_before_writes() {
    let test = TestDir::new();
    let (source, base) = prepare(&test.0, &['4']);
    let before = tree(&source);
    for (id, property, code) in [
        ('2', PropertyName::Comment, "source_edit_native_restricted"),
        ('4', PropertyName::Comment, "source_edit_frozen"),
        ('5', PropertyName::Comment, "source_edit_frozen"),
        ('f', PropertyName::Comment, "source_edit_object_unknown"),
        ('3', PropertyName::Name, "source_edit_operation_unsupported"),
    ] {
        assert_eq!(
            prepare_property_edit(
                &source,
                &base,
                &uuid(id),
                property,
                PropertyValue::Text("changed".into())
            )
            .unwrap_err()
            .code,
            code
        );
    }
    assert_eq!(tree(&source), before);
}

#[test]
fn external_writes_and_policy_pin_changes_cannot_publish_a_stale_plan() {
    let test = TestDir::new();
    let (source, base) = prepare(&test.0, &[]);
    let plan = prepare_property_edit(
        &source,
        &base,
        &uuid('3'),
        PropertyName::Comment,
        PropertyValue::Text("changed".into()),
    )
    .unwrap();
    let path = source.join("Catalogs/Sample/object.yaml");
    let external = format!("# external writer\n{}", fs::read_to_string(&path).unwrap());
    fs::write(&path, &external).unwrap();
    assert_eq!(
        plan.publish(&base).unwrap_err().code,
        "source_edit_conflict"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), external);
    let other_path = test.0.join("other-base");
    let other_ref = accept_source(&source, &other_path, &[uuid('3')]).unwrap();
    let other = AcceptedBase::open(&other_path, &other_ref).unwrap();
    assert_eq!(
        plan.publish(&other).unwrap_err().code,
        "source_edit_baseline_changed"
    );
    assert!(fs::read_dir(path.parent().unwrap()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".eska-properties-")
    }));
}

#[test]
fn uuid_substitution_rename_and_bom_are_refused_without_normalizing_source() {
    let test = TestDir::new();
    let (source, base) = prepare(&test.0, &[]);
    let path = source.join("Catalogs/Sample/object.yaml");
    let original = fs::read_to_string(&path).unwrap();
    for changed in [
        original.replace(uuid('3').as_str(), uuid('f').as_str()),
        original.replace("\"Description\"", "\"Renamed\""),
        format!("\u{feff}{original}"),
    ] {
        fs::write(&path, &changed).unwrap();
        assert!(
            prepare_property_edit(
                &source,
                &base,
                &uuid('3'),
                PropertyName::Comment,
                PropertyValue::Text("changed".into())
            )
            .is_err()
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), changed);
    }
}

#[test]
fn nested_descriptor_edit_and_bsl_change_export_only_the_expected_native_files() {
    let test = TestDir::new();
    let (source, base) = prepare(&test.0, &[]);
    let before = export_protected(&source, &base, &test.0.join("generations"), true).unwrap();
    let plan = prepare_property_edit(
        &source,
        &base,
        &uuid('7'),
        PropertyName::Comment,
        PropertyValue::Text("Updated form comment".into()),
    )
    .unwrap();
    plan.publish(&base).unwrap();
    let module = source.join("CommonModules/Tools/Ext/Module.bsl");
    let mut updated = fs::read(&module).unwrap();
    updated.extend_from_slice(b"\r\n// local experiment\r\n");
    fs::write(&module, &updated).unwrap();
    let after = export_protected(&source, &base, &test.0.join("generations"), true).unwrap();
    let old = tree(&before.xml);
    let new = tree(&after.xml);
    let changed = old
        .iter()
        .filter_map(|(path, bytes)| {
            (new.get(path) != Some(bytes)).then_some(path.to_string_lossy().into_owned())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        changed,
        [
            "Catalogs/Sample/Forms/Main.xml",
            "CommonModules/Tools/Ext/Module.bsl"
        ]
    );
    assert_eq!(
        fs::read(after.xml.join("CommonModules/Tools/Ext/Module.bsl")).unwrap(),
        updated
    );
}

#[test]
fn invalid_values_missing_properties_and_corrupt_base_cannot_change_source() {
    let test = TestDir::new();
    let (source, base) = prepare(&test.0, &[]);
    let before = tree(&source);
    for (id, property, value) in [
        (
            '3',
            PropertyName::Comment,
            PropertyValue::Text(String::new()),
        ),
        (
            '3',
            PropertyName::Comment,
            PropertyValue::Text("invalid\0XML".into()),
        ),
        (
            '3',
            PropertyName::Version,
            PropertyValue::Text("1.2.3".into()),
        ),
    ] {
        assert!(prepare_property_edit(&source, &base, &uuid(id), property, value).is_err());
    }
    assert_eq!(tree(&source), before);
    let plan = prepare_property_edit(
        &source,
        &base,
        &uuid('3'),
        PropertyName::Comment,
        PropertyValue::Text("changed".into()),
    )
    .unwrap();
    fs::write(test.0.join("accepted/accepted.json"), b"{}").unwrap();
    assert!(plan.publish(&base).is_err());
    assert_eq!(tree(&source), before);
}

#[cfg(unix)]
#[test]
fn read_only_files_and_symlink_substitution_are_refused_before_replacement() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let test = TestDir::new();
    let (source, base) = prepare(&test.0, &[]);
    let path = source.join("Catalogs/Sample/object.yaml");
    let original = fs::read(&path).unwrap();
    let permissions = fs::metadata(&path).unwrap().permissions();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
    assert_eq!(
        prepare_property_edit(
            &source,
            &base,
            &uuid('3'),
            PropertyName::Comment,
            PropertyValue::Text("changed".into())
        )
        .unwrap_err()
        .code,
        "source_edit_read_only"
    );
    fs::set_permissions(&path, permissions).unwrap();
    let plan = prepare_property_edit(
        &source,
        &base,
        &uuid('3'),
        PropertyName::Comment,
        PropertyValue::Text("changed".into()),
    )
    .unwrap();
    let external = test.0.join("external.yaml");
    fs::write(&external, &original).unwrap();
    fs::remove_file(&path).unwrap();
    symlink(&external, &path).unwrap();
    assert_eq!(
        plan.publish(&base).unwrap_err().code,
        "source_edit_unsafe_path"
    );
    assert_eq!(fs::read(external).unwrap(), original);
    assert!(fs::symlink_metadata(path).unwrap().file_type().is_symlink());
}
