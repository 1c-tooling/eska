use super::*;
use crate::{
    project_core::source_format::{
        Descriptor, InputAuthority, ObjectSource, OriginTexts, PermissionReason, Property,
        PropertyName, PropertyValue, Section, accept_source, export_designer, files::Inventory,
        import_designer, import_designer_readable, prepare_protected,
    },
    test_support::TestDir,
};
use std::{fmt::Write as _, fs, path::PathBuf};

/// Keep native identities independent of names and deterministic within one owned fixture.
fn uuid(index: u32) -> NativeUuid {
    NativeUuid::parse(&format!("10000000-0000-4000-8000-{index:012}")).unwrap()
}

/// Native transport and unknown sections are retained while collection order is tested.
fn native(kind: &str, index: u32, name: &str, children: &str) -> String {
    format!(
        "\u{feff}<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><{kind} uuid=\"{}\"><InternalInfo><Future>Preserved</Future></InternalInfo><Properties><Name>{name}</Name><Comment>Old</Comment></Properties><ChildObjects>{children}</ChildObjects></{kind}></MetaDataObject>\r\n",
        uuid(index).as_str()
    )
}

/// Every source, immutable base and generated output belongs to the current test directory.
fn fixture(test: &TestDir, readable: bool, frozen: &[NativeUuid]) -> (PathBuf, AcceptedBase) {
    let input = test.0.join("native");
    fs::create_dir_all(input.join("Enums")).unwrap();
    fs::write(
        input.join("Configuration.xml"),
        native(
            "Configuration",
            1,
            "Sample",
            "<Enum>Status</Enum><Enum>Other</Enum>",
        ),
    )
    .unwrap();
    let mut values = String::new();
    for (index, name) in [(11, "First"), (12, "Second"), (13, "Third")] {
        write!(values, "<EnumValue uuid=\"{}\"><Properties><Name>{name}</Name><Comment>Old</Comment></Properties></EnumValue>", uuid(index).as_str()).unwrap();
    }
    for (index, name) in [(2, "Status"), (3, "Other")] {
        let children = if index == 2 { values.as_str() } else { "" };
        fs::write(
            input.join(format!("Enums/{name}.xml")),
            native("Enum", index, name, children),
        )
        .unwrap();
    }
    let source = test.0.join("source");
    if readable {
        import_designer_readable(&input, &source).unwrap();
    } else {
        import_designer(&input, &source).unwrap();
    }
    let base = test.0.join("accepted");
    let pin = accept_source(&source, &base, frozen).unwrap();
    (source, AcceptedBase::open(&base, &pin).unwrap())
}

/// Raw edits exercise the same supported selection used by CLI and build consumers.
fn edit(source: &std::path::Path, path: &str, change: impl FnOnce(&mut ObjectSource)) {
    let file = source.join(path);
    let mut value = ObjectSource::from_yaml(&fs::read_to_string(&file).unwrap()).unwrap();
    change(&mut value);
    let text = format!(
        "# Authored comment\r\n{}",
        value.to_yaml().unwrap().replace('\n', "\r\n")
    );
    fs::write(file, text).unwrap();
}

/// Fixture children have one native section, with the order being the only collection authority.
fn children(source: &mut ObjectSource) -> &mut Vec<Child> {
    source
        .document
        .descriptor
        .object
        .sections
        .iter_mut()
        .find_map(|section| {
            if let Section::Children(children) = section {
                Some(children)
            } else {
                None
            }
        })
        .unwrap()
}

/// Compare logical child identities from actual native XML, including their original class slots.
fn names(xml: &std::path::Path, path: &str) -> Vec<String> {
    let descriptor = Descriptor::from_xml(&fs::read_to_string(xml.join(path)).unwrap()).unwrap();
    descriptor
        .object
        .sections
        .iter()
        .find_map(|section| {
            let Section::Children(children) = section else {
                return None;
            };
            Some(
                children
                    .iter()
                    .map(|child| match child {
                        Child::Inline { object } => {
                            match object.property(PropertyName::Name).unwrap() {
                                PropertyValue::Text(name) => name.clone(),
                                _ => panic!("fixture name"),
                            }
                        }
                        Child::Reference { name, .. } => name.clone(),
                        Child::Preserved { .. } => "preserved".into(),
                    })
                    .collect(),
            )
        })
        .unwrap()
}

/// Use a separate property change to prove denied order does not mask permitted values.
fn comment(node: &mut super::super::MetadataNode, text: &str) {
    for section in &mut node.sections {
        if let Section::Properties(items) = section {
            for item in items {
                if let Property::Typed {
                    name: PropertyName::Comment,
                    value,
                } = item
                {
                    *value = PropertyValue::Text(text.into());
                    return;
                }
            }
        }
    }
    panic!("fixture comment");
}

/// Native identity is case independent, but declaration spelling and membership are immutable.
#[test]
fn order_identity_rejects_replacements_duplicates_and_unknown_children() {
    let original = serde_json::json!([
        {"kind":"inline", "object":{"uuid":uuid(11)}},
        {"kind":"reference", "metadata_kind":"Enum", "name":"Status"}
    ]);
    let before = from_values(original.as_array().unwrap()).unwrap();
    let mut after = before.clone();
    after.reverse();
    assert!(same_members(&before, &after));
    after[0] = Identity::Reference(MetadataKind::Enum, "status".into());
    assert!(!same_members(&before, &after));
    assert!(!same_members(
        &before,
        &[before[0].clone(), before[0].clone()]
    ));
    assert!(!same_members(&before, &before[..1]));
    for invalid in [
        serde_json::json!([{"kind":"preserved","fragment":{"xml":"<!-- keep -->"}}]),
        serde_json::json!([{"kind":"inline","object":{"uuid":"bad"}}]),
        serde_json::json!([{"kind":"reference","metadata_kind":"Future","name":"Value"}]),
    ] {
        assert!(from_values(invalid.as_array().unwrap()).is_none());
    }
}

/// Both wire envelopes export inline and external order, with unchanged authority and exact repeat.
#[test]
fn permitted_order_matches_raw_export_without_rewriting_working_or_accepted_bytes() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, &[]);
        let accepted = Inventory::read(&test.0.join("accepted")).unwrap();
        let native = Inventory::read(&test.0.join("native")).unwrap();
        let manifest = fs::read(source.join("source.json")).unwrap();
        edit(&source, "configuration.yaml", |source| {
            children(source).reverse();
        });
        edit(&source, "Enums/Status/object.yaml", |source| {
            children(source).reverse();
        });
        let before = Inventory::read(&source).unwrap();
        let control = test.0.join("control");
        export_designer(&source, &control).unwrap();
        let prepared =
            prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
        assert!(prepared.diagnostics().is_empty());
        assert_eq!(
            names(prepared.xml(), "Configuration.xml"),
            ["Other", "Status"]
        );
        assert_eq!(
            names(prepared.xml(), "Enums/Status.xml"),
            ["Third", "Second", "First"]
        );
        assert_eq!(
            Inventory::read(prepared.xml()).unwrap(),
            Inventory::read(&control).unwrap()
        );
        let repeated =
            prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
        assert_eq!(prepared.generation(), repeated.generation());
        assert_eq!(Inventory::read(&source).unwrap(), before);
        assert_eq!(Inventory::read(&test.0.join("accepted")).unwrap(), accepted);
        assert_eq!(Inventory::read(&test.0.join("native")).unwrap(), native);
        assert_eq!(fs::read(source.join("source.json")).unwrap(), manifest);
        for operation in [
            Operation::Create,
            Operation::Delete,
            Operation::Rename,
            Operation::Move,
        ] {
            assert_eq!(
                base.permission(&uuid(2), operation).reason,
                PermissionReason::UnsupportedOperation
            );
        }
    }
}

/// Freeze on the owner, an ancestor or any displaced member denies only the order transaction.
#[test]
fn denied_order_preserves_allowed_values_and_reports_the_actual_frozen_identity() {
    for frozen in [1, 2, 11] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, true, &[uuid(frozen)]);
        edit(&source, "Enums/Status/object.yaml", |source| {
            let children = children(source);
            children.reverse();
            if let Child::Inline { object } = &mut children[0] {
                comment(object, "Independent edit");
            }
        });
        let before = Inventory::read(&source).unwrap();
        let output = test.0.join("generations");
        let failure = prepare_protected(&source, &base, &output, true).unwrap_err();
        assert_eq!(failure.error.code, "source_changes_excluded");
        assert!(!output.exists());
        let order = failure
            .diagnostics
            .iter()
            .find(|item| item.operation == Operation::Reorder)
            .unwrap();
        assert_eq!(order.reason, PermissionReason::EskaFrozen);
        assert_eq!(order.uuid, Some(uuid(if frozen == 11 { 11 } else { 2 })));
        let prepared = prepare_protected(&source, &base, &output, false).unwrap();
        assert_eq!(
            names(prepared.xml(), "Enums/Status.xml"),
            ["First", "Second", "Third"]
        );
        let native = fs::read_to_string(prepared.xml().join("Enums/Status.xml")).unwrap();
        assert_eq!(native.contains("Independent edit"), frozen == 11);
        assert_eq!(Inventory::read(&source).unwrap(), before);
    }
}

/// A frozen member that retains its exact position does not block moving independent siblings.
#[test]
fn stationary_frozen_members_do_not_grant_or_prevent_other_member_permissions() {
    let test = TestDir::new();
    let (source, base) = fixture(&test, true, &[uuid(11)]);
    edit(&source, "Enums/Status/object.yaml", |source| {
        children(source).swap(1, 2);
    });
    let prepared = prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
    assert_eq!(
        names(prepared.xml(), "Enums/Status.xml"),
        ["First", "Third", "Second"]
    );
    assert!(prepared.diagnostics().is_empty());
}

/// External descriptor identity is resolved through the accepted owner before child permission.
#[test]
fn frozen_external_member_blocks_root_order_without_freezing_unrelated_values() {
    let test = TestDir::new();
    let (source, base) = fixture(&test, false, &[uuid(3)]);
    edit(&source, "configuration.yaml", |source| {
        children(source).reverse();
        comment(&mut source.document.descriptor.object, "Working root");
    });
    let failure = prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap_err();
    assert_eq!(failure.diagnostics.len(), 1);
    assert_eq!(failure.diagnostics[0].operation, Operation::Reorder);
    assert_eq!(failure.diagnostics[0].uuid, Some(uuid(3)));
    assert_eq!(failure.diagnostics[0].reason, PermissionReason::EskaFrozen);
    let prepared = prepare_protected(&source, &base, &test.0.join("generations"), false).unwrap();
    assert_eq!(
        names(prepared.xml(), "Configuration.xml"),
        ["Status", "Other"]
    );
    assert!(
        fs::read_to_string(prepared.xml().join("Configuration.xml"))
            .unwrap()
            .contains("Working root")
    );
}

/// Changes in membership, native class and UUID remain excluded rather than being interpreted as order.
#[test]
fn order_cannot_authorize_create_delete_rename_or_identity_substitution() {
    for fault in ["add", "delete", "uuid", "kind", "rename", "duplicate"] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, true, &[]);
        let file = source.join("Enums/Status/object.yaml");
        let mut value = super::super::yaml::decode(&fs::read_to_string(&file).unwrap()).unwrap();
        let children = value["document"]["descriptor"]["object"]["children"]
            .as_array_mut()
            .unwrap();
        children.reverse();
        match fault {
            "add" => {
                let mut added = children[0].clone();
                added["object"]["uuid"] = serde_json::json!(uuid(99));
                children.push(added);
            }
            "delete" => {
                children.pop();
            }
            "uuid" => children[0]["object"]["uuid"] = serde_json::json!(uuid(99)),
            "kind" => children[0]["object"]["kind"] = serde_json::json!("Attribute"),
            "rename" => children[0]["object"]["properties"]["Name"] = serde_json::json!("NewName"),
            "duplicate" => children.push(children[0].clone()),
            _ => unreachable!(),
        }
        fs::write(&file, super::super::yaml::encode(&value).unwrap()).unwrap();
        let before = Inventory::read(&source).unwrap();
        assert!(
            prepare_protected(&source, &base, &test.0.join("generations"), true).is_err(),
            "{fault}"
        );
        assert!(!test.0.join("generations").exists());
        assert_eq!(Inventory::read(&source).unwrap(), before);
    }
}

/// Opaque children retain their position and bytes even when all known UUIDs are unchanged.
#[test]
fn preserved_child_slots_refuse_order_and_keep_native_bytes() {
    let test = TestDir::new();
    let (source, _) = fixture(&test, true, &[]);
    edit(&source, "Enums/Status/object.yaml", |source| {
        children(source).insert(
            1,
            Child::Preserved {
                fragment: super::super::XmlFragment {
                    xml: "<!-- Native slot -->".into(),
                    namespaces: Vec::new(),
                },
            },
        );
    });
    let pin = accept_source(&source, &test.0.join("with-slots"), &[]).unwrap();
    let base = AcceptedBase::open(&test.0.join("with-slots"), &pin).unwrap();
    edit(&source, "Enums/Status/object.yaml", |source| {
        children(source).reverse();
    });
    let failure = prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap_err();
    assert_eq!(failure.error.code, "source_changes_excluded");
    let prepared = prepare_protected(&source, &base, &test.0.join("generations"), false).unwrap();
    assert_eq!(
        names(prepared.xml(), "Enums/Status.xml"),
        ["First", "preserved", "Second", "Third"]
    );
    assert!(
        fs::read_to_string(prepared.xml().join("Enums/Status.xml"))
            .unwrap()
            .contains("<!-- Native slot -->")
    );
}

/// Reordered UUIDs still map a changed property to its actual working range instead of the old index.
#[test]
fn source_map_follows_identity_when_collection_order_and_a_property_change_together() {
    let test = TestDir::new();
    let (source, base) = fixture(&test, true, &[]);
    let file = source.join("Enums/Status/object.yaml");
    let accepted = fs::read_to_string(&file).unwrap();
    edit(&source, "Enums/Status/object.yaml", |source| {
        let children = children(source);
        children.reverse();
        if let Child::Inline { object } = &mut children[0] {
            comment(object, "Mapped value");
        }
    });
    let working = fs::read_to_string(&file).unwrap();
    let prepared = prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
    let xml = fs::read(prepared.xml().join("Enums/Status.xml")).unwrap();
    let offset = std::str::from_utf8(&xml)
        .unwrap()
        .find("Mapped value")
        .unwrap();
    let mapped = prepared
        .source_map()
        .files
        .iter()
        .find(|file| file.path.as_str() == "Enums/Status.xml")
        .unwrap();
    let location = mapped
        .locate(
            &xml,
            offset..offset + 1,
            Some(OriginTexts {
                accepted: &accepted,
                working: Some(&working),
            }),
        )
        .unwrap();
    assert_eq!(location.authority, InputAuthority::Working);
    assert_eq!(location.uuid, Some(uuid(13)));
    assert!(working[location.range.unwrap()].contains("Mapped value"));
}

/// Unsaved root order uses the same member bindings; renamed or added declarations still fail.
#[test]
fn reader_accepts_permuted_root_declarations_without_rebinding_saved_or_unsaved_members() {
    use crate::project_core::{metadata_parser::PropertiesMode, source_format::SourceReader};

    for readable in [false, true] {
        let test = TestDir::new();
        let (source, _) = fixture(&test, readable, &[]);
        let reader = SourceReader::open(&source).unwrap();
        let path = SourcePath::parse("configuration.yaml").unwrap();
        edit(&source, path.as_str(), |source| {
            children(source).reverse();
        });
        let authored = fs::read_to_string(source.join(path.as_path())).unwrap();
        let mut owner = reader.parse_owner(&path, &authored).unwrap();
        assert_eq!(
            from_children(children(&mut owner.source)).unwrap()[0],
            Identity::Reference(MetadataKind::Enum, "Other".into())
        );
        for mode in [PropertiesMode::All, PropertiesMode::Summary] {
            let unsaved = reader.parse_metadata(&path, &authored, mode).unwrap();
            let saved = reader.read_metadata(&path, mode).unwrap();
            assert_eq!(saved, unsaved);
            assert_eq!(
                unsaved.documents[0]
                    .descriptor
                    .references
                    .iter()
                    .map(|reference| reference.name.as_str())
                    .collect::<Vec<_>>(),
                ["Other", "Status"]
            );
        }
        let mut renamed = owner.source;
        let Child::Reference { name, .. } = &mut children(&mut renamed)[0] else {
            panic!("declared member");
        };
        *name = "Unknown".into();
        let renamed = renamed.to_yaml().unwrap();
        assert_eq!(
            reader.parse_owner(&path, &renamed).unwrap_err().code,
            "source_declaration_mismatch"
        );
        assert_eq!(
            reader
                .parse_metadata(&path, &renamed, PropertiesMode::All)
                .unwrap_err()
                .code,
            "source_declaration_mismatch"
        );
    }
}
