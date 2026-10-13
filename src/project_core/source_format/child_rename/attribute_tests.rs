//! Attribute names preserve native UUID and actual parentage while guarding selected dependencies.

use std::fs;

use super::*;
use crate::{
    project_core::{
        metadata_parser::PropertiesMode,
        source_format::{
            Descriptor, InputAuthority, ObjectSource, OriginTexts, PermissionReason, SourceReader,
            attribute_test_support::{columns, comment, fixture, member, rename, table},
            child_creation::attribute_tests::{attribute_xml, edit},
            enum_test_support::{children, uuid},
            files::Inventory,
            prepare_protected,
        },
    },
    test_support::TestDir,
};

const CATALOG: &str = "Catalogs/Records/object.yaml";
const DOCUMENT: &str = "Documents/Entry/object.yaml";
const BOOLEAN: &str = "<v8:Type>xs:boolean</v8:Type>";

/// Four actual field namespaces support English/Cyrillic names without changing identity, type, inputs or repeatability.
#[test]
fn root_field_and_column_rename_keeps_uuid_native_values_and_inputs_in_both_envelopes() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, &[], |_| {});
        let before = [test.0.join("native"), test.0.join("accepted")]
            .map(|path| Inventory::read(&path).unwrap());
        let manifest = fs::read(source.join("source.json")).unwrap();
        edit(&source, CATALOG, |owner| {
            rename(owner, 11, "Changed_Field");
            rename(owner, 21, "Новая_Колонка");
        });
        edit(&source, DOCUMENT, |owner| {
            rename(owner, 31, "Новое_Поле");
            rename(owner, 41, "Changed_Column");
        });
        let authored = Inventory::read(&source).unwrap();
        let output = test.0.join("generations");
        let prepared = prepare_protected(&source, &base, &output, true).unwrap();
        assert!(prepared.diagnostics().is_empty());
        for (path, replacements) in [
            (
                "Catalogs/Records.xml",
                [
                    ("Existing", "Changed_Field"),
                    ("FirstColumn", "Новая_Колонка"),
                ],
            ),
            (
                "Documents/Entry.xml",
                [
                    ("FirstDocumentField", "Новое_Поле"),
                    ("FirstDocumentColumn", "Changed_Column"),
                ],
            ),
        ] {
            let mut expected = fs::read_to_string(test.0.join("native").join(path)).unwrap();
            for (before, after) in replacements {
                expected = expected.replace(
                    &format!("<Name>{before}</Name>"),
                    &format!("<Name>{after}</Name>"),
                );
            }
            let xml = fs::read_to_string(prepared.xml().join(path)).unwrap();
            assert_eq!(
                Descriptor::from_xml(&xml).unwrap(),
                Descriptor::from_xml(&expected).unwrap()
            );
            assert!(xml.contains("Retained table service bytes"));
        }
        assert!(base.permission(&uuid(11), Operation::Rename).allowed);
        assert!(
            !base
                .permission(&uuid(11), Operation::Property(PropertyName::Name))
                .allowed
        );
        // Class permission is necessary; the unknown table service profile remains outside the rename selector.
        assert!(base.permission(&uuid(13), Operation::Rename).allowed);
        assert_eq!(
            prepared.generation(),
            prepare_protected(&source, &base, &output, true)
                .unwrap()
                .generation()
        );
        assert_eq!(Inventory::read(&source).unwrap(), authored);
        assert_eq!(fs::read(source.join("source.json")).unwrap(), manifest);
        assert_eq!(
            [test.0.join("native"), test.0.join("accepted")]
                .map(|path| Inventory::read(&path).unwrap()),
            before
        );
    }
}

/// Parent/member freeze restores only the refused namespace while independently allowed document changes survive.
#[test]
fn actual_parent_and_member_freezes_report_rename_with_independent_recovery() {
    for readable in [false, true] {
        for frozen in [2, 11, 13, 21] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[uuid(frozen)], |_| {});
            let target = if frozen >= 13 { 21 } else { 11 };
            edit(&source, CATALOG, |owner| rename(owner, target, "Renamed"));
            edit(&source, DOCUMENT, |owner| {
                rename(owner, 31, "IndependentDocumentField");
                comment(
                    &mut owner.document.descriptor.object,
                    "Independent document comment",
                );
            });
            let output = test.0.join("generations");
            let failure = prepare_protected(&source, &base, &output, true).unwrap_err();
            let changes: Vec<_> = failure
                .diagnostics
                .iter()
                .filter(|d| d.operation == Operation::Rename)
                .collect();
            assert_eq!(changes.len(), 1);
            assert_eq!(changes[0].uuid, Some(uuid(frozen)));
            assert_eq!(changes[0].reason, PermissionReason::EskaFrozen);
            assert!(!output.exists());
            let selected = prepare_protected(&source, &base, &output, false).unwrap();
            let catalog = fs::read_to_string(selected.xml().join("Catalogs/Records.xml")).unwrap();
            assert!(!catalog.contains("<Name>Renamed</Name>"));
            let document = fs::read_to_string(selected.xml().join("Documents/Entry.xml")).unwrap();
            assert!(
                document.contains("IndependentDocumentField")
                    && document.contains("Independent document comment")
            );
        }
    }
}

/// A stationary frozen sibling and a root field's name do not govern another local table-column namespace.
#[test]
fn stationary_siblings_and_distinct_column_namespaces_do_not_refuse_rename() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, &[uuid(12), uuid(22)], |_| {});
        edit(&source, CATALOG, |owner| {
            rename(owner, 11, "SharedName");
            rename(owner, 21, "SharedName");
        });
        assert!(
            prepare_protected(&source, &base, &test.0.join("generations"), true)
                .unwrap()
                .diagnostics()
                .is_empty()
        );
    }
}

/// The final sibling namespace rejects invalid identifiers and collisions with attributes or tabular sections.
#[test]
fn invalid_and_colliding_field_names_refuse_before_any_generation() {
    for readable in [false, true] {
        for (target, next, code) in [
            (11, "", "source_metadata_name_invalid"),
            (11, "2Invalid", "source_metadata_name_invalid"),
            (11, "Invalid Name", "source_metadata_name_invalid"),
            (11, "LABEL", "source_metadata_name_collision"),
            (11, "Rows", "source_metadata_name_collision"),
            (21, "LASTCOLUMN", "source_metadata_name_collision"),
        ] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[], |_| {});
            edit(&source, CATALOG, |owner| rename(owner, target, next));
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

/// Names are checked after all UUID-addressed renames, allowing a unique simultaneous swap.
#[test]
fn simultaneous_name_swap_keeps_order_and_native_identities() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, &[], |_| {});
        edit(&source, CATALOG, |owner| {
            rename(owner, 11, "Label");
            rename(owner, 12, "Existing");
        });
        let prepared =
            prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
        let actual = Descriptor::from_xml(
            &fs::read_to_string(prepared.xml().join("Catalogs/Records.xml")).unwrap(),
        )
        .unwrap();
        let mut owner = ObjectSource {
            form: None,
            document: super::super::NativeDocument {
                path: SourcePath::parse("Catalogs/Records.xml").unwrap(),
                descriptor: actual,
            },
            nested: vec![],
        };
        assert_eq!(
            member(&mut owner.document.descriptor.object, 11)
                .unwrap()
                .property(PropertyName::Name),
            Some(&PropertyValue::Text("Label".into()))
        );
        assert_eq!(
            member(&mut owner.document.descriptor.object, 12)
                .unwrap()
                .property(PropertyName::Name),
            Some(&PropertyValue::Text("Existing".into()))
        );
        assert_eq!(
            children(&mut owner)[0],
            super::super::child_creation::attribute_tests::attribute(11, "Label", BOOLEAN)
        );
    }
}

/// Fresh members can reuse a freed spelling, but cannot retarget a remaining old-name dependency silently.
#[test]
fn rename_and_creation_reuse_names_without_bypassing_existing_dependencies() {
    for readable in [false, true] {
        for dependency in [false, true] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[], |input| {
                if dependency {
                    let directory = input.join("Documents/Entry/Ext");
                    fs::create_dir_all(&directory).unwrap();
                    fs::write(
                        directory.join("ObjectModule.bsl"),
                        "\u{feff}Объект.Existing = Истина;\r\n",
                    )
                    .unwrap();
                }
            });
            edit(&source, CATALOG, |owner| {
                rename(owner, 11, "RenamedField");
                rename(owner, 21, "RenamedColumn");
                children(owner).insert(
                    2,
                    super::super::child_creation::attribute_tests::attribute(
                        100, "Existing", BOOLEAN,
                    ),
                );
                columns(table(owner, 13)).push(
                    super::super::child_creation::attribute_tests::attribute(
                        101,
                        "FirstColumn",
                        BOOLEAN,
                    ),
                );
            });
            let output = test.0.join("generations");
            let result = prepare_protected(&source, &base, &output, true);
            if dependency {
                let error = result.unwrap_err().error;
                assert_eq!(error.code, "source_structural_reference_preserved");
                assert!(error.location.contains(&uuid(11).as_str().to_owned()));
                assert!(!output.exists());
            } else {
                let prepared = result.unwrap();
                let native =
                    fs::read_to_string(prepared.xml().join("Catalogs/Records.xml")).unwrap();
                for id in [11, 21, 100, 101] {
                    assert!(native.contains(uuid(id).as_str()));
                }
                assert!(
                    native.contains("<Name>RenamedField</Name>")
                        && native.contains("<Name>Existing</Name>")
                );
                assert!(
                    native.contains("<Name>RenamedColumn</Name>")
                        && native.contains("<Name>FirstColumn</Name>")
                );
                assert_eq!(
                    base.permission(&uuid(100), Operation::Rename).reason,
                    PermissionReason::UnknownObject
                );
            }
        }
    }
}

/// Unknown native fields or unsupported parents retain Name while unrelated permitted comments still apply.
#[test]
fn unsupported_attribute_profiles_and_parent_classes_do_not_expand_rename_authority() {
    for readable in [false, true] {
        for mode in [
            "unknown_property",
            "internal_info",
            "namespace",
            "children",
            "foreign_parent",
            "table_name",
        ] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[], |input| {
                let file = input.join("Catalogs/Records.xml");
                let original = fs::read_to_string(&file).unwrap();
                let field = attribute_xml(11, "Existing", BOOLEAN);
                let next = match mode {
                    "unknown_property" => {
                        field.replace("</Properties>", "<FutureProperty/></Properties>")
                    }
                    "internal_info" => field.replace(
                        "<Properties>",
                        "<InternalInfo><Future/></InternalInfo><Properties>",
                    ),
                    "namespace" => field.replace(
                        "<Attribute uuid=",
                        "<Attribute xmlns:future=\"urn:future\" uuid=",
                    ),
                    "children" => field.replace("</Attribute>", "<ChildObjects/></Attribute>"),
                    _ => field.clone(),
                };
                fs::write(&file, original.replace(&field, &next)).unwrap();
                if mode == "foreign_parent" {
                    let configuration = input.join("Configuration.xml");
                    fs::write(
                        &configuration,
                        fs::read_to_string(&configuration).unwrap().replace(
                            "<Catalog>Records</Catalog>",
                            "<DataProcessor>Records</DataProcessor>",
                        ),
                    )
                    .unwrap();
                    fs::rename(input.join("Catalogs"), input.join("DataProcessors")).unwrap();
                    let file = input.join("DataProcessors/Records.xml");
                    fs::write(
                        &file,
                        fs::read_to_string(&file)
                            .unwrap()
                            .replace("<Catalog uuid=", "<DataProcessor uuid=")
                            .replace("</Catalog>", "</DataProcessor>"),
                    )
                    .unwrap();
                }
            });
            let path = if mode == "foreign_parent" {
                "DataProcessors/Records/object.yaml"
            } else {
                CATALOG
            };
            edit(&source, path, |owner| {
                rename(owner, if mode == "table_name" { 13 } else { 11 }, "Renamed");
                comment(
                    &mut owner.document.descriptor.object,
                    "Independent owner comment",
                );
            });
            let output = test.0.join("generations");
            assert!(prepare_protected(&source, &base, &output, true).is_err());
            assert!(!output.exists());
            let selected = prepare_protected(&source, &base, &output, false).unwrap();
            let native = if mode == "foreign_parent" {
                "DataProcessors/Records.xml"
            } else {
                "Catalogs/Records.xml"
            };
            let xml = fs::read_to_string(selected.xml().join(native)).unwrap();
            assert!(
                !xml.contains("<Name>Renamed</Name>") && xml.contains("Independent owner comment")
            );
        }
    }
}

/// Old-name mentions in decoded XML or BSL refuse rename; UUID-only references retain the same live target.
#[test]
fn preserved_xml_and_bsl_guard_old_names_without_invalidating_the_live_uuid() {
    for readable in [false, true] {
        for (kind, denied) in [
            ("xml", true),
            ("comment", true),
            ("processing-instruction", true),
            ("bsl", true),
            ("uuid", false),
        ] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[], |input| {
                if kind == "bsl" {
                    let directory = input.join("Documents/Entry/Ext");
                    fs::create_dir_all(&directory).unwrap();
                    fs::write(
                        directory.join("ObjectModule.bsl"),
                        "\u{feff}Объект.Existing = Истина;\r\n",
                    )
                    .unwrap();
                } else {
                    let path = input.join("Documents/Entry.xml");
                    let xml = fs::read_to_string(&path).unwrap();
                    let reference = if kind == "uuid" {
                        format!("<Future uuidRef=\"{}\"/>", uuid(11).as_str())
                    } else if kind == "comment" {
                        "<Future>Ex<!-- split -->isting</Future>".into()
                    } else if kind == "processing-instruction" {
                        "<Future>Ex<?note keep?>isting</Future>".into()
                    } else {
                        "<Future value=\"Ex&#x69;sting\"/>".into()
                    };
                    fs::write(
                        path,
                        xml.replace(
                            "<Name>Entry</Name>",
                            &("<Name>Entry</Name>".to_owned() + &reference),
                        ),
                    )
                    .unwrap();
                }
            });
            edit(&source, CATALOG, |owner| rename(owner, 11, "Renamed"));
            let before = Inventory::read(&source).unwrap();
            let output = test.0.join("generations");
            for strict in [true, false] {
                let result = prepare_protected(&source, &base, &output, strict);
                if denied {
                    assert_eq!(
                        result.unwrap_err().error.code,
                        "source_structural_reference_preserved"
                    );
                    assert!(!output.exists());
                } else {
                    assert!(result.is_ok());
                }
            }
            assert_eq!(Inventory::read(&source).unwrap(), before);
        }
    }
}

/// Case-only spelling retains BSL identity and performs no new XML payload validation without invalidation.
#[test]
fn case_only_rename_keeps_existing_mentions_and_unverified_payload_bytes() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, &[], |input| {
            let directory = input.join("Documents/Entry/Ext");
            fs::create_dir_all(&directory).unwrap();
            fs::write(
                directory.join("ObjectModule.bsl"),
                "\u{feff}Объект.Existing = Истина;\r\n",
            )
            .unwrap();
            fs::write(directory.join("Extra.xml"), "<unverified").unwrap();
        });
        edit(&source, CATALOG, |owner| rename(owner, 11, "eXISTING"));
        let prepared =
            prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
        assert!(
            fs::read_to_string(prepared.xml().join("Catalogs/Records.xml"))
                .unwrap()
                .contains("<Name>eXISTING</Name>")
        );
        assert_eq!(
            fs::read(prepared.xml().join("Documents/Entry/Ext/Extra.xml")).unwrap(),
            b"<unverified"
        );
    }
}

/// A permitted module fix clears a mention; a frozen module retains its pinned dependency despite working edits.
#[test]
fn selected_accepted_and_working_module_bytes_control_rename_dependency_checks() {
    for readable in [false, true] {
        for frozen in [false, true] {
            let test = TestDir::new();
            let (source, base) = fixture(
                &test,
                readable,
                &if frozen { vec![uuid(3)] } else { vec![] },
                |input| {
                    let directory = input.join("Documents/Entry/Ext");
                    fs::create_dir_all(&directory).unwrap();
                    fs::write(
                        directory.join("ObjectModule.bsl"),
                        "\u{feff}Объект.Existing = Истина;\r\n",
                    )
                    .unwrap();
                },
            );
            edit(&source, CATALOG, |owner| rename(owner, 11, "Renamed"));
            let native = SourcePath::parse("Documents/Entry/Ext/ObjectModule.bsl").unwrap();
            fs::write(
                source.join(base.package.payload_sources[&native].as_path()),
                "\u{feff}// cleared\r\n",
            )
            .unwrap();
            let output = test.0.join("generations");
            let result = prepare_protected(&source, &base, &output, false);
            if frozen {
                assert_eq!(
                    result.unwrap_err().error.code,
                    "source_structural_reference_preserved"
                );
                assert!(!output.exists());
            } else {
                assert!(result.is_ok());
            }
        }
    }
}

/// Reorder/removal keeps UUID-addressed Name/Comment selection and exact current source locations.
#[test]
fn names_and_coordinates_follow_identity_after_reordering_or_removing_another_field() {
    for readable in [false, true] {
        for removal in [false, true] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[], |_| {});
            edit(&source, CATALOG, |owner| {
                if removal {
                    children(owner).remove(1);
                } else {
                    children(owner).swap(0, 1);
                }
                rename(owner, 11, "RenamedField");
                comment(
                    member(&mut owner.document.descriptor.object, 11).unwrap(),
                    "Renamed field comment",
                );
                rename(owner, 21, "RenamedColumn");
                assert_eq!(columns(table(owner, 13)).len(), 2);
            });
            let prepared =
                prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
            let native_path = SourcePath::parse("Catalogs/Records.xml").unwrap();
            let xml = fs::read_to_string(prepared.xml().join(native_path.as_path())).unwrap();
            let file = prepared.source_map().file(&native_path).unwrap();
            let path = SourcePath::parse(CATALOG).unwrap();
            let accepted = base.read_file(&path).unwrap();
            let working = fs::read_to_string(source.join(CATALOG)).unwrap();
            for (value, id) in [
                ("RenamedField", 11),
                ("Renamed field comment", 11),
                ("RenamedColumn", 21),
            ] {
                let offset = xml.find(value).unwrap();
                let location = file
                    .locate(
                        xml.as_bytes(),
                        offset..offset + value.len(),
                        Some(OriginTexts {
                            accepted: &accepted,
                            working: Some(&working),
                        }),
                    )
                    .unwrap();
                assert_eq!(location.authority, InputAuthority::Working);
                assert_eq!(location.uuid, Some(uuid(id)));
                assert!(working[location.range.unwrap()].contains(value));
            }
            for mode in [PropertiesMode::All, PropertiesMode::Summary] {
                let reader = SourceReader::open(&source).unwrap();
                let saved = reader.read_metadata(&path, mode).unwrap();
                assert_eq!(saved, reader.parse_metadata(&path, &working, mode).unwrap());
                assert_eq!(
                    saved.documents[0]
                        .descriptor
                        .objects
                        .iter()
                        .filter(|object| object.metadata.uuid == uuid(11).as_str())
                        .count(),
                    1
                );
            }
        }
    }
}
