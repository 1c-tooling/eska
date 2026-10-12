use std::{
    fs,
    path::{Path, PathBuf},
};

use super::*;
use crate::{
    project_core::{
        MetadataKind,
        metadata_parser::{self, PropertiesMode},
        source_format::{
            AcceptedBase, Child, Descriptor, InputAuthority, MetadataNode, NativeUuid,
            ObjectSource, Operation, OriginTexts, PermissionReason, Property, PropertyName,
            PropertyValue, Section, SourcePath, SourceReader,
            enum_test_support::{self, native, uuid},
            files::Inventory,
            prepare_property_edit, prepare_protected,
        },
    },
    test_support::TestDir,
};

const FIRST: &str = "Enum.Status.EnumValue.First";
const SECOND: &str = "Enum.Status.EnumValue.Second";

/// Native annotations use explicit aliases, independently of the usual xsi/xr spellings.
fn namespaces(text: &str) -> String {
    text.replace(
        "xmlns:v8=",
        &format!("xmlns:i=\"{XSI_NAMESPACE}\" xmlns:r=\"{READABLE_NAMESPACE}\" xmlns:v8="),
    )
}

/// A four-owner profile test exercises parsing without relying on a serializer to manufacture input.
fn descriptor(owner: MetadataKind, fill: &str) -> Descriptor {
    let text = namespaces(&native(owner.designer_tag(), 5, "Sample", ""))
        .replace("<Comment/>", &format!("<Comment/>{fill}"));
    Descriptor::from_xml(&text).unwrap()
}

/// The trusted configuration declares both enum targets and a separate inline attribute dependency.
fn fixture(directory: &TestDir, readable: bool, frozen: &[NativeUuid]) -> (PathBuf, AcceptedBase) {
    enum_test_support::fixture_with(directory, readable, frozen, |input| {
        let path = input.join("Configuration.xml");
        let text = fs::read_to_string(&path).unwrap().replace(
            "<Enum>Status</Enum>",
            "<Enum>Status</Enum><Catalog>Items</Catalog>",
        );
        fs::write(path, text).unwrap();
        fs::create_dir(input.join("Catalogs")).unwrap();
        let children = format!(
            "<Attribute uuid=\"{}\"><Properties><Name>Status</Name><Synonym/><Comment/><FillValue i:type=\"r:DesignTimeRef\">{FIRST}</FillValue></Properties></Attribute>",
            uuid(5).as_str()
        );
        let text = namespaces(&native("Catalog", 4, "Items", &children));
        let descriptor = Descriptor::from_xml(&text).unwrap();
        fs::write(
            input.join("Catalogs/Items.xml"),
            descriptor.to_xml().unwrap(),
        )
        .unwrap();
    })
}

/// Resolve the sole accepted attribute rather than relying on its ordinal in unrelated sections.
fn attribute(owner: &mut ObjectSource) -> &mut MetadataNode {
    owner
        .document
        .descriptor
        .object
        .sections
        .iter_mut()
        .find_map(|section| match section {
            Section::Children(children) => children.iter_mut().find_map(|child| match child {
                Child::Inline { object } if object.uuid == uuid(5) => Some(object.as_mut()),
                _ => None,
            }),
            _ => None,
        })
        .unwrap()
}

/// Retarget exactly the accepted property while preserving its native annotation and owner.
fn retarget(node: &mut MetadataNode, target: &str) {
    fill(node).reference = MetadataReference::parse(target).unwrap();
}

/// Resolve only the typed `FillValue` authority, excluding coincidental preserved text.
fn fill(node: &mut MetadataNode) -> &mut EnumFillValue {
    node.sections
        .iter_mut()
        .find_map(|section| match section {
            Section::Properties(properties) => {
                properties.iter_mut().find_map(|property| match property {
                    Property::Typed {
                        name: PropertyName::FillValue,
                        value: PropertyValue::EnumFillValue(value),
                    } => Some(value.as_mut()),
                    _ => None,
                })
            }
            _ => None,
        })
        .unwrap()
}

/// Raw authoring is separate from editor validation, including negative protected-selector tests.
fn author(source: &Path, change: impl FnOnce(&mut ObjectSource)) -> String {
    let path = source.join("Catalogs/Items/object.yaml");
    let mut value = ObjectSource::from_yaml(&fs::read_to_string(&path).unwrap()).unwrap();
    change(&mut value);
    let text = format!(
        "# Keep annotation\r\n{}",
        crate::project_core::source_format::yaml::encode(&serde_json::to_value(&value).unwrap())
            .unwrap()
            .replace('\n', "\r\n")
    );
    fs::write(path, &text).unwrap();
    text
}

/// Enum targets have one editable copy and retain their `QName`, BOM and CRLF in both envelopes.
#[test]
fn named_enum_fill_value_round_trips_in_all_four_observed_owners() {
    for owner in [
        MetadataKind::Attribute,
        MetadataKind::CommonAttribute,
        MetadataKind::Dimension,
        MetadataKind::Resource,
    ] {
        let mut original = descriptor(
            owner,
            &format!("<FillValue i:type=\"r:DesignTimeRef\">{FIRST}</FillValue>"),
        );
        let Some(PropertyValue::EnumFillValue(value)) =
            original.object.property(PropertyName::FillValue)
        else {
            panic!("fill")
        };
        assert_eq!(value.reference.as_str(), FIRST);
        assert_eq!(value.native.xsi_prefix, "i");
        assert_eq!(value.native.type_prefix, "r");
        for schema in [1, 2] {
            original.schema_version = schema;
            let yaml = original.to_yaml().unwrap();
            assert_eq!(yaml.matches(FIRST).count(), 1);
            let restored = Descriptor::from_yaml(&yaml).unwrap();
            assert_eq!(restored, original);
            let xml = restored.to_xml().unwrap();
            assert!(xml.starts_with('\u{feff}'));
            assert!(!xml.replace("\r\n", "").contains('\n'));
            assert!(xml.contains(&format!(
                "<FillValue i:type=\"r:DesignTimeRef\">{FIRST}</FillValue>"
            )));
            let mut decoded = Descriptor::from_xml(&xml).unwrap();
            decoded.schema_version = schema;
            assert_eq!(decoded, original);
        }
    }
}

/// Coincidental names and unproven annotations stay complete preserved fragments, including nil/empty.
#[test]
fn unsupported_filling_values_remain_opaque() {
    for fill in [
        "<FillValue/>",
        "<FillValue i:nil=\"true\"/>",
        "<FillValue i:type=\"r:DesignTimeRef\">Enum.Status.EmptyRef</FillValue>",
        "<FillValue i:type=\"r:DesignTimeRef\">Catalog.Items.EmptyRef</FillValue>",
        "<FillValue i:type=\"r:DesignTimeRef\">20000000-0000-4000-8000-000000000001.20000000-0000-4000-8000-000000000002</FillValue>",
        "<FillValue i:type=\"r:DesignTimeRef\"><Unknown/></FillValue>",
        "<FillValue i:type=\"r:DesignTimeRef\">Enum.Status.EnumValue.First<!--keep--></FillValue>",
        "<FillValue i:type=\"r:Other\">Enum.Status.EnumValue.First</FillValue>",
        "<FillValue i:type=\"i:DesignTimeRef\">Enum.Status.EnumValue.First</FillValue>",
        "<FillValue i:type=\"missing:DesignTimeRef\">Enum.Status.EnumValue.First</FillValue>",
        "<FillValue i:type=\"r:DesignTimeRef\" custom=\"yes\">Enum.Status.EnumValue.First</FillValue>",
        "<FillValue xmlns:local=\"urn:private\" i:type=\"r:DesignTimeRef\">Enum.Status.EnumValue.First</FillValue>",
    ] {
        for owner in [MetadataKind::Attribute, MetadataKind::Catalog] {
            let value = descriptor(owner, fill);
            assert!(value.object.property(PropertyName::FillValue).is_none());
            assert!(value.to_xml().unwrap().contains(fill));
        }
    }
}

/// Retargeting validates declared targets, preserves authoring syntax and produces only the native value change.
#[test]
fn point_edit_retargets_with_exact_bytes_and_independent_native_control() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, &[]);
        let before = author(&source, |_| {});
        let source_before = Inventory::read(&source).unwrap();
        let accepted_before = Inventory::read(&test.0.join("accepted")).unwrap();
        let path = source.join("Catalogs/Items/object.yaml");
        let mut owner = ObjectSource::from_yaml(&before).unwrap();
        retarget(attribute(&mut owner), SECOND);
        let candidate = attribute(&mut owner)
            .property(PropertyName::FillValue)
            .unwrap()
            .clone();
        let plan = prepare_property_edit(
            &source,
            &base,
            &uuid(5),
            PropertyName::FillValue,
            candidate.clone(),
        )
        .unwrap();
        assert_eq!(plan.updated(), before.replace(FIRST, SECOND));
        assert_eq!(Inventory::read(&source).unwrap(), source_before);
        plan.publish(&base).unwrap();
        let no_op =
            prepare_property_edit(&source, &base, &uuid(5), PropertyName::FillValue, candidate)
                .unwrap();
        assert_eq!(no_op.updated(), fs::read_to_string(&path).unwrap());
        let selected =
            prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
        assert!(selected.diagnostics().is_empty());
        let expected = fs::read_to_string(test.0.join("native/Catalogs/Items.xml"))
            .unwrap()
            .replace(FIRST, SECOND);
        assert_eq!(
            fs::read_to_string(selected.xml().join("Catalogs/Items.xml")).unwrap(),
            expected
        );
        assert_eq!(
            Inventory::read(&test.0.join("accepted")).unwrap(),
            accepted_before
        );
        assert!(
            base.permission(&uuid(5), Operation::Property(PropertyName::FillValue))
                .allowed
        );
    }
}

/// A present object of another class or an unresolved name cannot become a valid enum dependency.
#[test]
fn target_shape_and_declared_existence_fail_before_edit_or_generation() {
    for readable in [false, true] {
        for (reference, code) in [
            (
                "Enum.Status.EnumValue.Missing",
                "source_metadata_reference_missing",
            ),
            ("Catalog.Items", "source_metadata_reference_invalid"),
        ] {
            let directory = TestDir::new();
            let (source, base) = fixture(&directory, readable, &[]);
            let original = fs::read_to_string(source.join("Catalogs/Items/object.yaml")).unwrap();
            let mut owner = ObjectSource::from_yaml(&original).unwrap();
            retarget(attribute(&mut owner), reference);
            let candidate =
                PropertyValue::EnumFillValue(Box::new(fill(attribute(&mut owner)).clone()));
            let error =
                prepare_property_edit(&source, &base, &uuid(5), PropertyName::FillValue, candidate)
                    .unwrap_err();
            assert_eq!(error.code, code);
            author(&source, |owner| retarget(attribute(owner), reference));
            let output = directory.0.join("generations");
            for strict in [false, true] {
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

/// XML namespace and owner binding checks reject coerced values in both serialized envelopes.
#[test]
fn invalid_bindings_and_scalar_states_are_not_filling_values() {
    let original = descriptor(
        MetadataKind::Attribute,
        &format!("<FillValue i:type=\"r:DesignTimeRef\">{FIRST}</FillValue>"),
    );
    for prefix in ["missing", "v8", "bad:name"] {
        let mut value = original.clone();
        for section in &mut value.object.sections {
            if let Section::Properties(properties) = section {
                for property in properties {
                    if let Property::Typed {
                        value: PropertyValue::EnumFillValue(value),
                        ..
                    } = property
                    {
                        value.native.type_prefix = prefix.into();
                    }
                }
            }
        }
        assert_eq!(
            value.validate().unwrap_err().code,
            "source_metadata_reference_namespace_invalid"
        );
    }
    for schema in [1, 2] {
        let mut descriptor = original.clone();
        descriptor.schema_version = schema;
        let mut raw = serde_json::to_value(&descriptor).unwrap();
        if schema == 2 {
            raw["object"]["properties"]["FillValue"] = serde_json::json!({"state":"nil"});
        } else {
            raw["object"]["sections"][0]["items"][3]["value"] = serde_json::json!({"state":"nil"});
        }
        assert!(
            Descriptor::from_yaml(&crate::project_core::source_format::yaml::encode(&raw).unwrap())
                .is_err()
        );
    }
}

/// The accepted ancestor or attribute freeze restores only forbidden values without publishing strict output.
#[test]
fn frozen_owner_or_value_cannot_retarget_a_dependency() {
    for readable in [false, true] {
        for frozen in [1, 4, 5] {
            let directory = TestDir::new();
            let (source, base) = fixture(&directory, readable, &[uuid(frozen)]);
            let authored = author(&source, |owner| retarget(attribute(owner), SECOND));
            let output = directory.0.join("generations");
            let failure = prepare_protected(&source, &base, &output, true).unwrap_err();
            assert_eq!(failure.error.code, "source_changes_excluded");
            assert!(!output.exists());
            assert_eq!(failure.diagnostics.len(), 1);
            assert_eq!(
                failure.diagnostics[0].operation,
                Operation::Property(PropertyName::FillValue)
            );
            assert_eq!(failure.diagnostics[0].reason, PermissionReason::EskaFrozen);
            let selected = prepare_protected(&source, &base, &output, false).unwrap();
            assert_eq!(
                fs::read_to_string(selected.xml().join("Catalogs/Items.xml")).unwrap(),
                fs::read_to_string(directory.0.join("native/Catalogs/Items.xml")).unwrap()
            );
            assert_eq!(
                fs::read_to_string(source.join("Catalogs/Items/object.yaml")).unwrap(),
                authored
            );
        }
    }
}

/// A modified reconstruction helper cannot grant authority or erase an independent valid retarget.
#[test]
fn tampered_prefix_is_restored_while_permitted_reference_is_selected() {
    for readable in [false, true] {
        let directory = TestDir::new();
        let (source, base) = fixture(&directory, readable, &[]);
        author(&source, |owner| {
            let value = fill(attribute(owner));
            value.reference = MetadataReference::parse(SECOND).unwrap();
            value.native.type_prefix = "i".into();
        });
        let output = directory.0.join("generations");
        let strict = prepare_protected(&source, &base, &output, true).unwrap_err();
        assert_eq!(strict.error.code, "source_changes_excluded");
        assert!(!output.exists());
        let selected = prepare_protected(&source, &base, &output, false).unwrap();
        assert_eq!(selected.diagnostics().len(), 1);
        let expected = fs::read_to_string(directory.0.join("native/Catalogs/Items.xml"))
            .unwrap()
            .replace(FIRST, SECOND);
        assert_eq!(
            fs::read_to_string(selected.xml().join("Catalogs/Items.xml")).unwrap(),
            expected
        );
    }
}

/// Structural edits validate the final selected reference; a forbidden repair preserves the old dependency.
#[test]
fn rename_and_delete_require_the_selected_fill_value_to_follow_composition() {
    for readable in [false, true] {
        for deleted in [false, true] {
            for frozen in [false, true] {
                let directory = TestDir::new();
                let freeze = if frozen { vec![uuid(5)] } else { Vec::new() };
                let (source, base) = fixture(&directory, readable, &freeze);
                enum_test_support::edit(&source, |owner| {
                    let children = enum_test_support::children(owner);
                    if deleted {
                        children.remove(0);
                    } else {
                        let Child::Inline { object } = &mut children[0] else {
                            panic!("value")
                        };
                        let Section::Properties(properties) = &mut object.sections[0] else {
                            panic!("properties")
                        };
                        let Property::Typed { value, .. } = &mut properties[0] else {
                            panic!("name")
                        };
                        *value = PropertyValue::Text("Renamed".into());
                    }
                });
                let output = directory.0.join("generations");
                assert_eq!(
                    prepare_protected(&source, &base, &output, false)
                        .unwrap_err()
                        .error
                        .code,
                    "source_metadata_reference_missing"
                );
                assert!(!output.exists());
                let target = if deleted {
                    SECOND
                } else {
                    "Enum.Status.EnumValue.Renamed"
                };
                author(&source, |owner| retarget(attribute(owner), target));
                if frozen {
                    assert_eq!(
                        prepare_protected(&source, &base, &output, false)
                            .unwrap_err()
                            .error
                            .code,
                        "source_metadata_reference_missing"
                    );
                    assert!(!output.exists());
                } else {
                    let selected = prepare_protected(&source, &base, &output, true).unwrap();
                    let xml =
                        fs::read_to_string(selected.xml().join("Catalogs/Items.xml")).unwrap();
                    assert!(xml.contains(target));
                    assert!(!xml.contains(FIRST));
                }
            }
        }
    }
}

/// Saved/unsaved readers and source-map coordinates share one editable reference and native type semantics.
#[test]
fn projection_and_working_locations_match_independent_native_properties() {
    for readable in [false, true] {
        let directory = TestDir::new();
        let (source, base) = fixture(&directory, readable, &[]);
        let path = SourcePath::parse("Catalogs/Items/object.yaml").unwrap();
        let accepted = fs::read_to_string(source.join(path.as_path())).unwrap();
        let working = author(&source, |owner| retarget(attribute(owner), SECOND));
        let reader = SourceReader::open(&source).unwrap();
        let native = fs::read_to_string(directory.0.join("native/Catalogs/Items.xml"))
            .unwrap()
            .replace(FIRST, SECOND);
        for mode in [PropertiesMode::All, PropertiesMode::Summary] {
            let saved = reader.read_metadata(&path, mode).unwrap();
            assert_eq!(saved, reader.parse_metadata(&path, &working, mode).unwrap());
            let parsed = metadata_parser::parse(&native, None, mode).unwrap();
            for (actual, expected) in saved.documents[0]
                .descriptor
                .objects
                .iter()
                .zip(&parsed.objects)
            {
                assert_eq!(actual.metadata, expected.metadata);
                assert_eq!(
                    actual
                        .properties
                        .as_ref()
                        .map(|items| items.iter().map(|v| &v.property).collect::<Vec<_>>()),
                    expected
                        .properties
                        .as_ref()
                        .map(|items| items.iter().map(|v| &v.property).collect::<Vec<_>>())
                );
            }
        }
        let prepared =
            prepare_protected(&source, &base, &directory.0.join("generations"), true).unwrap();
        let path = SourcePath::parse("Catalogs/Items.xml").unwrap();
        let bytes = fs::read(prepared.xml().join(path.as_path())).unwrap();
        let at = std::str::from_utf8(&bytes).unwrap().find(SECOND).unwrap();
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
        assert!(working[location.range.unwrap()].contains(SECOND));
    }
}

/// Migration follows reference/native field identity without changing native bytes or losing comments and quotes.
#[test]
fn annotated_family_migration_keeps_one_reference_and_native_prefix_annotations() {
    let directory = TestDir::new();
    let (source, _) = fixture(&directory, false, &[]);
    let original = fs::read_to_string(source.join("Catalogs/Items/object.yaml")).unwrap();
    let annotated = original
        .replace(
            &format!("\"reference\": \"{FIRST}\""),
            &format!("\"reference\": '{FIRST}' # target note"),
        )
        .replace(
            "\"xsi_prefix\": \"i\"",
            "\"xsi_prefix\": 'i' # annotation note",
        );
    for input in [annotated.clone(), annotated.replace('\n', "\r\n")] {
        let upgraded = crate::project_core::source_format::readable::upgrade_text(&input).unwrap();
        assert_eq!(upgraded.matches(FIRST).count(), 1);
        assert_eq!(upgraded.matches("# target note").count(), 1);
        assert_eq!(upgraded.matches("# annotation note").count(), 1);
        assert!(upgraded.contains(&format!("'{FIRST}'")));
        assert_eq!(
            ObjectSource::from_yaml(&upgraded)
                .unwrap()
                .document
                .descriptor
                .to_xml()
                .unwrap(),
            ObjectSource::from_yaml(&input)
                .unwrap()
                .document
                .descriptor
                .to_xml()
                .unwrap()
        );
        assert_eq!(upgraded.contains("\r\n"), input.contains("\r\n"));
    }
}

/// Changing only native letter case leaves the existing enum filling dependency valid.
#[test]
fn case_only_enum_rename_preserves_the_original_filling_reference() {
    for readable in [false, true] {
        let directory = TestDir::new();
        let (source, base) = fixture(&directory, readable, &[]);
        enum_test_support::edit(&source, |owner| {
            let Child::Inline { object } = &mut enum_test_support::children(owner)[0] else {
                panic!("value")
            };
            let Section::Properties(properties) = &mut object.sections[0] else {
                panic!("properties")
            };
            let Property::Typed { value, .. } = &mut properties[0] else {
                panic!("name")
            };
            *value = PropertyValue::Text("FIRST".into());
        });
        let prepared =
            prepare_protected(&source, &base, &directory.0.join("generations"), true).unwrap();
        assert!(prepared.diagnostics().is_empty());
        assert_eq!(
            fs::read_to_string(prepared.xml().join("Catalogs/Items.xml")).unwrap(),
            fs::read_to_string(directory.0.join("native/Catalogs/Items.xml")).unwrap()
        );
    }
}

/// Existing preserved source/base classification is never promoted implicitly by new native import rules.
#[test]
fn an_older_preserved_fill_value_remains_locked_without_explicit_reimport() {
    for readable in [false, true] {
        let directory = TestDir::new();
        let (source, _) = fixture(&directory, readable, &[]);
        let fragment = crate::project_core::source_format::XmlFragment {
            xml: format!("<FillValue i:type=\"r:DesignTimeRef\">{FIRST}</FillValue>"),
            namespaces: Vec::new(),
        };
        author(&source, |owner| {
            for section in &mut attribute(owner).sections {
                if let Section::Properties(properties) = section {
                    for property in properties {
                        if matches!(
                            property,
                            Property::Typed {
                                name: PropertyName::FillValue,
                                ..
                            }
                        ) {
                            *property = Property::Preserved {
                                fragment: fragment.clone(),
                            };
                        }
                    }
                }
            }
        });
        let storage = directory.0.join("old-accepted");
        let pin =
            crate::project_core::source_format::accept_source(&source, &storage, &[]).unwrap();
        let base = AcceptedBase::open(&storage, &pin).unwrap();
        let native = fs::read_to_string(directory.0.join("native/Catalogs/Items.xml")).unwrap();
        let output = directory.0.join("generations");
        let initial = prepare_protected(&source, &base, &output, true).unwrap();
        assert_eq!(
            fs::read_to_string(initial.xml().join("Catalogs/Items.xml")).unwrap(),
            native
        );
        let path = source.join("Catalogs/Items/object.yaml");
        let authored = fs::read_to_string(&path).unwrap().replace(FIRST, SECOND);
        fs::write(&path, &authored).unwrap();
        assert_eq!(
            prepare_protected(&source, &base, &output, true)
                .unwrap_err()
                .error
                .code,
            "source_changes_excluded"
        );
        let restored = prepare_protected(&source, &base, &output, false).unwrap();
        assert_eq!(
            fs::read_to_string(restored.xml().join("Catalogs/Items.xml")).unwrap(),
            native
        );
        assert_eq!(
            restored.generation(),
            prepare_protected(&source, &base, &output, false)
                .unwrap()
                .generation()
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), authored);
    }
}
