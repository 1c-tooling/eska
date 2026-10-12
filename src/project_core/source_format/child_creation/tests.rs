use std::fs;

use super::*;
use crate::{
    project_core::{
        metadata_parser::PropertiesMode,
        source_format::{
            Descriptor, InputAuthority, ObjectSource, OriginTexts, PermissionReason, SourceReader,
            export_designer, files::Inventory, prepare_protected,
        },
    },
    test_support::TestDir,
};

use super::super::enum_test_support::{children, edit, fixture, uuid, value};

/// Both envelopes introduce fresh UUIDs through their accepted parent and retain source/base bytes.
#[test]
fn new_values_match_native_control_without_becoming_accepted_objects() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, &[]);
        let accepted = Inventory::read(&test.0.join("accepted")).unwrap();
        let input = Inventory::read(&test.0.join("native")).unwrap();
        let manifest = fs::read(source.join("source.json")).unwrap();
        edit(&source, |owner| {
            children(owner).extend([value(100, "Third"), value(101, "Четвёртое_2")]);
        });
        let authored = Inventory::read(&source).unwrap();
        let control = test.0.join("control");
        export_designer(&source, &control).unwrap();
        let prepared =
            prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
        assert!(prepared.diagnostics().is_empty());
        assert_eq!(
            Inventory::read(prepared.xml()).unwrap(),
            Inventory::read(&control).unwrap()
        );
        assert!(base.permission(&uuid(2), Operation::Create).allowed);
        assert_eq!(
            base.permission(&uuid(100), Operation::Module).reason,
            PermissionReason::UnknownObject
        );
        assert_eq!(
            base.permission(&uuid(1), Operation::Create).reason,
            PermissionReason::UnsupportedOperation
        );
        let repeated =
            prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
        assert_eq!(prepared.generation(), repeated.generation());
        assert_eq!(Inventory::read(&source).unwrap(), authored);
        assert_eq!(Inventory::read(&test.0.join("accepted")).unwrap(), accepted);
        assert_eq!(Inventory::read(&test.0.join("native")).unwrap(), input);
        assert_eq!(fs::read(source.join("source.json")).unwrap(), manifest);
    }
}

/// A parent or ancestor freeze cannot be bypassed by a UUID absent from native supplier policy.
#[test]
fn frozen_owner_refuses_creation_in_both_envelopes_before_publication() {
    for readable in [false, true] {
        for frozen in [1, 2] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[uuid(frozen)]);
            edit(&source, |owner| children(owner).push(value(100, "Third")));
            let before = Inventory::read(&source).unwrap();
            let output = test.0.join("generations");
            let failure = prepare_protected(&source, &base, &output, true).unwrap_err();
            assert_eq!(failure.error.code, "source_changes_excluded");
            assert_eq!(failure.diagnostics.len(), 1);
            assert_eq!(failure.diagnostics[0].operation, Operation::Create);
            assert_eq!(failure.diagnostics[0].uuid, Some(uuid(2)));
            assert_eq!(failure.diagnostics[0].reason, PermissionReason::EskaFrozen);
            assert!(!output.exists());
            let restored = prepare_protected(&source, &base, &output, false).unwrap();
            assert_eq!(
                Descriptor::from_xml(
                    &fs::read_to_string(restored.xml().join("Enums/Status.xml")).unwrap()
                )
                .unwrap(),
                Descriptor::from_xml(
                    &fs::read_to_string(test.0.join("native/Enums/Status.xml")).unwrap()
                )
                .unwrap()
            );
            assert_eq!(Inventory::read(&source).unwrap(), before);
        }
    }
}

/// A stationary frozen sibling permits append, but insertion cannot change its native index.
#[test]
fn shifted_frozen_member_blocks_insertion_without_masking_independent_properties() {
    for inserted_first in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, true, &[uuid(11)]);
        edit(&source, |owner| {
            let children = children(owner);
            if let Child::Inline { object } = &mut children[1]
                && let Section::Properties(properties) = &mut object.sections[0]
                && let Property::Typed { value, .. } = &mut properties[2]
            {
                *value = PropertyValue::Text("Independent edit".into());
            }
            children.insert(if inserted_first { 0 } else { 2 }, value(100, "Third"));
        });
        let output = test.0.join("generations");
        if inserted_first {
            let failure = prepare_protected(&source, &base, &output, true).unwrap_err();
            assert_eq!(failure.diagnostics.len(), 1);
            assert_eq!(failure.diagnostics[0].operation, Operation::Create);
            assert_eq!(failure.diagnostics[0].uuid, Some(uuid(11)));
            assert_eq!(failure.diagnostics[0].reason, PermissionReason::EskaFrozen);
            assert!(!output.exists());
        }
        let prepared = prepare_protected(&source, &base, &output, !inserted_first).unwrap();
        let xml = fs::read_to_string(prepared.xml().join("Enums/Status.xml")).unwrap();
        assert_eq!(xml.contains("Third"), !inserted_first);
        assert!(xml.contains("Independent edit"));
    }
}

/// An identity already belonging to another accepted owner cannot be presented as a new value.
#[test]
fn foreign_identity_and_duplicate_addition_do_not_expand_native_composition() {
    for index in [3, 11] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, true, &[]);
        edit(&source, |owner| children(owner).push(value(index, "Third")));
        let before = Inventory::read(&source).unwrap();
        assert!(prepare_protected(&source, &base, &test.0.join("generations"), true).is_err());
        assert!(!test.0.join("generations").exists());
        assert_eq!(Inventory::read(&source).unwrap(), before);
    }
}

/// Names are checked only for the new profile, including Cyrillic and case-insensitive collisions.
#[test]
fn invalid_or_shadowing_names_refuse_before_creating_a_generation() {
    for readable in [false, true] {
        for (name, code) in [
            ("1Value", "source_metadata_name_invalid"),
            ("With space", "source_metadata_name_invalid"),
            ("Имя-Имя", "source_metadata_name_invalid"),
            ("first", "source_metadata_name_collision"),
        ] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[]);
            edit(&source, |owner| children(owner).push(value(100, name)));
            let before = Inventory::read(&source).unwrap();
            let failure =
                prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap_err();
            assert_eq!(failure.error.code, code, "{name}");
            assert!(!test.0.join("generations").exists());
            assert_eq!(Inventory::read(&source).unwrap(), before);
        }
    }
}

/// New class, attributes, namespace, opaque data or incomplete property bindings lack an inverse rule.
#[test]
fn unproven_native_shapes_cannot_introduce_child_bytes() {
    for fault in ["class", "attribute", "namespace", "opaque", "property"] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, true, &[]);
        edit(&source, |owner| {
            let Child::Inline { mut object } = value(100, "Third") else {
                unreachable!()
            };
            match fault {
                "class" => object.kind = MetadataKind::Attribute,
                "attribute" => object.attributes.push(super::super::XmlAttribute {
                    name: "future".into(),
                    value: "value".into(),
                }),
                "namespace" => object.namespaces.push(super::super::Namespace {
                    prefix: Some("f".into()),
                    uri: "urn:future".into(),
                }),
                "opaque" => object
                    .sections
                    .push(Section::Preserved(super::super::XmlFragment {
                        xml: "<Future/>".into(),
                        namespaces: Vec::new(),
                    })),
                "property" => {
                    let Section::Properties(properties) = &mut object.sections[0] else {
                        unreachable!()
                    };
                    properties.pop();
                }
                _ => unreachable!(),
            }
            children(owner).push(Child::Inline { object });
        });
        let failure =
            prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap_err();
        assert_eq!(
            failure.error.code, "source_child_creation_profile_unsupported",
            "{fault}"
        );
        assert!(!test.0.join("generations").exists());
    }
}

/// A creation request cannot also delete or reorder old members, or gain permission through a raw owner class.
#[test]
fn creation_does_not_enable_other_composition_operations() {
    for fault in ["delete", "order", "owner_kind"] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, true, &[]);
        edit(&source, |owner| {
            let children = children(owner);
            match fault {
                "delete" => {
                    children.remove(0);
                }
                "order" => children.reverse(),
                "owner_kind" => {}
                _ => unreachable!(),
            }
            children.push(value(100, "Third"));
            if fault == "owner_kind" {
                owner.document.descriptor.object.kind = MetadataKind::Catalog;
            }
        });
        assert!(prepare_protected(&source, &base, &test.0.join("generations"), true).is_err());
        assert!(!test.0.join("generations").exists());
    }
}

/// A created node and its Name refer to working YAML; source-map DTO does not manufacture accepted origin.
#[test]
fn created_node_locations_require_matching_selected_working_bytes() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, &[]);
        let path = source.join("Enums/Status/object.yaml");
        let accepted = fs::read_to_string(&path).unwrap();
        let working = edit(&source, |owner| {
            children(owner).push(value(100, "NewValue"));
        });
        let prepared =
            prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
        let native = fs::read(prepared.xml().join("Enums/Status.xml")).unwrap();
        let xml = std::str::from_utf8(&native).unwrap();
        let mapped = prepared
            .source_map()
            .files
            .iter()
            .find(|file| file.path.as_str() == "Enums/Status.xml")
            .unwrap();
        for needle in [uuid(100).as_str(), "<Name>NewValue</Name>"] {
            let offset = xml.find(needle).unwrap();
            let location = mapped
                .locate(
                    &native,
                    offset..offset + 1,
                    Some(OriginTexts {
                        accepted: &accepted,
                        working: Some(&working),
                    }),
                )
                .unwrap();
            assert_eq!(location.authority, InputAuthority::Working);
            assert_eq!(location.uuid, Some(uuid(100)));
            assert!(working[location.range.unwrap()].contains("NewValue"));
        }
        let offset = xml.find("<Name>NewValue</Name>").unwrap();
        assert!(
            mapped
                .locate(
                    &native,
                    offset..offset + 1,
                    Some(OriginTexts {
                        accepted: &accepted,
                        working: None
                    })
                )
                .is_err()
        );
        let changed = working.replace("NewValue", "Tampered");
        assert!(
            mapped
                .locate(
                    &native,
                    offset..offset + 1,
                    Some(OriginTexts {
                        accepted: &accepted,
                        working: Some(&changed)
                    })
                )
                .is_err()
        );
    }
}

/// Saved and overlay projections expose the authored value directly, independent of accepted policy.
#[test]
fn saved_and_unsaved_readers_include_created_value_identity_and_properties() {
    let test = TestDir::new();
    let (source, base) = fixture(&test, true, &[]);
    let working = edit(&source, |owner| {
        children(owner).push(value(100, "NewValue"));
    });
    let reader = SourceReader::open(&source).unwrap();
    let path = SourcePath::parse("Enums/Status/object.yaml").unwrap();
    for mode in [PropertiesMode::All, PropertiesMode::Summary] {
        let saved = reader.read_metadata(&path, mode).unwrap();
        assert_eq!(saved, reader.parse_metadata(&path, &working, mode).unwrap());
        assert!(
            saved.documents[0]
                .descriptor
                .objects
                .iter()
                .any(|object| object.metadata.name == "NewValue")
        );
    }
    assert!(!base.permission(&uuid(100), Operation::Create).allowed);
    let prepared = prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
    let descriptor =
        Descriptor::from_xml(&fs::read_to_string(prepared.xml().join("Enums/Status.xml")).unwrap())
            .unwrap();
    assert!(
        descriptor
            .object
            .sections
            .iter()
            .any(|section| matches!(section, Section::Children(children) if children.len() == 3))
    );
}

/// An empty accepted native collection does not require a template value or an edited reverse map.
#[test]
fn empty_enum_collection_accepts_a_first_value_with_its_own_identity() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, &[]);
        let path = source.join("Enums/Other/object.yaml");
        let manifest = fs::read(source.join("source.json")).unwrap();
        let mut owner = ObjectSource::from_yaml(&fs::read_to_string(&path).unwrap()).unwrap();
        children(&mut owner).push(value(100, "Первое_Значение"));
        let authored = owner.to_yaml().unwrap();
        fs::write(&path, &authored).unwrap();
        let prepared =
            prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
        assert!(prepared.diagnostics().is_empty());
        let xml = fs::read_to_string(prepared.xml().join("Enums/Other.xml")).unwrap();
        assert!(xml.contains(uuid(100).as_str()));
        assert!(xml.contains("Первое_Значение"));
        assert_eq!(fs::read_to_string(path).unwrap(), authored);
        assert_eq!(fs::read(source.join("source.json")).unwrap(), manifest);
    }
}

/// Fresh identity uniqueness is configuration-wide, rather than limited to one owner or working file.
#[test]
fn separate_owners_cannot_introduce_the_same_fresh_native_uuid() {
    let test = TestDir::new();
    let (source, base) = fixture(&test, true, &[]);
    edit(&source, |owner| {
        children(owner).push(value(100, "NewValue"));
    });
    let path = source.join("Enums/Other/object.yaml");
    let mut other = ObjectSource::from_yaml(&fs::read_to_string(&path).unwrap()).unwrap();
    children(&mut other).push(value(100, "AnotherValue"));
    fs::write(path, other.to_yaml().unwrap()).unwrap();
    let before = Inventory::read(&source).unwrap();
    let failure = prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap_err();
    assert_eq!(failure.error.code, "native_uuid_duplicate");
    assert!(!test.0.join("generations").exists());
    assert_eq!(Inventory::read(&source).unwrap(), before);
}
