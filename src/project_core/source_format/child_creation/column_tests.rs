//! Columns use the accepted tabular section's identity, policy and inherited native scope.

use std::{fs, path::Path};

use super::*;
use super::{
    super::enum_test_support::{children, uuid},
    attribute_tests::{attribute, attribute_xml, edit},
};
use crate::{
    project_core::{
        metadata_parser::PropertiesMode,
        source_format::{
            Descriptor, InputAuthority, ObjectSource, OriginTexts, PermissionReason, SourceReader,
            files::Inventory, prepare_protected,
        },
    },
    test_support::TestDir,
};

const CATALOG: &str = "Catalogs/Records/object.yaml";
const DOCUMENT: &str = "Documents/Entry/object.yaml";
const BOOLEAN: &str = "<v8:Type>xs:boolean</v8:Type>";
const STRING: &str = "<v8:Type>xs:string</v8:Type><v8:StringQualifiers><v8:Length>40</v8:Length><v8:AllowedLength>Variable</v8:AllowedLength></v8:StringQualifiers>";

/// Existing table identities and internal native bytes are independently accepted before authoring.
fn fixture(
    test: &TestDir,
    readable: bool,
    frozen: &[NativeUuid],
    adjust: impl FnOnce(&Path),
) -> (std::path::PathBuf, AcceptedBase) {
    super::attribute_tests::fixture(test, readable, frozen, |input| {
        for (path, table, name, columns) in [
            (
                "Catalogs/Records.xml",
                13,
                "Rows",
                attribute_xml(21, "Existing", BOOLEAN) + &attribute_xml(22, "Label", STRING),
            ),
            ("Documents/Entry.xml", 14, "Lines", String::new()),
        ] {
            let file = input.join(path);
            let native = fs::read_to_string(&file).unwrap();
            let table = format!(
                "<TabularSection uuid=\"{}\"><InternalInfo><FutureData>Accepted only</FutureData></InternalInfo><Properties><Name>{name}</Name><Synonym/><Comment/></Properties><ChildObjects>{columns}</ChildObjects></TabularSection>",
                uuid(table).as_str()
            );
            fs::write(
                file,
                native.replace("</ChildObjects>", &(table + "</ChildObjects>")),
            )
            .unwrap();
        }
        adjust(input);
    })
}

/// Select one real direct parent; a descendant's kind or UUID cannot invent this ownership.
fn table(owner: &mut ObjectSource, index: u32) -> &mut MetadataNode {
    children(owner)
        .iter_mut()
        .find_map(|child| {
            if let Child::Inline { object } = child
                && object.uuid == uuid(index)
            {
                Some(object.as_mut())
            } else {
                None
            }
        })
        .unwrap()
}

/// There is one authored column list in the accepted tabular section's native composition slot.
fn columns(table: &mut MetadataNode) -> &mut Vec<Child> {
    table
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

/// Both owner classes and source envelopes keep accepted internals, external inputs and repeatability.
#[test]
fn columns_extend_existing_and_empty_tables_without_granting_new_identity_authority() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, &[], |_| {});
        let inputs = [test.0.join("native"), test.0.join("accepted")];
        let before: Vec<_> = inputs
            .iter()
            .map(|path| Inventory::read(path).unwrap())
            .collect();
        edit(&source, CATALOG, |owner| {
            columns(table(owner, 13)).push(attribute(100, "Новое_Поле", STRING));
        });
        edit(&source, DOCUMENT, |owner| {
            columns(table(owner, 14)).push(attribute(101, "Flag", BOOLEAN));
        });
        let authored = Inventory::read(&source).unwrap();
        let output = test.0.join("generations");
        let prepared = prepare_protected(&source, &base, &output, true).unwrap();
        assert!(prepared.diagnostics().is_empty());
        for (path, index, name, types) in [
            ("Catalogs/Records.xml", 100, "Новое_Поле", STRING),
            ("Documents/Entry.xml", 101, "Flag", BOOLEAN),
        ] {
            let expected = fs::read_to_string(test.0.join("native").join(path))
                .unwrap()
                .replacen(
                    "</ChildObjects></TabularSection>",
                    &(attribute_xml(index, name, types) + "</ChildObjects></TabularSection>"),
                    1,
                );
            let actual = fs::read_to_string(prepared.xml().join(path)).unwrap();
            assert_eq!(
                Descriptor::from_xml(&actual).unwrap(),
                Descriptor::from_xml(&expected).unwrap()
            );
            assert!(actual.contains("<FutureData>Accepted only</FutureData>"));
        }
        for index in [13, 14] {
            assert!(base.permission(&uuid(index), Operation::Create).allowed);
        }
        assert_eq!(
            base.permission(&uuid(100), Operation::Create).reason,
            PermissionReason::UnknownObject
        );
        for operation in [Operation::Create, Operation::Delete, Operation::Rename] {
            assert_eq!(
                base.permission(&uuid(21), operation).reason,
                PermissionReason::UnsupportedOperation
            );
        }
        assert_eq!(
            prepared.generation(),
            prepare_protected(&source, &base, &output, true)
                .unwrap()
                .generation()
        );
        assert_eq!(Inventory::read(&source).unwrap(), authored);
        assert_eq!(
            before,
            inputs
                .iter()
                .map(|path| Inventory::read(path).unwrap())
                .collect::<Vec<_>>()
        );
    }
}

/// A table, root or configuration freeze denies creation and preserves independent edits in recovery.
#[test]
fn column_creation_inherits_table_and_ancestor_freezes() {
    for readable in [false, true] {
        for frozen in [1, 2, 13] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[uuid(frozen)], |_| {});
            edit(&source, CATALOG, |owner| {
                columns(table(owner, 13)).push(attribute(100, "NewColumn", BOOLEAN));
            });
            let authored = Inventory::read(&source).unwrap();
            let output = test.0.join("generations");
            let failure = prepare_protected(&source, &base, &output, true).unwrap_err();
            assert_eq!(failure.error.code, "source_changes_excluded");
            assert_eq!(failure.diagnostics.len(), 1);
            assert_eq!(failure.diagnostics[0].operation, Operation::Create);
            assert_eq!(failure.diagnostics[0].uuid, Some(uuid(13)));
            assert_eq!(failure.diagnostics[0].reason, PermissionReason::EskaFrozen);
            assert!(!output.exists());
            let restored = prepare_protected(&source, &base, &output, false).unwrap();
            assert!(
                !fs::read_to_string(restored.xml().join("Catalogs/Records.xml"))
                    .unwrap()
                    .contains("NewColumn")
            );
            assert_eq!(Inventory::read(&source).unwrap(), authored);
        }
    }
}

/// Creation checks every displaced column while retaining unrelated permitted property changes.
#[test]
fn frozen_column_can_stay_in_place_but_cannot_be_displaced_by_new_columns() {
    for readable in [false, true] {
        for inserted_first in [false, true] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[uuid(21)], |_| {});
            edit(&source, CATALOG, |owner| {
                let columns = columns(table(owner, 13));
                let Child::Inline { object } = &mut columns[1] else {
                    unreachable!()
                };
                let Section::Properties(properties) = &mut object.sections[0] else {
                    unreachable!()
                };
                let Property::Typed { value, .. } = &mut properties[2] else {
                    unreachable!()
                };
                *value = PropertyValue::Text("Independent column edit".into());
                columns.insert(
                    if inserted_first { 0 } else { 2 },
                    attribute(100, "NewColumn", BOOLEAN),
                );
            });
            let output = test.0.join("generations");
            if inserted_first {
                let failure = prepare_protected(&source, &base, &output, true).unwrap_err();
                assert_eq!(failure.diagnostics.len(), 1);
                assert_eq!(failure.diagnostics[0].uuid, Some(uuid(21)));
                assert_eq!(failure.diagnostics[0].reason, PermissionReason::EskaFrozen);
                assert!(!output.exists());
            }
            let prepared = prepare_protected(&source, &base, &output, !inserted_first).unwrap();
            let xml = fs::read_to_string(prepared.xml().join("Catalogs/Records.xml")).unwrap();
            assert_eq!(xml.contains("NewColumn"), !inserted_first);
            assert!(xml.contains("Independent column edit"));
        }
    }
}

/// Columns have a native local field namespace and the same closed type/profile gates as attributes.
#[test]
fn column_names_and_shapes_refuse_collisions_and_unsupported_native_state() {
    for fault in [
        "collision",
        "name",
        "class",
        "opaque",
        "missing_type",
        "namespace",
    ] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, true, &[], |_| {});
        edit(&source, CATALOG, |owner| {
            let Child::Inline { mut object } = attribute(
                100,
                if fault == "collision" {
                    "EXISTING"
                } else if fault == "name" {
                    "Bad column"
                } else {
                    "NewColumn"
                },
                BOOLEAN,
            ) else {
                unreachable!()
            };
            match fault {
                "class" => object.kind = MetadataKind::Dimension,
                "opaque" => object
                    .sections
                    .push(Section::Preserved(super::super::XmlFragment {
                        xml: "<Future/>".into(),
                        namespaces: Vec::new(),
                    })),
                "missing_type" => {
                    let Section::Properties(properties) = &mut object.sections[0] else {
                        unreachable!()
                    };
                    properties.pop();
                }
                "namespace" => object.namespaces.push(super::super::Namespace {
                    prefix: Some("f".into()),
                    uri: "urn:future".into(),
                }),
                _ => {}
            }
            columns(table(owner, 13)).push(Child::Inline { object });
        });
        let output = test.0.join("generations");
        let failure = prepare_protected(&source, &base, &output, true).unwrap_err();
        assert_eq!(
            failure.error.code,
            match fault {
                "collision" => "source_metadata_name_collision",
                "name" => "source_metadata_name_invalid",
                _ => "source_child_creation_profile_unsupported",
            },
            "{fault}"
        );
        assert!(!output.exists());
    }
    let test = TestDir::new();
    let (source, base) = fixture(&test, true, &[], |_| {});
    edit(&source, CATALOG, |owner| {
        columns(table(owner, 13)).push(attribute(100, "Rows", BOOLEAN));
    });
    assert!(prepare_protected(&source, &base, &test.0.join("generations"), true).is_ok());
}

/// Creation cannot transfer accepted identities, create a parent or combine column removal/rename/reorder.
#[test]
fn column_creation_does_not_authorize_other_structural_changes() {
    for fault in [
        "foreign_uuid",
        "delete",
        "rename",
        "order",
        "new_table",
        "unknown_slot",
    ] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, true, &[], |input| {
            if fault == "unknown_slot" {
                let path = input.join("Catalogs/Records.xml");
                fs::write(
                    &path,
                    fs::read_to_string(&path).unwrap().replacen(
                        "</ChildObjects></TabularSection>",
                        "<!-- unknown slot --></ChildObjects></TabularSection>",
                        1,
                    ),
                )
                .unwrap();
            }
        });
        edit(&source, CATALOG, |owner| {
            let parent = table(owner, 13);
            if fault == "new_table" {
                parent.uuid = uuid(99);
            }
            let columns = columns(parent);
            match fault {
                "delete" => {
                    columns.remove(0);
                }
                "order" => columns.reverse(),
                "rename" => {
                    let Child::Inline { object } = &mut columns[0] else {
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
                _ => {}
            }
            columns.push(attribute(
                if fault == "foreign_uuid" { 4 } else { 100 },
                "NewColumn",
                BOOLEAN,
            ));
        });
        let output = test.0.join("generations");
        assert!(
            prepare_protected(&source, &base, &output, true).is_err(),
            "{fault}"
        );
        assert!(!output.exists());
    }
}

/// Parentage comes from acceptance even when working kinds try to turn an unsupported owner into a catalog.
#[test]
fn tabular_sections_of_other_root_classes_cannot_create_columns() {
    let test = TestDir::new();
    let (source, base) = fixture(&test, true, &[], |input| {
        let path = input.join("Catalogs/Records.xml");
        let xml = fs::read_to_string(&path)
            .unwrap()
            .replace("<Catalog uuid=", "<DataProcessor uuid=")
            .replace("</Catalog>", "</DataProcessor>");
        fs::write(path, xml).unwrap();
        fs::create_dir(input.join("DataProcessors")).unwrap();
        fs::rename(
            input.join("Catalogs/Records.xml"),
            input.join("DataProcessors/Records.xml"),
        )
        .unwrap();
        let configuration = input.join("Configuration.xml");
        fs::write(
            &configuration,
            fs::read_to_string(&configuration).unwrap().replace(
                "<Catalog>Records</Catalog>",
                "<DataProcessor>Records</DataProcessor>",
            ),
        )
        .unwrap();
    });
    assert_eq!(
        base.permission(&uuid(13), Operation::Create).reason,
        PermissionReason::UnsupportedOperation
    );
    edit(&source, "DataProcessors/Records/object.yaml", |owner| {
        owner.document.descriptor.object.kind = MetadataKind::Catalog;
        columns(table(owner, 13)).push(attribute(100, "NewColumn", BOOLEAN));
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
    let xml = fs::read_to_string(restored.xml().join("DataProcessors/Records.xml")).unwrap();
    assert!(!xml.contains("NewColumn"));
    assert!(xml.contains("<DataProcessor uuid="));
}

/// A type target is checked against final composition even for an accepted table without old columns.
#[test]
fn named_column_types_require_existing_targets_without_allow_exclusions_bypass() {
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
                columns(table(owner, 14)).push(Child::Inline { object });
            });
            let output = test.0.join("generations");
            for strict in [false, true] {
                let prepared = prepare_protected(&source, &base, &output, strict);
                if missing {
                    assert_eq!(
                        prepared.unwrap_err().error.code,
                        "source_type_target_missing"
                    );
                    assert!(!output.exists());
                } else {
                    assert!(prepared.unwrap().diagnostics().is_empty());
                }
            }
        }
    }
}

/// The accepted table's namespace shadowing also governs type validation and new-node source locations.
#[test]
fn created_columns_use_actual_parent_scope_and_verified_working_locations() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, &[], |input| {
            let path = input.join("Documents/Entry.xml");
            let xml = fs::read_to_string(&path).unwrap().replace("<TabularSection uuid=", "<TabularSection xmlns:scalar=\"http://v8.1c.ru/8.1/data/core\" xmlns:schema=\"http://www.w3.org/2001/XMLSchema\" xmlns:v8=\"urn:shadowed\" uuid=");
            fs::write(path, xml).unwrap();
        });
        let accepted = fs::read_to_string(source.join(DOCUMENT)).unwrap();
        let working = edit(&source, DOCUMENT, |owner| {
            let Child::Inline { mut object } = attribute(100, "NewColumn", BOOLEAN) else {
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
            value.native.prefix = "scalar".into();
            value.data.alternatives[0].name = "schema:boolean".into();
            columns(table(owner, 14)).push(Child::Inline { object });
        });
        let prepared =
            prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
        let native = fs::read(prepared.xml().join("Documents/Entry.xml")).unwrap();
        let xml = std::str::from_utf8(&native).unwrap();
        let mapped = prepared
            .source_map()
            .files
            .iter()
            .find(|file| file.path.as_str() == "Documents/Entry.xml")
            .unwrap();
        let field_start = xml.find(uuid(100).as_str()).unwrap();
        for (needle, value) in [
            (uuid(100).as_str(), "NewColumn"),
            ("<Name>NewColumn</Name>", "NewColumn"),
            (
                "<scalar:Type>schema:boolean</scalar:Type>",
                "schema:boolean",
            ),
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
            assert!(working[location.range.unwrap()].contains(value));
            for text in [None, Some(working.replace("NewColumn", "Tampered"))] {
                assert!(
                    mapped
                        .locate(
                            &native,
                            offset..offset + 1,
                            Some(OriginTexts {
                                accepted: &accepted,
                                working: text.as_deref()
                            })
                        )
                        .is_err()
                );
            }
        }
        let reader = SourceReader::open(&source).unwrap();
        let path = SourcePath::parse(DOCUMENT).unwrap();
        for mode in [PropertiesMode::All, PropertiesMode::Summary] {
            let saved = reader.read_metadata(&path, mode).unwrap();
            assert_eq!(saved, reader.parse_metadata(&path, &working, mode).unwrap());
            assert!(
                saved.documents[0]
                    .descriptor
                    .objects
                    .iter()
                    .any(|object| object.metadata.uuid == uuid(100).as_str()
                        && object.metadata.name == "NewColumn")
            );
        }
        edit(&source, DOCUMENT, |owner| {
            columns(table(owner, 14)).push(attribute(101, "WrongScope", BOOLEAN));
        });
        let output = test.0.join("invalid-generation");
        assert!(prepare_protected(&source, &base, &output, true).is_err());
        assert!(!output.exists());
    }
}
