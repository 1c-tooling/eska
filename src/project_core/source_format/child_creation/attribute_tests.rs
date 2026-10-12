//! Attribute construction shares identity and policy selection while retaining its own closed shape.

use std::{fs, path::Path};

use super::*;
use crate::{
    project_core::{
        metadata_parser::PropertiesMode,
        source_format::{
            Descriptor, InputAuthority, ObjectSource, OriginTexts, PermissionReason, SourceReader,
            accept_source, files::Inventory, import_designer, import_designer_readable,
            prepare_protected, yaml,
        },
    },
    test_support::TestDir,
};

use super::super::enum_test_support::{children, native, uuid};

const CATALOG: &str = "Catalogs/Records/object.yaml";
const DOCUMENT: &str = "Documents/Entry/object.yaml";
const BOOLEAN: &str = "<v8:Type>xs:boolean</v8:Type>";
const STRING: &str = "<v8:Type>xs:string</v8:Type><v8:StringQualifiers><v8:Length>40</v8:Length><v8:AllowedLength>Variable</v8:AllowedLength></v8:StringQualifiers>";

/// Fields have no generated identities or payloads; native qualifier declarations remain explicit.
pub(super) fn attribute_xml(index: u32, name: &str, types: &str) -> String {
    format!(
        "<Attribute uuid=\"{}\"><Properties><Name>{name}</Name><Synonym/><Comment/><Type>{types}</Type></Properties></Attribute>",
        uuid(index).as_str()
    )
}

/// Reuse the native decoder for the public type transport without copying qualifier values into slots.
pub(super) fn attribute(index: u32, name: &str, types: &str) -> Child {
    let descriptor = Descriptor::from_xml(&format!(
        "<MetaDataObject xmlns=\"{}\" xmlns:v8=\"http://v8.1c.ru/8.1/data/core\" xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" version=\"2.20\">{}</MetaDataObject>",
        super::super::MD_NAMESPACE,
        attribute_xml(index, name, types),
    ))
    .unwrap();
    Child::Inline {
        object: Box::new(descriptor.object),
    }
}

/// An ordinary configuration has two proven parent classes and a declared named type target.
pub(super) fn fixture(
    test: &TestDir,
    readable: bool,
    frozen: &[NativeUuid],
    adjust: impl FnOnce(&Path),
) -> (std::path::PathBuf, AcceptedBase) {
    let input = test.0.join("native");
    for folder in ["Catalogs", "Documents", "Enums"] {
        fs::create_dir_all(input.join(folder)).unwrap();
    }
    let context = |text: String| {
        text.replace(
            "version=\"2.20\"",
            "xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" xmlns:cfg=\"http://v8.1c.ru/8.1/data/enterprise/current-config\" xmlns:xr=\"http://v8.1c.ru/8.3/xcf/readable\" version=\"2.20\"",
        )
    };
    fs::write(
        input.join("Configuration.xml"),
        context(native(
            "Configuration",
            1,
            "Sample",
            "<Catalog>Records</Catalog><Document>Entry</Document><Enum>Status</Enum>",
        )),
    )
    .unwrap();
    fs::write(
        input.join("Catalogs/Records.xml"),
        context(native(
            "Catalog",
            2,
            "Records",
            &(attribute_xml(11, "Existing", BOOLEAN) + &attribute_xml(12, "Label", STRING)),
        )),
    )
    .unwrap();
    fs::write(
        input.join("Documents/Entry.xml"),
        context(native("Document", 3, "Entry", "")),
    )
    .unwrap();
    fs::write(
        input.join("Enums/Status.xml"),
        context(native("Enum", 4, "Status", "")).replace(
            "<Properties>",
            "<InternalInfo><xr:GeneratedType name=\"EnumRef.Status\" category=\"Ref\"/></InternalInfo><Properties>",
        ),
    )
    .unwrap();
    adjust(&input);
    let source = test.0.join("source");
    if readable {
        import_designer_readable(&input, &source).unwrap();
    } else {
        import_designer(&input, &source).unwrap();
    }
    let accepted = test.0.join("accepted");
    let pin = accept_source(&source, &accepted, frozen).unwrap();
    (source, AcceptedBase::open(&accepted, &pin).unwrap())
}

/// Authoring keeps caller comments and line endings, bypassing editor gates for negative inputs.
pub(super) fn edit(source: &Path, path: &str, change: impl FnOnce(&mut ObjectSource)) -> String {
    let path = source.join(path);
    let mut owner = ObjectSource::from_yaml(&fs::read_to_string(&path).unwrap()).unwrap();
    change(&mut owner);
    let text = format!(
        "# Preserve authored annotation\r\n{}",
        yaml::encode(&serde_json::to_value(owner).unwrap())
            .unwrap()
            .replace('\n', "\r\n")
    );
    fs::write(path, &text).unwrap();
    text
}

/// Both parent classes and envelopes retain independent base/native bytes, identities and repeatability.
#[test]
fn fresh_attributes_keep_native_types_and_inputs_without_becoming_accepted_objects() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, &[], |_| {});
        let inputs = [test.0.join("native"), test.0.join("accepted")];
        let before: Vec<_> = inputs.iter().map(|p| Inventory::read(p).unwrap()).collect();
        let manifest = fs::read(source.join("source.json")).unwrap();
        edit(&source, CATALOG, |owner| {
            children(owner).push(attribute(100, "Новое_Поле", STRING));
        });
        edit(&source, DOCUMENT, |owner| {
            children(owner).push(attribute(101, "Flag", BOOLEAN));
        });
        let authored = Inventory::read(&source).unwrap();
        let prepared =
            prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
        assert!(prepared.diagnostics().is_empty());
        for (path, index, name, types) in [
            ("Catalogs/Records.xml", 100, "Новое_Поле", STRING),
            ("Documents/Entry.xml", 101, "Flag", BOOLEAN),
        ] {
            let expected = fs::read_to_string(test.0.join("native").join(path))
                .unwrap()
                .replace(
                    "</ChildObjects>",
                    &(attribute_xml(index, name, types) + "</ChildObjects>"),
                );
            assert_eq!(
                Descriptor::from_xml(&fs::read_to_string(prepared.xml().join(path)).unwrap())
                    .unwrap(),
                Descriptor::from_xml(&expected).unwrap()
            );
        }
        assert!(base.permission(&uuid(2), Operation::Create).allowed);
        assert!(base.permission(&uuid(3), Operation::Create).allowed);
        assert_eq!(
            base.permission(&uuid(100), Operation::Create).reason,
            PermissionReason::UnknownObject
        );
        assert_eq!(
            base.permission(&uuid(11), Operation::Create).reason,
            PermissionReason::UnsupportedOperation
        );
        assert_eq!(
            base.permission(&uuid(11), Operation::Delete).reason,
            PermissionReason::UnsupportedOperation
        );
        assert_eq!(
            base.permission(&uuid(11), Operation::Rename).reason,
            PermissionReason::UnsupportedOperation
        );
        let repeated =
            prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
        assert_eq!(prepared.generation(), repeated.generation());
        assert_eq!(Inventory::read(&source).unwrap(), authored);
        assert_eq!(fs::read(source.join("source.json")).unwrap(), manifest);
        assert_eq!(
            before,
            inputs
                .iter()
                .map(|p| Inventory::read(p).unwrap())
                .collect::<Vec<_>>()
        );
    }
}

/// Named alternatives use the final declared generated-type index, including on an empty collection.
#[test]
fn new_attribute_types_resolve_targets_and_reject_missing_targets_before_publication() {
    for readable in [false, true] {
        for missing in [false, true] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[], |_| {});
            edit(&source, DOCUMENT, |owner| {
                let Child::Inline { mut object } = attribute(100, "Status", BOOLEAN) else {
                    unreachable!()
                };
                let Section::Properties(properties) = &mut object.sections[0] else {
                    unreachable!()
                };
                let Property::Typed {
                    value: PropertyValue::TypeDescription(value),
                    ..
                } = &mut properties[3]
                else {
                    unreachable!()
                };
                value.data.alternatives[0].name = if missing {
                    "cfg:EnumRef.Missing"
                } else {
                    "cfg:EnumRef.Status"
                }
                .into();
                children(owner).push(Child::Inline { object });
            });
            let output = test.0.join("generations");
            let prepared = prepare_protected(&source, &base, &output, true);
            if missing {
                assert_eq!(
                    prepared.unwrap_err().error.code,
                    "source_type_target_missing"
                );
                assert!(!output.exists());
            } else {
                assert!(
                    fs::read_to_string(prepared.unwrap().xml().join("Documents/Entry.xml"))
                        .unwrap()
                        .contains("cfg:EnumRef.Status")
                );
            }
        }
    }
}

/// Owner and ancestor freezes apply to fresh fields whose UUIDs have no accepted supplier entry.
#[test]
fn owner_freezes_refuse_creation_and_recovery_preserves_independent_allowed_values() {
    for readable in [false, true] {
        for frozen in [1, 2] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[uuid(frozen)], |_| {});
            edit(&source, CATALOG, |owner| {
                children(owner).push(attribute(100, "NewField", BOOLEAN));
            });
            let authored = Inventory::read(&source).unwrap();
            let output = test.0.join("generations");
            let failure = prepare_protected(&source, &base, &output, true).unwrap_err();
            assert_eq!(failure.error.code, "source_changes_excluded");
            assert_eq!(failure.diagnostics.len(), 1);
            assert_eq!(failure.diagnostics[0].operation, Operation::Create);
            assert_eq!(failure.diagnostics[0].uuid, Some(uuid(2)));
            assert_eq!(failure.diagnostics[0].reason, PermissionReason::EskaFrozen);
            assert!(!output.exists());
            let restored = prepare_protected(&source, &base, &output, false).unwrap();
            assert!(
                !fs::read_to_string(restored.xml().join("Catalogs/Records.xml"))
                    .unwrap()
                    .contains("NewField")
            );
            assert_eq!(Inventory::read(&source).unwrap(), authored);
        }
    }
}

/// Appending after a frozen field is valid, but insertion must not move its accepted native position.
#[test]
fn insertion_checks_displaced_fields_without_excluding_an_independent_comment() {
    for inserted_first in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, true, &[uuid(11)], |_| {});
        edit(&source, CATALOG, |owner| {
            let fields = children(owner);
            let Child::Inline { object } = &mut fields[1] else {
                unreachable!()
            };
            let Section::Properties(properties) = &mut object.sections[0] else {
                unreachable!()
            };
            let Property::Typed { value, .. } = &mut properties[2] else {
                unreachable!()
            };
            *value = PropertyValue::Text("Independent edit".into());
            fields.insert(
                if inserted_first { 0 } else { 2 },
                attribute(100, "NewField", BOOLEAN),
            );
        });
        let output = test.0.join("generations");
        if inserted_first {
            let failure = prepare_protected(&source, &base, &output, true).unwrap_err();
            assert_eq!(failure.diagnostics.len(), 1);
            assert_eq!(failure.diagnostics[0].uuid, Some(uuid(11)));
            assert_eq!(failure.diagnostics[0].reason, PermissionReason::EskaFrozen);
            assert!(!output.exists());
        }
        let prepared = prepare_protected(&source, &base, &output, !inserted_first).unwrap();
        let xml = fs::read_to_string(prepared.xml().join("Catalogs/Records.xml")).unwrap();
        assert_eq!(xml.contains("NewField"), !inserted_first);
        assert!(xml.contains("Independent edit"));
    }
}

/// Field names cannot shadow accepted attributes or tabular sections under native case-insensitive lookup.
#[test]
fn attribute_names_reject_invalid_spelling_and_field_namespace_collisions() {
    for readable in [false, true] {
        for (name, code) in [
            ("1Field", "source_metadata_name_invalid"),
            ("Bad field", "source_metadata_name_invalid"),
            ("existing", "source_metadata_name_collision"),
            ("ROWS", "source_metadata_name_collision"),
        ] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[], |input| {
                let path = input.join("Catalogs/Records.xml");
                let xml = fs::read_to_string(&path).unwrap().replace("</ChildObjects>", &format!("<TabularSection uuid=\"{}\"><Properties><Name>Rows</Name></Properties><ChildObjects/></TabularSection></ChildObjects>", uuid(13).as_str()));
                fs::write(path, xml).unwrap();
            });
            edit(&source, CATALOG, |owner| {
                children(owner).insert(2, attribute(100, name, BOOLEAN));
            });
            let output = test.0.join("generations");
            assert_eq!(
                prepare_protected(&source, &base, &output, true)
                    .unwrap_err()
                    .error
                    .code,
                code
            );
            assert!(!output.exists());
        }
    }
}

/// A new field has exactly four properties and cannot smuggle opaque state, namespaces or generated types.
#[test]
fn incomplete_or_extra_native_attribute_shapes_refuse_before_publication() {
    for fault in [
        "missing_type",
        "extra_property",
        "opaque",
        "attribute",
        "namespace",
        "wrong_class",
    ] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, true, &[], |_| {});
        edit(&source, CATALOG, |owner| {
            let Child::Inline { mut object } = attribute(100, "NewField", BOOLEAN) else {
                unreachable!()
            };
            match fault {
                "missing_type" => {
                    let Section::Properties(properties) = &mut object.sections[0] else {
                        unreachable!()
                    };
                    properties.pop();
                }
                "extra_property" => {
                    let Section::Properties(properties) = &mut object.sections[0] else {
                        unreachable!()
                    };
                    properties.push(Property::Typed {
                        name: PropertyName::PasswordMode,
                        value: PropertyValue::Boolean(false),
                    });
                }
                "opaque" => object
                    .sections
                    .push(Section::Preserved(super::super::XmlFragment {
                        xml: "<Future/>".into(),
                        namespaces: Vec::new(),
                    })),
                "attribute" => object.attributes.push(super::super::XmlAttribute {
                    name: "future".into(),
                    value: "value".into(),
                }),
                "namespace" => object.namespaces.push(super::super::Namespace {
                    prefix: Some("f".into()),
                    uri: "urn:future".into(),
                }),
                "wrong_class" => object.kind = MetadataKind::Dimension,
                _ => unreachable!(),
            }
            children(owner).push(Child::Inline { object });
        });
        let output = test.0.join("generations");
        assert_eq!(
            prepare_protected(&source, &base, &output, true)
                .unwrap_err()
                .error
                .code,
            "source_child_creation_profile_unsupported",
            "{fault}"
        );
        assert!(!output.exists());
    }
}

/// Types cannot omit reconstruction slots, use empty values or resolve prefixes against authored namespaces.
#[test]
fn invalid_new_type_values_and_namespace_bindings_never_create_native_bytes() {
    for fault in ["empty", "layout", "prefix", "primitive"] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, true, &[], |_| {});
        edit(&source, CATALOG, |owner| {
            let Child::Inline { mut object } = attribute(100, "NewField", BOOLEAN) else {
                unreachable!()
            };
            let Section::Properties(properties) = &mut object.sections[0] else {
                unreachable!()
            };
            let Property::Typed { value, .. } = &mut properties[3] else {
                unreachable!()
            };
            if fault == "empty" {
                *value = PropertyValue::Empty;
            } else if let PropertyValue::TypeDescription(value) = value {
                match fault {
                    "layout" => value.native.parts.clear(),
                    "prefix" => value.native.prefix = "xs".into(),
                    "primitive" => value.data.alternatives[0].name = "xs:Future".into(),
                    _ => unreachable!(),
                }
            }
            children(owner).push(Child::Inline { object });
        });
        let output = test.0.join("generations");
        assert!(
            prepare_protected(&source, &base, &output, true).is_err(),
            "{fault}"
        );
        assert!(!output.exists());
    }
}

/// Native class groups stay fixed; additions cannot be moved past a tabular section or unknown child.
#[test]
fn attributes_require_their_class_group_and_known_native_composition() {
    for preserved in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, true, &[], |input| {
            let path = input.join("Catalogs/Records.xml");
            let xml = fs::read_to_string(&path).unwrap().replace("</ChildObjects>", if preserved { "<!-- unknown slot --></ChildObjects>" } else { "<TabularSection uuid=\"20000000-0000-4000-8000-000000000013\"><Properties><Name>Rows</Name></Properties><ChildObjects/></TabularSection></ChildObjects>" });
            fs::write(path, xml).unwrap();
        });
        edit(&source, CATALOG, |owner| {
            children(owner).push(attribute(100, "NewField", BOOLEAN));
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
        let prepared = prepare_protected(&source, &base, &output, false).unwrap();
        assert!(
            !fs::read_to_string(prepared.xml().join("Catalogs/Records.xml"))
                .unwrap()
                .contains("NewField")
        );
    }
}

/// The existing structural selector must not enable deletion or renaming of attributes as a side effect.
#[test]
fn attribute_creation_does_not_enable_transfer_deletion_rename_or_reordering() {
    for fault in ["foreign_uuid", "delete", "rename", "order", "owner_class"] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, true, &[], |_| {});
        edit(&source, CATALOG, |owner| {
            let fields = children(owner);
            match fault {
                "delete" => {
                    fields.remove(0);
                }
                "rename" => {
                    let Child::Inline { object } = &mut fields[0] else {
                        unreachable!()
                    };
                    let Section::Properties(properties) = &mut object.sections[0] else {
                        unreachable!()
                    };
                    let Property::Typed { value, .. } = &mut properties[0] else {
                        unreachable!()
                    };
                    *value = PropertyValue::Text("Renamed".into());
                }
                "order" => fields.reverse(),
                "foreign_uuid" | "owner_class" => {}
                _ => unreachable!(),
            }
            fields.push(attribute(
                if fault == "foreign_uuid" { 4 } else { 100 },
                "NewField",
                BOOLEAN,
            ));
            if fault == "owner_class" {
                owner.document.descriptor.object.kind = MetadataKind::Enum;
            }
        });
        let output = test.0.join("generations");
        assert!(
            prepare_protected(&source, &base, &output, true).is_err(),
            "{fault}"
        );
        assert!(!output.exists());
    }
}

/// Created type data maps to verified working YAML and the common saved/overlay semantic projection.
#[test]
fn created_attribute_types_have_working_locations_and_saved_overlay_semantics() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, &[], |_| {});
        let accepted = fs::read_to_string(source.join(CATALOG)).unwrap();
        let working = edit(&source, CATALOG, |owner| {
            children(owner).push(attribute(100, "NewField", STRING));
        });
        let prepared =
            prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
        let native = fs::read(prepared.xml().join("Catalogs/Records.xml")).unwrap();
        let xml = std::str::from_utf8(&native).unwrap();
        let mapped = prepared
            .source_map()
            .files
            .iter()
            .find(|file| file.path.as_str() == "Catalogs/Records.xml")
            .unwrap();
        let field_start = xml.find(uuid(100).as_str()).unwrap();
        for needle in [
            uuid(100).as_str(),
            "<Name>NewField</Name>",
            "<v8:Length>40</v8:Length>",
        ] {
            let offset = field_start + xml[field_start..].find(needle).unwrap();
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
            let range = location.range.unwrap();
            assert!(working[range].contains(if needle.contains("Length") {
                "40"
            } else {
                "NewField"
            }));
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
            assert!(
                mapped
                    .locate(
                        &native,
                        offset..offset + 1,
                        Some(OriginTexts {
                            accepted: &accepted,
                            working: Some(&working.replace("NewField", "Tampered"))
                        })
                    )
                    .is_err()
            );
        }
        let reader = SourceReader::open(&source).unwrap();
        let path = SourcePath::parse(CATALOG).unwrap();
        for mode in [PropertiesMode::All, PropertiesMode::Summary] {
            let saved = reader.read_metadata(&path, mode).unwrap();
            assert_eq!(saved, reader.parse_metadata(&path, &working, mode).unwrap());
            assert!(
                saved.documents[0]
                    .descriptor
                    .objects
                    .iter()
                    .any(|object| object.metadata.uuid == uuid(100).as_str()
                        && object.metadata.name == "NewField")
            );
        }
    }
}
