//! Field removal preserves native composition, selected dependency bytes and independent policy decisions.

use std::{fs, path::Path};

use super::super::{
    Descriptor, InputAuthority, MetadataNode, ObjectSource, OriginTexts, PermissionReason,
    Property, PropertyName, PropertyValue, Section, SourceReader,
    child_creation::attribute_tests::{attribute_xml, edit},
    enum_test_support::{children, uuid},
    files::Inventory,
    prepare_protected,
};
use super::*;
use crate::{project_core::metadata_parser::PropertiesMode, test_support::TestDir};

const CATALOG: &str = "Catalogs/Records/object.yaml";
const DOCUMENT: &str = "Documents/Entry/object.yaml";
const BOOLEAN: &str = "<v8:Type>xs:boolean</v8:Type>";

/// Both roots have accepted attributes and direct tables with independent identities and inert service bytes.
fn fixture(
    test: &TestDir,
    readable: bool,
    frozen: &[NativeUuid],
    adjust: impl FnOnce(&Path),
) -> (std::path::PathBuf, AcceptedBase) {
    super::super::child_creation::attribute_tests::fixture(test, readable, frozen, |input| {
        for (path, table, first, second, first_name, second_name) in [
            (
                "Catalogs/Records.xml",
                13,
                21,
                22,
                "FirstColumn",
                "LastColumn",
            ),
            (
                "Documents/Entry.xml",
                14,
                41,
                42,
                "FirstDocumentColumn",
                "LastDocumentColumn",
            ),
        ] {
            let file = input.join(path);
            let native = fs::read_to_string(&file).unwrap();
            let table = format!(
                "<TabularSection uuid=\"{}\"><InternalInfo><FutureData>Retained table service bytes</FutureData></InternalInfo><Properties><Name>Rows</Name><Synonym/><Comment/></Properties><ChildObjects>{}{}</ChildObjects></TabularSection>",
                uuid(table).as_str(),
                attribute_xml(first, first_name, BOOLEAN),
                attribute_xml(second, second_name, BOOLEAN)
            );
            let fields = if first == 41 {
                attribute_xml(31, "FirstDocumentField", BOOLEAN)
                    + &attribute_xml(32, "LastDocumentField", BOOLEAN)
            } else {
                String::new()
            };
            fs::write(
                file,
                native.replace("</ChildObjects>", &(fields + &table + "</ChildObjects>")),
            )
            .unwrap();
        }
        adjust(input);
    })
}

/// Native parent identity selects one actual inline table and never a coincidental object kind.
fn table(owner: &mut ObjectSource, index: u32) -> &mut MetadataNode {
    children(owner)
        .iter_mut()
        .find_map(|child| match child {
            Child::Inline { object } if object.uuid == uuid(index) => Some(object.as_mut()),
            _ => None,
        })
        .unwrap()
}

/// An accepted inline collection remains explicit even after its last column is removed.
fn columns(table: &mut MetadataNode) -> &mut Vec<Child> {
    table
        .sections
        .iter_mut()
        .find_map(|section| match section {
            Section::Children(children) => Some(children),
            _ => None,
        })
        .unwrap()
}

/// Independent comments remain selectable when a neighboring structural operation is refused.
fn comment(object: &mut MetadataNode, text: &str) {
    for section in &mut object.sections {
        if let Section::Properties(properties) = section {
            for property in properties {
                if let Property::Typed {
                    name: PropertyName::Comment,
                    value,
                } = property
                {
                    *value = PropertyValue::Text(text.into());
                }
            }
        }
    }
}

/// Root fields and columns disappear by accepted UUID without changing native/source/base bytes or repeatability.
#[test]
fn fields_and_columns_reduce_both_envelopes_without_changing_accepted_inputs() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, &[], |_| {});
        let before =
            [test.0.join("native"), test.0.join("accepted")].map(|p| Inventory::read(&p).unwrap());
        let manifest = fs::read(source.join("source.json")).unwrap();
        edit(&source, CATALOG, |owner| {
            children(owner).remove(0);
            columns(table(owner, 13)).pop();
        });
        edit(&source, DOCUMENT, |owner| {
            children(owner).remove(0);
            columns(table(owner, 14)).pop();
        });
        let authored = Inventory::read(&source).unwrap();
        let output = test.0.join("generations");
        let prepared = prepare_protected(&source, &base, &output, true).unwrap();
        assert!(prepared.diagnostics().is_empty());
        for (path, remove) in [
            (
                "Catalogs/Records.xml",
                [
                    attribute_xml(11, "Existing", BOOLEAN),
                    attribute_xml(22, "LastColumn", BOOLEAN),
                ],
            ),
            (
                "Documents/Entry.xml",
                [
                    attribute_xml(31, "FirstDocumentField", BOOLEAN),
                    attribute_xml(42, "LastDocumentColumn", BOOLEAN),
                ],
            ),
        ] {
            let mut expected = fs::read_to_string(test.0.join("native").join(path)).unwrap();
            for removed in remove {
                expected = expected.replace(&removed, "");
            }
            let native = fs::read_to_string(prepared.xml().join(path)).unwrap();
            assert_eq!(
                Descriptor::from_xml(&native).unwrap(),
                Descriptor::from_xml(&expected).unwrap()
            );
            assert!(native.contains("Retained table service bytes"));
        }
        assert!(base.permission(&uuid(11), Operation::Delete).allowed);
        assert_eq!(
            base.permission(&uuid(13), Operation::Delete).reason,
            PermissionReason::UnsupportedOperation
        );
        assert_eq!(
            prepared.generation(),
            prepare_protected(&source, &base, &output, true)
                .unwrap()
                .generation()
        );
        assert_eq!(Inventory::read(&source).unwrap(), authored);
        assert_eq!(fs::read(source.join("source.json")).unwrap(), manifest);
        assert_eq!(
            [test.0.join("native"), test.0.join("accepted")].map(|p| Inventory::read(&p).unwrap()),
            before
        );
    }
}

/// Removing every column retains its table, generated service bytes and explicit empty collection.
#[test]
fn empty_table_and_stationary_frozen_fields_keep_actual_composition() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, &[uuid(11)], |_| {});
        edit(&source, CATALOG, |owner| {
            children(owner).remove(1);
            columns(table(owner, 13)).clear();
        });
        let prepared =
            prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
        let native = fs::read_to_string(prepared.xml().join("Catalogs/Records.xml")).unwrap();
        assert!(native.contains(uuid(11).as_str()));
        assert!(native.contains(uuid(13).as_str()));
        for removed in [12, 21, 22] {
            assert!(!native.contains(uuid(removed).as_str()));
        }
        let mut model =
            ObjectSource::from_yaml(&fs::read_to_string(source.join(CATALOG)).unwrap()).unwrap();
        assert!(columns(table(&mut model, 13)).is_empty());
    }
}

/// Parent, deleted member and displaced sibling policies each restore one collection while another owner remains editable.
#[test]
fn frozen_fields_tables_and_displaced_siblings_restore_without_masking_another_owner() {
    for readable in [false, true] {
        for frozen in [2, 11, 12, 13, 21, 22] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[uuid(frozen)], |_| {});
            edit(&source, CATALOG, |owner| {
                if frozen >= 13 {
                    columns(table(owner, 13)).remove(0);
                } else {
                    children(owner).remove(0);
                }
            });
            edit(&source, DOCUMENT, |owner| {
                children(owner).remove(0);
                comment(
                    &mut owner.document.descriptor.object,
                    "Independent document comment",
                );
            });
            let output = test.0.join("generations");
            let failed = prepare_protected(&source, &base, &output, true).unwrap_err();
            let diagnostics: Vec<_> = failed
                .diagnostics
                .iter()
                .filter(|d| d.operation == Operation::Delete)
                .collect();
            assert_eq!(diagnostics.len(), 1);
            assert_eq!(diagnostics[0].uuid, Some(uuid(frozen)));
            assert_eq!(diagnostics[0].reason, PermissionReason::EskaFrozen);
            assert!(!output.exists());
            let selected = prepare_protected(&source, &base, &output, false).unwrap();
            let catalog = fs::read_to_string(selected.xml().join("Catalogs/Records.xml")).unwrap();
            assert!(catalog.contains(uuid(if frozen >= 13 { 21 } else { 11 }).as_str()));
            let document = fs::read_to_string(selected.xml().join("Documents/Entry.xml")).unwrap();
            assert!(!document.contains(uuid(31).as_str()));
            assert!(document.contains("Independent document comment"));
        }
    }
}

/// Known native default properties can disappear with the whole field; unknown metadata and payload-bearing nodes cannot.
#[test]
fn known_native_attribute_domains_and_unverified_profiles_are_distinguished() {
    for readable in [false, true] {
        for shape in [
            "native_defaults",
            "unknown_property",
            "internal_info",
            "namespace",
            "children",
        ] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[], |input| {
                let path = input.join("Catalogs/Records.xml");
                let native = fs::read_to_string(&path).unwrap();
                let original = attribute_xml(11, "Existing", BOOLEAN);
                let changed=match shape {
                    "native_defaults"=>original.replace("</Properties>","<PasswordMode>false</PasswordMode><Format/><ToolTip><v8:item><v8:lang>ru</v8:lang><v8:content>Подсказка</v8:content></v8:item></ToolTip><MinValue/><FillChecking>DontCheck</FillChecking><ChoiceParameters/><DataHistory>Use</DataHistory></Properties>"),
                    "unknown_property"=>original.replace("</Properties>","<FutureProperty/></Properties>"),
                    "internal_info"=>original.replace("<Properties>","<InternalInfo><Future/></InternalInfo><Properties>"),
                    "namespace"=>original.replace("<Attribute uuid=","<Attribute xmlns:local=\"urn:future\" uuid="),
                    _=>original.replace("</Attribute>","<ChildObjects/></Attribute>"),
                };
                fs::write(path, native.replace(&original, &changed)).unwrap();
            });
            edit(&source, CATALOG, |owner| {
                children(owner).remove(0);
            });
            let output = test.0.join("generations");
            if shape == "native_defaults" {
                let prepared = prepare_protected(&source, &base, &output, true).unwrap();
                assert!(
                    !fs::read_to_string(prepared.xml().join("Catalogs/Records.xml"))
                        .unwrap()
                        .contains(uuid(11).as_str())
                );
            } else {
                let failure = prepare_protected(&source, &base, &output, true).unwrap_err();
                assert_eq!(failure.error.code, "source_changes_excluded");
                assert!(!output.exists());
                let prepared = prepare_protected(&source, &base, &output, false).unwrap();
                assert!(
                    fs::read_to_string(prepared.xml().join("Catalogs/Records.xml"))
                        .unwrap()
                        .contains(uuid(11).as_str())
                );
            }
        }
    }
}

/// Direct/local path, UUID, entities, CDATA and XML element names cannot hide field dependencies.
#[test]
fn decoded_xml_field_mentions_refuse_before_publication_in_both_envelopes() {
    for readable in [false, true] {
        for reference in [
            "<Future>Объект.EXISTING</Future>",
            "<Future ref=\"Catalog.Records.Attribute.Existing\"/>",
            "<Future><![CDATA[Existing]]></Future>",
            "<Future>Ex&#x69;sting</Future>",
            "<Existing/>",
        ]
        .map(String::from)
        .into_iter()
        .chain([format!("<Future>{}</Future>", uuid(11).as_str())])
        {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[], |input| {
                let path = input.join("Documents/Entry.xml");
                let native = fs::read_to_string(&path).unwrap();
                fs::write(
                    path,
                    native.replace(
                        "<Name>Entry</Name>",
                        &("<Name>Entry</Name>".to_owned() + &reference),
                    ),
                )
                .unwrap();
            });
            edit(&source, CATALOG, |owner| {
                children(owner).remove(0);
            });
            let before = Inventory::read(&source).unwrap();
            let output = test.0.join("generations");
            for strict in [true, false] {
                let failure = prepare_protected(&source, &base, &output, strict).unwrap_err();
                assert_eq!(failure.error.code, "source_structural_reference_preserved");
                assert!(
                    failure
                        .error
                        .location
                        .contains(&uuid(11).as_str().to_owned())
                );
                assert!(!output.exists());
            }
            assert_eq!(Inventory::read(&source).unwrap(), before);
        }
    }
}

/// BSL token mentions remain conservative even in comments/strings; substrings in other identifiers do not invalidate fields.
#[test]
fn selected_bsl_mentions_and_identifier_boundaries_guard_field_deletion() {
    for readable in [false, true] {
        for (body, denied) in [
            ("ЭтотОбъект.Existing = Истина;", true),
            ("Имя = \"EXISTING\";", true),
            ("// Existing", true),
            ("Object.ExistingOther = 1; Object._Existing = 2;", false),
        ] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[], |input| {
                let directory = input.join("Documents/Entry/Ext");
                fs::create_dir_all(&directory).unwrap();
                fs::write(
                    directory.join("ObjectModule.bsl"),
                    format!("\u{feff}{body}\r\n"),
                )
                .unwrap();
            });
            edit(&source, CATALOG, |owner| {
                children(owner).remove(0);
            });
            let output = test.0.join("generations");
            let result = prepare_protected(&source, &base, &output, true);
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
    }
}

/// Denied working module edits cannot erase a dependency retained in the selected pinned bytes.
#[test]
fn selected_accepted_bsl_dependencies_survive_forbidden_working_removal() {
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
                        "\u{feff}Object.Existing = Истина;\r\n",
                    )
                    .unwrap();
                },
            );
            edit(&source, CATALOG, |owner| {
                children(owner).remove(0);
            });
            fs::write(
                source.join(
                    base.package
                        .payload_sources
                        .get(&SourcePath::parse("Documents/Entry/Ext/ObjectModule.bsl").unwrap())
                        .unwrap()
                        .as_path(),
                ),
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

/// A retained member's coordinates and saved/overlay identities follow UUID after a preceding field disappears.
#[test]
fn surviving_field_locations_and_saved_overlay_readers_use_current_composition() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, &[], |_| {});
        edit(&source, CATALOG, |owner| {
            children(owner).remove(0);
            let Child::Inline { object } = &mut children(owner)[0] else {
                unreachable!()
            };
            comment(object, "Retained field comment");
        });
        let prepared =
            prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
        let native_path = SourcePath::parse("Catalogs/Records.xml").unwrap();
        let native = fs::read_to_string(prepared.xml().join(native_path.as_path())).unwrap();
        let file = prepared.source_map().file(&native_path).unwrap();
        let accepted = base
            .read_file(&SourcePath::parse(CATALOG).unwrap())
            .unwrap();
        let working = fs::read_to_string(source.join(CATALOG)).unwrap();
        let start = native.find("Retained field comment").unwrap();
        let location = file
            .locate(
                native.as_bytes(),
                start..start + "Retained field comment".len(),
                Some(OriginTexts {
                    accepted: &accepted,
                    working: Some(&working),
                }),
            )
            .unwrap();
        assert_eq!(location.uuid, Some(uuid(12)));
        assert_eq!(location.authority, InputAuthority::Working);
        assert!(working[location.range.unwrap()].contains("Retained field comment"));
        for mode in [PropertiesMode::All, PropertiesMode::Summary] {
            let reader = SourceReader::open(&source).unwrap();
            let path = SourcePath::parse(CATALOG).unwrap();
            let saved = reader.read_metadata(&path, mode).unwrap();
            let overlay = reader.parse_metadata(&path, &working, mode).unwrap();
            assert_eq!(saved, overlay);
            assert!(
                !saved.documents[0]
                    .descriptor
                    .objects
                    .iter()
                    .any(|object| object.metadata.uuid == uuid(11).as_str())
            );
        }
    }
}

/// Selected XML payloads retain pinned references when a working change is refused; malformed XML is never guessed.
#[test]
fn selected_xml_resources_and_unverified_bytes_guard_local_field_paths() {
    for readable in [false, true] {
        for mode in ["retained", "fixed", "frozen_fix", "malformed"] {
            let test = TestDir::new();
            let frozen = if mode == "frozen_fix" {
                vec![uuid(3)]
            } else {
                Vec::new()
            };
            let (source, base) = fixture(&test, readable, &frozen, |input| {
                let directory = input.join("Documents/Entry/Ext");
                fs::create_dir_all(&directory).unwrap();
                fs::write(
                    directory.join("Extra.xml"),
                    if mode == "malformed" {
                        "<broken"
                    } else {
                        "<Root><DataPath><![CDATA[Объект.Existing]]></DataPath></Root>"
                    },
                )
                .unwrap();
            });
            edit(&source, CATALOG, |owner| {
                children(owner).remove(0);
            });
            if matches!(mode, "fixed" | "frozen_fix") {
                let path = base
                    .package
                    .payload_sources
                    .get(&SourcePath::parse("Documents/Entry/Ext/Extra.xml").unwrap())
                    .unwrap();
                fs::write(source.join(path.as_path()), "<Root><DataPath/></Root>").unwrap();
            }
            let output = test.0.join("generations");
            let result = prepare_protected(&source, &base, &output, false);
            if mode == "fixed" {
                assert!(result.is_ok());
            } else {
                assert_eq!(
                    result.unwrap_err().error.code,
                    if mode == "malformed" {
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

/// Foreign root classes, unknown child slots and mixed create/delete never expand the accepted removal profile.
#[test]
fn unsupported_scope_unknown_composition_and_mixed_edits_restore_without_deleting_fields() {
    for readable in [false, true] {
        for mode in [
            "foreign_parent",
            "unknown_child",
            "mixed_creation",
            "mixed_order",
        ] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[], |input| {
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
                } else if mode == "unknown_child" {
                    let file = input.join("Catalogs/Records.xml");
                    fs::write(
                        &file,
                        fs::read_to_string(&file).unwrap().replacen(
                            "<ChildObjects>",
                            "<ChildObjects><!-- future child -->",
                            1,
                        ),
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
                children(owner).retain(
                    |child| !matches!(child, Child::Inline { object } if object.uuid == uuid(11)),
                );
                if mode == "mixed_creation" {
                    children(owner).insert(
                        0,
                        super::super::child_creation::attribute_tests::attribute(
                            100, "NewField", BOOLEAN,
                        ),
                    );
                }
                if mode == "mixed_order" {
                    children(owner).reverse();
                }
                comment(
                    &mut owner.document.descriptor.object,
                    "Independent owner comment",
                );
            });
            let output = test.0.join("generations");
            assert!(prepare_protected(&source, &base, &output, true).is_err());
            assert!(!output.exists());
            let restored = prepare_protected(&source, &base, &output, false).unwrap();
            let native_path = if mode == "foreign_parent" {
                "DataProcessors/Records.xml"
            } else {
                "Catalogs/Records.xml"
            };
            let native = fs::read_to_string(restored.xml().join(native_path)).unwrap();
            assert!(native.contains(uuid(11).as_str()));
            assert!(native.contains("Independent owner comment"));
            assert!(!native.contains(uuid(100).as_str()));
        }
    }
}
