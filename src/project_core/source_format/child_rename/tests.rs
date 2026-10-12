use std::fs;

use super::*;
use crate::{
    project_core::{
        metadata_parser::{self, PropertiesMode},
        source_format::{
            Descriptor, InputAuthority, MetadataReference, ObjectSource, OriginTexts,
            PermissionReason, SourceReader,
            enum_test_support::{children, edit, fixture, fixture_with, native, uuid, value},
            files::Inventory,
            prepare_protected,
        },
    },
    test_support::TestDir,
};

/// Mutate exactly one existing typed Name while retaining its native UUID and other slots.
fn name(node: &mut MetadataNode, next: &str) {
    for section in &mut node.sections {
        if let Section::Properties(properties) = section {
            for property in properties {
                if let Property::Typed {
                    name: PropertyName::Name,
                    value,
                } = property
                {
                    *value = PropertyValue::Text(next.into());
                }
            }
        }
    }
}

/// Resolve a fixture value by its accepted identity, independently of current order or displayed name.
fn member(owner: &mut ObjectSource, id: u32) -> &mut MetadataNode {
    children(owner)
        .iter_mut()
        .find_map(|child| match child {
            Child::Inline { object } if object.uuid == uuid(id) => Some(object.as_mut()),
            _ => None,
        })
        .unwrap()
}

/// English and Cyrillic renames replace only the native Name, retaining every source/base/input byte.
#[test]
fn existing_value_rename_keeps_identity_and_native_transport_in_both_schemas() {
    for readable in [false, true] {
        for next in ["Changed_First", "Новое_Значение"] {
            let test = TestDir::new();
            let (source, base) = fixture_with(&test, readable, &[], |input| {
                let path = input.join("Enums/Status.xml");
                let original = fs::read_to_string(&path).unwrap();
                fs::write(
                    path,
                    Descriptor::from_xml(&original).unwrap().to_xml().unwrap(),
                )
                .unwrap();
            });
            let accepted = Inventory::read(&test.0.join("accepted")).unwrap();
            let original = Inventory::read(&test.0.join("native")).unwrap();
            let manifest = fs::read(source.join("source.json")).unwrap();
            edit(&source, |owner| name(member(owner, 11), next));
            let authored = Inventory::read(&source).unwrap();
            let output = test.0.join("generations");
            let prepared = prepare_protected(&source, &base, &output, true).unwrap();
            assert!(prepared.diagnostics().is_empty());
            let native = fs::read_to_string(test.0.join("native/Enums/Status.xml")).unwrap();
            assert_eq!(
                fs::read_to_string(prepared.xml().join("Enums/Status.xml")).unwrap(),
                native.replace("<Name>First</Name>", &format!("<Name>{next}</Name>"))
            );
            assert_eq!(
                prepared.generation(),
                prepare_protected(&source, &base, &output, true)
                    .unwrap()
                    .generation()
            );
            assert!(base.permission(&uuid(11), Operation::Rename).allowed);
            assert!(
                !base
                    .permission(&uuid(11), Operation::Property(PropertyName::Name))
                    .allowed
            );
            assert_eq!(
                base.permission(&uuid(2), Operation::Rename).reason,
                PermissionReason::UnsupportedOperation
            );
            assert_eq!(
                base.permission(&uuid(31), Operation::Rename).reason,
                PermissionReason::UnknownObject
            );
            assert_eq!(Inventory::read(&source).unwrap(), authored);
            assert_eq!(Inventory::read(&test.0.join("accepted")).unwrap(), accepted);
            assert_eq!(Inventory::read(&test.0.join("native")).unwrap(), original);
            assert_eq!(fs::read(source.join("source.json")).unwrap(), manifest);
        }
    }
}

/// Parent/ancestor and renamed-value freezes restore Name; unrelated permitted comments still survive.
#[test]
fn frozen_namespace_or_value_reports_one_rename_with_the_actual_blocker() {
    for readable in [false, true] {
        for frozen in [1, 2, 11] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[uuid(frozen)]);
            edit(&source, |owner| {
                name(member(owner, 11), "Renamed");
                for section in &mut owner.document.descriptor.object.sections {
                    if let Section::Properties(properties) = section {
                        for property in properties {
                            if let Property::Typed {
                                name: PropertyName::Comment,
                                value,
                            } = property
                            {
                                *value = PropertyValue::Text("Independent owner edit".into());
                            }
                        }
                    }
                }
            });
            let output = test.0.join("generations");
            let failure = prepare_protected(&source, &base, &output, true).unwrap_err();
            let changes: Vec<_> = failure
                .diagnostics
                .iter()
                .filter(|entry| entry.operation == Operation::Rename)
                .collect();
            assert_eq!(changes.len(), 1);
            assert_eq!(changes[0].uuid, Some(uuid(if frozen < 3 { 2 } else { 11 })));
            assert_eq!(changes[0].reason, PermissionReason::EskaFrozen);
            assert!(!output.exists());
            let selected = prepare_protected(&source, &base, &output, false).unwrap();
            let xml = fs::read_to_string(selected.xml().join("Enums/Status.xml")).unwrap();
            assert!(xml.contains("<Name>First</Name>") && !xml.contains("Renamed"));
            assert_eq!(xml.contains("Independent owner edit"), frozen == 11);
        }
    }
}

/// Renaming does not modify the native index of a stationary frozen sibling.
#[test]
fn stationary_frozen_sibling_does_not_restrict_another_values_name() {
    let test = TestDir::new();
    let (source, base) = fixture(&test, true, &[uuid(12)]);
    edit(&source, |owner| name(member(owner, 11), "Renamed"));
    assert!(
        prepare_protected(&source, &base, &test.0.join("generations"), true)
            .unwrap()
            .diagnostics()
            .is_empty()
    );
}

/// Invalid native identifiers and case-insensitive collisions refuse all publication.
#[test]
fn invalid_or_colliding_names_fail_before_generation() {
    for readable in [false, true] {
        for (next, code) in [
            ("bad name", "source_metadata_name_invalid"),
            ("9Bad", "source_metadata_name_invalid"),
            ("Second", "source_metadata_name_collision"),
            ("sEcOnD", "source_metadata_name_collision"),
        ] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[]);
            edit(&source, |owner| name(member(owner, 11), next));
            let output = test.0.join("generations");
            for strict in [true, false] {
                assert_eq!(
                    prepare_protected(&source, &base, &output, strict)
                        .unwrap_err()
                        .error
                        .code,
                    code
                );
                assert!(!output.exists());
            }
        }
    }
}

/// Names are validated in the final selection, permitting swaps and explicit reuse by creation or deletion.
#[test]
fn final_composition_supports_atomic_name_swaps_and_name_reuse() {
    for readable in [false, true] {
        for variant in ["swap", "create", "delete"] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[]);
            edit(&source, |owner| match variant {
                "swap" => {
                    name(member(owner, 11), "Second");
                    name(member(owner, 12), "First");
                }
                "create" => {
                    name(member(owner, 11), "Renamed");
                    children(owner).push(value(31, "First"));
                }
                "delete" => {
                    name(member(owner, 11), "Second");
                    children(owner).pop();
                }
                _ => unreachable!(),
            });
            assert!(
                prepare_protected(&source, &base, &test.0.join("generations"), true)
                    .unwrap()
                    .diagnostics()
                    .is_empty()
            );
        }
    }
}

/// A forbidden reorder is restored without discarding an independent permitted rename.
#[test]
fn forbidden_order_restores_positions_but_keeps_a_permitted_name() {
    let test = TestDir::new();
    let (source, base) = fixture(&test, true, &[uuid(12)]);
    edit(&source, |owner| {
        name(member(owner, 11), "Renamed");
        children(owner).swap(0, 1);
    });
    let output = test.0.join("generations");
    let failure = prepare_protected(&source, &base, &output, true).unwrap_err();
    assert_eq!(failure.diagnostics.len(), 1);
    assert_eq!(failure.diagnostics[0].operation, Operation::Reorder);
    let selected = prepare_protected(&source, &base, &output, false).unwrap();
    let xml = fs::read_to_string(selected.xml().join("Enums/Status.xml")).unwrap();
    assert!(xml.contains("<Name>Renamed</Name>"));
    assert!(xml.find(uuid(11).as_str()).unwrap() < xml.find(uuid(12).as_str()).unwrap());
}

/// A preserved accepted field prevents extending a closed rename profile by guesswork.
#[test]
fn an_unsupported_accepted_value_profile_restores_name() {
    let test = TestDir::new();
    let (source, base) = fixture_with(&test, true, &[], |input| {
        let path = input.join("Enums/Status.xml");
        let xml = fs::read_to_string(&path).unwrap().replace(
            "<Name>First</Name>",
            "<Name>First</Name><FutureProperty>keep</FutureProperty>",
        );
        fs::write(path, xml).unwrap();
    });
    edit(&source, |owner| name(member(owner, 11), "Renamed"));
    let output = test.0.join("generations");
    let failure = prepare_protected(&source, &base, &output, true).unwrap_err();
    assert_eq!(failure.diagnostics.len(), 1);
    assert_eq!(failure.diagnostics[0].operation, Operation::Rename);
    assert_eq!(
        failure.diagnostics[0].reason,
        PermissionReason::UnsupportedOperation
    );
    let selected = prepare_protected(&source, &base, &output, false).unwrap();
    assert!(
        fs::read_to_string(selected.xml().join("Enums/Status.xml"))
            .unwrap()
            .contains("<Name>First</Name>")
    );
}

/// UUID dependencies survive rename; an old canonical name in decoded XML blocks it until explicitly repaired.
#[test]
fn selected_xml_references_distinguish_preserved_uuid_from_invalidated_name() {
    for readable in [false, true] {
        for fixed in [false, true] {
            let test = TestDir::new();
            let (source, base) = fixture_with(&test, readable, &[], |input| {
                fs::create_dir_all(input.join("Enums/Status/Ext")).unwrap();
                fs::write(input.join("Enums/Status/Ext/Links.xml"), format!("<Links><UUID>{}</UUID><Name><![CDATA[Enum.Status.EnumValue.First]]></Name></Links>", uuid(11).as_str())).unwrap();
            });
            edit(&source, |owner| name(member(owner, 11), "Renamed"));
            let payload = base
                .package
                .payload_sources
                .get(&SourcePath::parse("Enums/Status/Ext/Links.xml").unwrap())
                .unwrap();
            if fixed {
                let file = source.join(payload.as_path());
                let xml = fs::read_to_string(&file).unwrap().replace(
                    "Enum.Status.EnumValue.First",
                    "Enum.Status.EnumValue.Renamed",
                );
                fs::write(file, xml).unwrap();
            }
            let output = test.0.join("generations");
            if fixed {
                let selected = prepare_protected(&source, &base, &output, true).unwrap();
                assert!(
                    fs::read_to_string(selected.xml().join("Enums/Status/Ext/Links.xml"))
                        .unwrap()
                        .contains(uuid(11).as_str())
                );
            } else {
                for strict in [true, false] {
                    assert_eq!(
                        prepare_protected(&source, &base, &output, strict)
                            .unwrap_err()
                            .error
                            .code,
                        "source_structural_reference_preserved"
                    );
                    assert!(!output.exists());
                }
            }
        }
    }
}

/// A changed UUID or class cannot smuggle a new Name into the original accepted value.
#[test]
fn working_identity_or_class_substitution_never_grants_rename_authority() {
    for readable in [false, true] {
        for class in [false, true] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[]);
            edit(&source, |owner| {
                let node = member(owner, 11);
                name(node, "Renamed");
                if class {
                    node.kind = MetadataKind::Attribute;
                } else {
                    node.uuid = uuid(31);
                }
            });
            let output = test.0.join("generations");
            assert_eq!(
                prepare_protected(&source, &base, &output, true)
                    .unwrap_err()
                    .error
                    .code,
                "source_changes_excluded"
            );
            assert!(!output.exists());
            let restored = prepare_protected(&source, &base, &output, false).unwrap();
            let xml = fs::read_to_string(restored.xml().join("Enums/Status.xml")).unwrap();
            assert!(xml.contains("<Name>First</Name>") && xml.contains(uuid(11).as_str()));
            assert!(!xml.contains("Renamed") && !xml.contains(uuid(31).as_str()));
        }
    }
}

/// Metadata fragments share decoded dependency checks, and invalid XML cannot be treated as an empty index.
#[test]
fn preserved_metadata_and_unverified_payloads_block_name_invalidation() {
    for readable in [false, true] {
        for resource in [false, true] {
            let test = TestDir::new();
            let (source, base) = fixture_with(&test, readable, &[], |input| {
                if resource {
                    fs::create_dir_all(input.join("Enums/Status/Ext")).unwrap();
                    fs::write(input.join("Enums/Status/Ext/Links.xml"), "<Links>").unwrap();
                } else {
                    let path = input.join("Enums/Status.xml");
                    let xml = fs::read_to_string(&path).unwrap().replace("<Name>Status</Name>", "<Name>Status</Name><FutureReference target=\"Enum.St&#97;tus.EnumValue.First\"/>");
                    fs::write(path, xml).unwrap();
                }
            });
            edit(&source, |owner| name(member(owner, 11), "Renamed"));
            let output = test.0.join("generations");
            for strict in [true, false] {
                assert_eq!(
                    prepare_protected(&source, &base, &output, strict)
                        .unwrap_err()
                        .error
                        .code,
                    if resource {
                        "source_structural_xml_unverified"
                    } else {
                        "source_structural_reference_preserved"
                    }
                );
                assert!(!output.exists());
            }
        }
    }
}

/// A forbidden payload repair cannot remove the accepted XML dependency which actually reaches the build.
#[test]
fn frozen_resource_reference_fix_cannot_bless_a_rename() {
    let test = TestDir::new();
    let (source, base) = fixture_with(&test, false, &[uuid(3)], |input| {
        fs::create_dir_all(input.join("Enums/Other/Ext")).unwrap();
        fs::write(
            input.join("Enums/Other/Ext/Links.xml"),
            "<Links target=\"Enum.Status.EnumValue.First\"/>",
        )
        .unwrap();
    });
    edit(&source, |owner| name(member(owner, 11), "Renamed"));
    fs::write(
        source.join("Enums/Other/Ext/Links.xml"),
        "<Links target=\"Enum.Status.EnumValue.Renamed\"/>",
    )
    .unwrap();
    let output = test.0.join("generations");
    for strict in [true, false] {
        assert_eq!(
            prepare_protected(&source, &base, &output, strict)
                .unwrap_err()
                .error
                .code,
            if strict {
                "source_changes_excluded"
            } else {
                "source_structural_reference_preserved"
            }
        );
        assert!(!output.exists());
    }
}

/// Typed references use final names and case-insensitive lookup; a frozen old reference remains blocking.
#[test]
fn typed_dependencies_require_allowed_reference_edits() {
    for readable in [false, true] {
        for (fixed, frozen) in [(false, false), (true, false), (true, true)] {
            let test = TestDir::new();
            let freeze = if frozen { vec![uuid(30)] } else { Vec::new() };
            let (source, base) = fixture_with(&test, readable, &freeze, |input| {
                let file = input.join("Configuration.xml");
                let xml = fs::read_to_string(&file).unwrap().replace(
                    "</ChildObjects>",
                    "<Subsystem>Navigation</Subsystem></ChildObjects>",
                );
                fs::write(file, xml).unwrap();
                fs::create_dir_all(input.join("Subsystems")).unwrap();
                let xml = native("Subsystem", 30, "Navigation", "").replace("<MetaDataObject ", "<MetaDataObject xmlns:xr=\"http://v8.1c.ru/8.3/xcf/readable\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" ")
                    .replace("</Properties>", "<Content><xr:Item xsi:type=\"xr:MDObjectRef\">Enum.Status.EnumValue.First</xr:Item></Content></Properties>");
                fs::write(input.join("Subsystems/Navigation.xml"), xml).unwrap();
            });
            edit(&source, |owner| name(member(owner, 11), "Renamed"));
            if fixed {
                let path = source.join("Subsystems/Navigation/object.yaml");
                let mut owner =
                    ObjectSource::from_yaml(&fs::read_to_string(&path).unwrap()).unwrap();
                for section in &mut owner.document.descriptor.object.sections {
                    if let Section::Properties(properties) = section {
                        for property in properties {
                            if let Property::Typed {
                                name: PropertyName::Content,
                                value: PropertyValue::ReferenceList(list),
                            } = property
                            {
                                list.items[0] =
                                    MetadataReference::parse("Enum.Status.EnumValue.Renamed")
                                        .unwrap();
                            }
                        }
                    }
                }
                fs::write(path, owner.to_yaml().unwrap()).unwrap();
            }
            let output = test.0.join("generations");
            if fixed && !frozen {
                assert!(
                    prepare_protected(&source, &base, &output, true)
                        .unwrap()
                        .diagnostics()
                        .is_empty()
                );
            } else {
                for strict in [true, false] {
                    assert_eq!(
                        prepare_protected(&source, &base, &output, strict)
                            .unwrap_err()
                            .error
                            .code,
                        "source_metadata_reference_missing"
                    );
                    assert!(!output.exists());
                }
            }
        }
    }
}

/// Case-only changes preserve the existing reference namespace, including preserved UUID/name mentions.
#[test]
fn case_only_rename_keeps_the_existing_reference_namespace() {
    let test = TestDir::new();
    let (source, base) = fixture_with(&test, true, &[], |input| {
        fs::create_dir_all(input.join("Enums/Status/Ext")).unwrap();
        fs::write(
            input.join("Enums/Status/Ext/Links.xml"),
            "<Links>Enum.Status.EnumValue.First</Links>",
        )
        .unwrap();
    });
    edit(&source, |owner| name(member(owner, 11), "fIrSt"));
    assert!(
        prepare_protected(&source, &base, &test.0.join("generations"), true)
            .unwrap()
            .diagnostics()
            .is_empty()
    );
}

/// Projection and working coordinates resolve the changed Name by accepted UUID in both source envelopes.
#[test]
fn saved_and_unsaved_readers_and_working_name_locations_share_the_renamed_value() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, &[]);
        let path = SourcePath::parse("Enums/Status/object.yaml").unwrap();
        let accepted = fs::read_to_string(source.join(path.as_path())).unwrap();
        let working = edit(&source, |owner| name(member(owner, 11), "Renamed"));
        let reader = SourceReader::open(&source).unwrap();
        let native = fs::read_to_string(test.0.join("native/Enums/Status.xml"))
            .unwrap()
            .replace("<Name>First</Name>", "<Name>Renamed</Name>");
        for mode in [PropertiesMode::All, PropertiesMode::Summary] {
            let saved = reader.read_metadata(&path, mode).unwrap();
            assert_eq!(saved, reader.parse_metadata(&path, &working, mode).unwrap());
            let parsed = metadata_parser::parse(&native, None, mode).unwrap();
            assert_eq!(
                saved.documents[0]
                    .descriptor
                    .objects
                    .iter()
                    .map(|node| (&node.metadata.uuid, &node.metadata.name))
                    .collect::<Vec<_>>(),
                parsed
                    .objects
                    .iter()
                    .map(|node| (&node.metadata.uuid, &node.metadata.name))
                    .collect::<Vec<_>>()
            );
        }
        let prepared =
            prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
        let path = SourcePath::parse("Enums/Status.xml").unwrap();
        let bytes = fs::read(prepared.xml().join(path.as_path())).unwrap();
        let at = std::str::from_utf8(&bytes)
            .unwrap()
            .find("Renamed")
            .unwrap();
        let location = prepared
            .source_map()
            .file(&path)
            .unwrap()
            .locate(
                &bytes,
                at..at + 1,
                Some(OriginTexts {
                    accepted: &accepted,
                    working: Some(&working),
                }),
            )
            .unwrap();
        assert_eq!(location.authority, InputAuthority::Working);
        assert!(working[location.range.unwrap()].contains("Renamed"));
        assert!(
            Descriptor::from_xml(&native)
                .unwrap()
                .to_xml()
                .unwrap()
                .contains(uuid(11).as_str())
        );
    }
}
