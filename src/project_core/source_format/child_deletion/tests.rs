use std::fs;

use super::*;
use crate::{
    project_core::{
        metadata_parser::PropertiesMode,
        source_format::{
            Descriptor, InputAuthority, ObjectSource, OriginTexts, PermissionReason, Property,
            PropertyName, PropertyValue, Section, SourceReader,
            enum_test_support::{children, edit, fixture, fixture_with, native, uuid, value},
            export_designer,
            files::Inventory,
            prepare_protected,
        },
    },
    test_support::TestDir,
};

/// Removal uses only accepted UUIDs and leaves authoring, reverse-map and accepted bytes intact.
#[test]
fn ordered_subset_matches_control_and_repeats_without_changing_inputs() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, &[]);
        let accepted = Inventory::read(&test.0.join("accepted")).unwrap();
        let original = Inventory::read(&test.0.join("native")).unwrap();
        let manifest = fs::read(source.join("source.json")).unwrap();
        edit(&source, |owner| {
            children(owner).remove(0);
        });
        let authored = Inventory::read(&source).unwrap();
        let control = test.0.join("control");
        export_designer(&source, &control).unwrap();
        let output = test.0.join("generations");
        let prepared = prepare_protected(&source, &base, &output, true).unwrap();
        assert!(prepared.diagnostics().is_empty());
        assert_eq!(
            Inventory::read(prepared.xml()).unwrap(),
            Inventory::read(&control).unwrap()
        );
        assert_eq!(
            prepared.generation(),
            prepare_protected(&source, &base, &output, true)
                .unwrap()
                .generation()
        );
        assert!(base.permission(&uuid(11), Operation::Delete).allowed);
        assert_eq!(
            base.permission(&uuid(2), Operation::Delete).reason,
            PermissionReason::UnsupportedOperation
        );
        assert_eq!(Inventory::read(&source).unwrap(), authored);
        assert_eq!(Inventory::read(&test.0.join("accepted")).unwrap(), accepted);
        assert_eq!(Inventory::read(&test.0.join("native")).unwrap(), original);
        assert_eq!(fs::read(source.join("source.json")).unwrap(), manifest);
    }
}

/// An explicit empty array removes the accepted values but never removes the descriptor or service slot.
#[test]
fn empty_collection_has_no_ghost_values_or_manifest_edits() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, &[]);
        edit(&source, |owner| children(owner).clear());
        let prepared =
            prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
        let xml = fs::read_to_string(prepared.xml().join("Enums/Status.xml")).unwrap();
        let selected = Descriptor::from_xml(&xml).unwrap();
        assert!(
            matches!(&selected.object.sections[1], Section::Children(children) if children.is_empty())
        );
        assert!(xml.contains(uuid(2).as_str()));
        assert!(!xml.contains(uuid(11).as_str()));
        assert!(!xml.contains(uuid(12).as_str()));
    }
}

/// Owner/ancestor, deleted member and shifted sibling freeze each refuse the entire reduction.
#[test]
fn frozen_authority_restores_missing_members_without_hiding_other_edits() {
    for readable in [false, true] {
        for frozen in [1, 2, 11, 12] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[uuid(frozen)]);
            edit(&source, |owner| {
                children(owner).remove(0);
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
            let deletion: Vec<_> = failure
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.operation == Operation::Delete)
                .collect();
            assert_eq!(deletion.len(), 1);
            assert_eq!(
                deletion[0].uuid,
                Some(uuid(if frozen < 3 { 2 } else { frozen }))
            );
            assert_eq!(deletion[0].reason, PermissionReason::EskaFrozen);
            assert!(!output.exists());
            let prepared = prepare_protected(&source, &base, &output, false).unwrap();
            let xml = fs::read_to_string(prepared.xml().join("Enums/Status.xml")).unwrap();
            assert!(xml.contains(uuid(11).as_str()));
            assert!(xml.contains(uuid(12).as_str()));
            assert_eq!(xml.contains("Independent owner edit"), frozen > 2);
        }
    }
}

/// A frozen member at the same native index permits deleting later values without changing its authority.
#[test]
fn stationary_frozen_value_allows_removal_after_it() {
    let test = TestDir::new();
    let (source, base) = fixture(&test, true, &[uuid(11)]);
    edit(&source, |owner| {
        children(owner).pop();
    });
    let prepared = prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
    let xml = fs::read_to_string(prepared.xml().join("Enums/Status.xml")).unwrap();
    assert!(xml.contains(uuid(11).as_str()));
    assert!(!xml.contains(uuid(12).as_str()));
}

/// Reductions do not reinterpret an opaque accepted value or create permission for an external descriptor.
#[test]
fn unsupported_native_profile_and_external_form_removal_are_restored() {
    for form in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture_with(&test, true, &[], |input| {
            let path = input.join("Enums/Status.xml");
            let xml = fs::read_to_string(&path).unwrap();
            if form {
                fs::write(
                    &path,
                    xml.replace("</ChildObjects>", "<Form>Main</Form></ChildObjects>"),
                )
                .unwrap();
                fs::create_dir_all(input.join("Enums/Status/Forms")).unwrap();
                fs::write(
                    input.join("Enums/Status/Forms/Main.xml"),
                    native("Form", 20, "Main", ""),
                )
                .unwrap();
            } else {
                fs::write(
                    &path,
                    xml.replace("<Name>Second</Name>", "<Name>Second</Name><Unverified/>"),
                )
                .unwrap();
            }
        });
        edit(&source, |owner| {
            children(owner).pop();
        });
        let output = test.0.join("generations");
        let failure = prepare_protected(&source, &base, &output, true).unwrap_err();
        assert_eq!(failure.diagnostics.len(), 1);
        assert_eq!(failure.diagnostics[0].operation, Operation::Delete);
        assert_eq!(
            failure.diagnostics[0].reason,
            PermissionReason::UnsupportedOperation
        );
        assert!(!output.exists());
        let restored = prepare_protected(&source, &base, &output, false).unwrap();
        assert!(
            fs::read_to_string(restored.xml().join("Enums/Status.xml"))
                .unwrap()
                .contains(if form {
                    "<Form>Main</Form>"
                } else {
                    "<Name>Second</Name>"
                })
        );
    }
}

/// Addition, transfer, duplication or reordering with deletion cannot silently substitute old native identities.
#[test]
fn mixed_composition_changes_never_remove_an_accepted_value() {
    for variant in 0..4 {
        let test = TestDir::new();
        let (source, base) = fixture(&test, true, &[]);
        edit(&source, |owner| {
            let children = children(owner);
            children.remove(0);
            match variant {
                0 => children.push(value(100, "Third")),
                1 => children.push(value(3, "Foreign")),
                2 => children.push(children[0].clone()),
                _ => {
                    children.push(value(100, "Third"));
                    children.reverse();
                }
            }
        });
        let output = test.0.join("generations");
        assert!(prepare_protected(&source, &base, &output, true).is_err());
        assert!(!output.exists());
        if variant != 2 {
            let restored = prepare_protected(&source, &base, &output, false).unwrap();
            let xml = fs::read_to_string(restored.xml().join("Enums/Status.xml")).unwrap();
            assert!(xml.contains(uuid(11).as_str()));
            assert!(xml.contains(uuid(12).as_str()));
            assert!(!xml.contains(uuid(100).as_str()));
        }
    }
}

/// A preserved dependency is decoded, so entity escapes, case changes, CDATA and attributes cannot hide it.
#[test]
fn preserved_metadata_references_refuse_before_publication_in_both_schemas() {
    for readable in [false, true] {
        for dependency in [
            "<Unverified>Enum.Status.EnumValue.Second</Unverified>".into(),
            "<Unverified><![CDATA[enum.status.enumvalue.second]]></Unverified>".into(),
            "<Unverified>Enum.Status.EnumValue.<!-- split -->Second</Unverified>".into(),
            "<Unverified>Enum.Status.EnumValue.Se<?note keep?>cond</Unverified>".into(),
            "<Unverified target=\"Enum.Status.EnumValue.&#83;econd\"/>".into(),
            format!(
                "<Unverified>{}</Unverified>",
                uuid(12).as_str().to_uppercase()
            ),
        ] {
            let test = TestDir::new();
            let (source, base) = fixture_with(&test, readable, &[], |input| {
                let file = input.join("Enums/Other.xml");
                let xml = fs::read_to_string(&file)
                    .unwrap()
                    .replace("</Properties>", &format!("{dependency}</Properties>"));
                fs::write(file, xml).unwrap();
            });
            edit(&source, |owner| {
                children(owner).pop();
            });
            let before = Inventory::read(&source).unwrap();
            for strict in [true, false] {
                let output = test.0.join("generations");
                let failure = prepare_protected(&source, &base, &output, strict).unwrap_err();
                assert_eq!(failure.error.code, "source_structural_reference_preserved");
                assert!(failure.error.location.contains(&"Enums/Other.xml".into()));
                assert!(failure.error.location.contains(&uuid(12).as_str().into()));
                assert!(!output.exists());
            }
            assert_eq!(Inventory::read(&source).unwrap(), before);
        }
    }
}

/// Preserved transport attributes can carry unverified metadata identity and must not hide it.
#[test]
fn preserved_descriptor_and_object_attributes_can_block_deletion() {
    for marker in ["<Enum uuid=", "<MetaDataObject xmlns="] {
        let test = TestDir::new();
        let (source, base) = fixture_with(&test, true, &[], |input| {
            let path = input.join("Enums/Other.xml");
            let xml = fs::read_to_string(&path).unwrap().replace(
                marker,
                &marker.replacen(' ', " dependency=\"Enum.Status.EnumValue.Second\" ", 1),
            );
            fs::write(path, xml).unwrap();
        });
        edit(&source, |owner| {
            children(owner).pop();
        });
        let output = test.0.join("generations");
        let failure = prepare_protected(&source, &base, &output, true).unwrap_err();
        assert_eq!(failure.error.code, "source_structural_reference_preserved");
        assert!(!output.exists());
    }
}

/// Unknown XML resources cannot be assumed to have no references; allowed payload fixes use their selected bytes.
#[test]
fn resource_dependency_and_unverified_xml_block_removal_but_selected_fix_allows_it() {
    for readable in [false, true] {
        for payload in [
            "<Resource><Ref>Enum.Status.EnumValue.Second</Ref></Resource>",
            "<Resource><Ref>Enum.Status.EnumValue.Se<!-- split -->cond</Ref></Resource>",
            "<Resource><Ref>Enum.Status.EnumValue.Se<?note keep?>cond</Ref></Resource>",
            "<Resource>",
        ] {
            let test = TestDir::new();
            let (source, base) = fixture_with(&test, readable, &[], |input| {
                fs::create_dir_all(input.join("Enums/Other/Ext")).unwrap();
                fs::write(input.join("Enums/Other/Ext/Resource.xml"), payload).unwrap();
            });
            edit(&source, |owner| {
                children(owner).pop();
            });
            let output = test.0.join("generations");
            let failure = prepare_protected(&source, &base, &output, true).unwrap_err();
            assert_eq!(
                failure.error.code,
                if payload.ends_with("</Resource>") {
                    "source_structural_reference_preserved"
                } else {
                    "source_structural_xml_unverified"
                }
            );
            assert!(!output.exists());
            let physical = base
                .package
                .payload_sources
                .get(&SourcePath::parse("Enums/Other/Ext/Resource.xml").unwrap())
                .unwrap();
            fs::write(source.join(physical.as_path()), "<Resource/>").unwrap();
            let prepared = prepare_protected(&source, &base, &output, true).unwrap();
            assert_eq!(
                fs::read_to_string(prepared.xml().join("Enums/Other/Ext/Resource.xml")).unwrap(),
                "<Resource/>"
            );
        }
    }
}

/// A forbidden resource change is restored before dependency checking, retaining its blocking reference.
#[test]
fn protected_payload_cannot_hide_a_reference_to_a_deleted_value() {
    let test = TestDir::new();
    let (source, base) = fixture_with(&test, true, &[uuid(3)], |input| {
        fs::create_dir_all(input.join("Enums/Other/Ext")).unwrap();
        fs::write(
            input.join("Enums/Other/Ext/Resource.xml"),
            "<Resource>Enum.Status.EnumValue.Second</Resource>",
        )
        .unwrap();
    });
    edit(&source, |owner| {
        children(owner).pop();
    });
    let path = base
        .package
        .payload_sources
        .get(&SourcePath::parse("Enums/Other/Ext/Resource.xml").unwrap())
        .unwrap();
    fs::write(source.join(path.as_path()), "<Resource/>").unwrap();
    let output = test.0.join("generations");
    assert_eq!(
        prepare_protected(&source, &base, &output, false)
            .unwrap_err()
            .error
            .code,
        "source_structural_reference_preserved"
    );
    assert!(!output.exists());
}

/// Typed dependencies resolve against the final selection; a forbidden reference fix cannot bless deletion.
#[test]
fn typed_dependency_requires_an_allowed_fix_in_the_same_selected_composition() {
    for readable in [false, true] {
        for (fixed, frozen) in [(false, false), (true, false), (true, true)] {
            let test = TestDir::new();
            let frozen = if frozen { vec![uuid(30)] } else { Vec::new() };
            let (source, base) = fixture_with(&test, readable, &frozen, |input| {
                let file = input.join("Configuration.xml");
                let xml = fs::read_to_string(&file).unwrap().replace(
                    "</ChildObjects>",
                    "<Subsystem>Navigation</Subsystem></ChildObjects>",
                );
                fs::write(file, xml).unwrap();
                fs::create_dir_all(input.join("Subsystems")).unwrap();
                let xml = native("Subsystem", 30, "Navigation", "").replace(
                    "<MetaDataObject ",
                    "<MetaDataObject xmlns:xr=\"http://v8.1c.ru/8.3/xcf/readable\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" ",
                ).replace(
                    "</Properties>",
                    "<Content><xr:Item xsi:type=\"xr:MDObjectRef\">Enum.Status.EnumValue.Second</xr:Item></Content></Properties>",
                );
                fs::write(input.join("Subsystems/Navigation.xml"), xml).unwrap();
            });
            edit(&source, |owner| {
                children(owner).pop();
            });
            if fixed {
                let path = source.join("Subsystems/Navigation/object.yaml");
                let mut owner =
                    ObjectSource::from_yaml(&fs::read_to_string(&path).unwrap()).unwrap();
                for section in &mut owner.document.descriptor.object.sections {
                    if let Section::Properties(properties) = section {
                        for property in properties {
                            if let Property::Typed {
                                name: PropertyName::Content,
                                value,
                            } = property
                            {
                                *value = PropertyValue::Empty;
                            }
                        }
                    }
                }
                fs::write(path, owner.to_yaml().unwrap()).unwrap();
            }
            let output = test.0.join("generations");
            if fixed && frozen.is_empty() {
                let prepared = prepare_protected(&source, &base, &output, true).unwrap();
                assert!(
                    !fs::read_to_string(prepared.xml().join("Enums/Status.xml"))
                        .unwrap()
                        .contains(uuid(12).as_str())
                );
            } else {
                for strict in [true, false] {
                    let failure = prepare_protected(&source, &base, &output, strict).unwrap_err();
                    assert_eq!(failure.error.code, "source_metadata_reference_missing");
                    assert!(!output.exists());
                }
            }
        }
    }
}

/// Deleted members disappear from saved and unsaved semantic projections; surviving properties retain working coordinates.
#[test]
fn source_reader_and_map_resolve_remaining_values_by_identity() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, &[]);
        let path = source.join("Enums/Status/object.yaml");
        let accepted = fs::read_to_string(&path).unwrap();
        let working = edit(&source, |owner| {
            children(owner).remove(0);
            let Child::Inline { object } = &mut children(owner)[0] else {
                unreachable!()
            };
            let Section::Properties(properties) = &mut object.sections[0] else {
                unreachable!()
            };
            let Property::Typed { value, .. } = &mut properties[2] else {
                unreachable!()
            };
            *value = PropertyValue::Text("Remaining edit".into());
        });
        let reader = SourceReader::open(&source).unwrap();
        let owner = SourcePath::parse("Enums/Status/object.yaml").unwrap();
        for mode in [PropertiesMode::All, PropertiesMode::Summary] {
            let saved = reader.read_metadata(&owner, mode).unwrap();
            assert_eq!(
                saved,
                reader.parse_metadata(&owner, &working, mode).unwrap()
            );
            assert!(
                saved.documents[0]
                    .descriptor
                    .objects
                    .iter()
                    .all(|object| object.metadata.uuid != uuid(11).as_str())
            );
        }
        let prepared =
            prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
        let native = fs::read(prepared.xml().join("Enums/Status.xml")).unwrap();
        let offset = std::str::from_utf8(&native)
            .unwrap()
            .find("Remaining edit")
            .unwrap();
        let mapped = prepared
            .source_map()
            .files
            .iter()
            .find(|file| file.path.as_str() == "Enums/Status.xml")
            .unwrap();
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
        assert_eq!(location.uuid, Some(uuid(12)));
        assert_eq!(location.authority, InputAuthority::Working);
        assert!(working[location.range.unwrap()].contains("Remaining edit"));
    }
}
