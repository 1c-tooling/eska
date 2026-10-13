//! Complete table removal retains subtree permissions, service identity guards and source ownership.

use std::{fs, path::Path};

use super::super::{
    Descriptor, InputAuthority, OriginTexts, PermissionReason, SourceReader,
    attribute_test_support::{comment, fixture as field_fixture, table},
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

/// Native table/row declarations derive their names from one parent/table Name and have four unique UUID slots.
fn table_xml(kind: &str, parent: &str, index: u32, name: &str, columns: &str) -> String {
    let declarations = [("TabularSection", 1000), ("TabularSectionRow", 1002)]
        .map(|(role, offset)| {
            let type_id = uuid(index * 10 + offset);
            let value_id = uuid(index * 10 + offset + 1);
            format!(
                "<xr:GeneratedType name=\"{kind}{role}.{parent}.{name}\" category=\"{role}\"><xr:TypeId>{}</xr:TypeId><xr:ValueId>{}</xr:ValueId></xr:GeneratedType>",
                type_id.as_str(), value_id.as_str()
            )
        })
        .concat();
    format!(
        "<TabularSection uuid=\"{}\"><InternalInfo>{declarations}</InternalInfo><Properties><Name>{name}</Name><Synonym/><Comment/><ToolTip/><FillChecking>DontCheck</FillChecking>{}<LineNumberLength>5</LineNumberLength></Properties><ChildObjects>{columns}</ChildObjects></TabularSection>",
        uuid(index).as_str(),
        if kind == "Catalog" {
            "<Use>ForItem</Use>"
        } else {
            ""
        }
    )
}

/// Known service declarations replace only this test's inert table data before independently pinning it.
fn fixture(
    test: &TestDir,
    readable: bool,
    frozen: &[NativeUuid],
    adjust: impl FnOnce(&Path),
) -> (std::path::PathBuf, AcceptedBase) {
    field_fixture(test, readable, frozen, |input| {
        for (path, kind, parent, index, first, last) in [
            ("Catalogs/Records.xml", "Catalog", "Records", 13, 21, 22),
            ("Documents/Entry.xml", "Document", "Entry", 14, 41, 42),
        ] {
            let file = input.join(path);
            let native = fs::read_to_string(&file).unwrap();
            let start = native
                .find(&format!(
                    "<TabularSection uuid=\"{}\"",
                    uuid(index).as_str()
                ))
                .unwrap();
            let end = start
                + native[start..].find("</TabularSection>").unwrap()
                + "</TabularSection>".len();
            let columns = attribute_xml(
                first,
                if first == 21 {
                    "FirstColumn"
                } else {
                    "FirstDocumentColumn"
                },
                BOOLEAN,
            ) + &attribute_xml(
                last,
                if last == 22 {
                    "LastColumn"
                } else {
                    "LastDocumentColumn"
                },
                BOOLEAN,
            );
            let mut changed = native.clone();
            changed.replace_range(
                start..end,
                &table_xml(kind, parent, index, "Rows", &columns),
            );
            if index == 13 {
                let start = changed
                    .find(&format!("<TabularSection uuid=\"{}\"", uuid(13).as_str()))
                    .unwrap();
                let end = start
                    + changed[start..].find("</TabularSection>").unwrap()
                    + "</TabularSection>".len();
                changed.insert_str(
                    end,
                    &table_xml(
                        kind,
                        parent,
                        15,
                        "RemainingRows",
                        &attribute_xml(23, "ExtraColumn", BOOLEAN),
                    ),
                );
            }
            fs::write(file, changed).unwrap();
        }
        adjust(input);
    })
}

/// Membership removes one table and all service/column identities while retaining inputs and a repeatable generation.
#[test]
fn complete_table_subtrees_delete_in_both_envelopes_and_preserve_inputs() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, &[], |_| {});
        let inputs =
            [test.0.join("native"), test.0.join("accepted")].map(|p| Inventory::read(&p).unwrap());
        let manifest = fs::read(source.join("source.json")).unwrap();
        for (path, index) in [(CATALOG, 13), (DOCUMENT, 14)] {
            edit(&source, path, |owner| {
                children(owner).retain(
                    |child| !matches!(child,Child::Inline { object } if object.uuid==uuid(index)),
                );
            });
        }
        let authored = Inventory::read(&source).unwrap();
        let output = test.0.join("generations");
        let prepared = prepare_protected(&source, &base, &output, true).unwrap();
        assert!(prepared.diagnostics().is_empty());
        for (path, index, cols) in [
            ("Catalogs/Records.xml", 13, [21, 22]),
            ("Documents/Entry.xml", 14, [41, 42]),
        ] {
            let native = fs::read_to_string(test.0.join("native").join(path)).unwrap();
            let start = native
                .find(&format!(
                    "<TabularSection uuid=\"{}\"",
                    uuid(index).as_str()
                ))
                .unwrap();
            let end = start
                + native[start..].find("</TabularSection>").unwrap()
                + "</TabularSection>".len();
            let mut expected = native.clone();
            expected.replace_range(start..end, "");
            let actual = fs::read_to_string(prepared.xml().join(path)).unwrap();
            assert_eq!(
                Descriptor::from_xml(&actual).unwrap(),
                Descriptor::from_xml(&expected).unwrap()
            );
            for deleted in [
                index,
                cols[0],
                cols[1],
                index * 10 + 1000,
                index * 10 + 1001,
                index * 10 + 1002,
                index * 10 + 1003,
            ] {
                assert!(!actual.contains(uuid(deleted).as_str()));
            }
        }
        assert!(base.permission(&uuid(13), Operation::Delete).allowed);
        assert_eq!(
            base.permission(&uuid(1130), Operation::Delete).reason,
            PermissionReason::UnknownObject
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
            inputs
        );
    }
}

/// Every removed column keeps its own native/freeze authority, and shifted tables retain Reorder permission.
#[test]
fn frozen_parent_table_column_and_displaced_table_restore_independent_changes() {
    for readable in [false, true] {
        for frozen in [2, 13, 21, 22, 15, 23] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[uuid(frozen)], |_| {});
            edit(&source, CATALOG, |owner| {
                children(owner).retain(
                    |child| !matches!(child,Child::Inline { object } if object.uuid==uuid(13)),
                );
                comment(
                    &mut owner.document.descriptor.object,
                    "Independent catalog comment",
                );
            });
            edit(&source, DOCUMENT, |owner| {
                children(owner).retain(
                    |child| !matches!(child,Child::Inline { object } if object.uuid==uuid(14)),
                );
            });
            let output = test.0.join("generations");
            let denied = frozen != 23;
            if denied {
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
            }
            let selected = prepare_protected(&source, &base, &output, false).unwrap();
            let catalog = fs::read_to_string(selected.xml().join("Catalogs/Records.xml")).unwrap();
            assert_eq!(catalog.contains(uuid(13).as_str()), denied);
            assert!(catalog.contains(uuid(15).as_str()));
            assert_eq!(catalog.contains("Independent catalog comment"), frozen != 2);
            assert!(
                !fs::read_to_string(selected.xml().join("Documents/Entry.xml"))
                    .unwrap()
                    .contains(uuid(14).as_str())
            );
        }
    }
}

/// Unknown sections/columns/owned slots and undeclared service identities cannot be removed with the table.
#[test]
fn unverified_table_profiles_restore_without_dropping_independent_properties() {
    for readable in [false, true] {
        for shape in [
            "unknown_property",
            "unknown_section",
            "unknown_column",
            "namespace",
            "opaque_ids",
            "reference",
        ] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[], |input| {
                let file = input.join("Catalogs/Records.xml");
                let native = fs::read_to_string(&file).unwrap();
                let start = native
                    .find(&format!("<TabularSection uuid=\"{}\"", uuid(13).as_str()))
                    .unwrap();
                let end = start
                    + native[start..].find("</TabularSection>").unwrap()
                    + "</TabularSection>".len();
                let original = &native[start..end];
                let replacement = match shape {
                    "unknown_property" => {
                        original.replace("<ToolTip/>", "<FutureProperty/><ToolTip/>")
                    }
                    "unknown_section" => {
                        original.replace("<Properties>", "<FutureSection/><Properties>")
                    }
                    "unknown_column" => original.replace("<Type>", "<Future/><Type>"),
                    "namespace" => original.replace(
                        "<TabularSection uuid=",
                        "<TabularSection xmlns:local=\"urn:future\" uuid=",
                    ),
                    "opaque_ids" => {
                        original.replace("category=\"TabularSection\"", "category=\"Future\"")
                    }
                    _ => original.replace(
                        "<ChildObjects>",
                        "<ChildObjects><future:Owned xmlns:future=\"urn:future\"/>",
                    ),
                };
                fs::write(file, native.replace(original, &replacement)).unwrap();
            });
            edit(&source, CATALOG, |owner| {
                children(owner).retain(
                    |child| !matches!(child,Child::Inline { object } if object.uuid==uuid(13)),
                );
                comment(
                    &mut owner.document.descriptor.object,
                    "Independent owner comment",
                );
            });
            let output = test.0.join("generations");
            let failed = prepare_protected(&source, &base, &output, true).unwrap_err();
            assert_eq!(failed.error.code, "source_changes_excluded");
            assert!(!output.exists());
            let prepared = prepare_protected(&source, &base, &output, false).unwrap();
            let xml = fs::read_to_string(prepared.xml().join("Catalogs/Records.xml")).unwrap();
            assert!(xml.contains(uuid(13).as_str()));
            assert!(xml.contains("Independent owner comment"));
        }
    }
}

/// Deleted table/column names and all native declaration slots invalidate selected XML and BSL dependencies.
#[test]
fn table_column_and_service_references_refuse_in_both_modes() {
    for readable in [false, true] {
        for (kind, value) in [
            ("xml", "Object.Rows".to_owned()),
            ("xml", "FirstColumn".into()),
            ("xml", uuid(13).as_str().into()),
            ("xml", uuid(1130).as_str().into()),
            ("xml", uuid(1131).as_str().into()),
            ("xml", uuid(1132).as_str().into()),
            ("xml", uuid(1133).as_str().into()),
            ("bsl", "Object.ROWS.Clear();".into()),
            ("bsl", "// LastColumn".into()),
        ] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[], |input| {
                let dir = input.join("Documents/Entry/Ext");
                fs::create_dir_all(&dir).unwrap();
                fs::write(
                    dir.join(if kind == "xml" {
                        "Extra.xml"
                    } else {
                        "ObjectModule.bsl"
                    }),
                    if kind == "xml" {
                        format!("<Root ref=\"{value}\"/>")
                    } else {
                        format!("\u{feff}{value}\r\n")
                    },
                )
                .unwrap();
            });
            edit(&source, CATALOG, |owner| {
                children(owner).retain(
                    |child| !matches!(child,Child::Inline { object } if object.uuid==uuid(13)),
                );
            });
            let output = test.0.join("generations");
            for strict in [true, false] {
                let failed = prepare_protected(&source, &base, &output, strict).unwrap_err();
                assert_eq!(failed.error.code, "source_structural_reference_preserved");
                assert!(!output.exists());
            }
        }
    }
}

/// A permitted fix removes selected dependencies, while a frozen resource retains accepted mentions.
#[test]
fn selected_table_dependencies_follow_payload_policy_and_require_well_formed_xml() {
    for readable in [false, true] {
        for mode in ["fixed", "frozen_fix", "malformed"] {
            let test = TestDir::new();
            let (source, base) = fixture(
                &test,
                readable,
                &if mode == "frozen_fix" {
                    vec![uuid(3)]
                } else {
                    vec![]
                },
                |input| {
                    let dir = input.join("Documents/Entry/Ext");
                    fs::create_dir_all(&dir).unwrap();
                    fs::write(
                        dir.join("Extra.xml"),
                        if mode == "malformed" {
                            "<broken"
                        } else {
                            "<Root><DataPath>Object.Rows</DataPath></Root>"
                        },
                    )
                    .unwrap();
                },
            );
            edit(&source, CATALOG, |owner| {
                children(owner).retain(
                    |child| !matches!(child,Child::Inline { object } if object.uuid==uuid(13)),
                );
            });
            if mode != "malformed" {
                let path = base
                    .package
                    .payload_sources
                    .get(&SourcePath::parse("Documents/Entry/Ext/Extra.xml").unwrap())
                    .unwrap();
                fs::write(source.join(path.as_path()), "<Root/>").unwrap();
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

/// Removing a preceding table preserves UUID-based coordinates and saved/overlay metadata for the surviving table.
#[test]
fn remaining_table_coordinates_and_readers_follow_uuid_after_table_deletion() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, &[], |_| {});
        edit(&source, CATALOG, |owner| {
            children(owner)
                .retain(|child| !matches!(child,Child::Inline { object } if object.uuid==uuid(13)));
            comment(table(owner, 15), "Remaining table comment");
        });
        let output = test.0.join("generations");
        let prepared = prepare_protected(&source, &base, &output, true).unwrap();
        let native_path = SourcePath::parse("Catalogs/Records.xml").unwrap();
        let xml = fs::read_to_string(prepared.xml().join(native_path.as_path())).unwrap();
        let accepted = base
            .read_file(&SourcePath::parse(CATALOG).unwrap())
            .unwrap();
        let working = fs::read_to_string(source.join(CATALOG)).unwrap();
        let offset = xml.find("Remaining table comment").unwrap();
        let location = prepared
            .source_map()
            .file(&native_path)
            .unwrap()
            .locate(
                xml.as_bytes(),
                offset..offset + "Remaining table comment".len(),
                Some(OriginTexts {
                    accepted: &accepted,
                    working: Some(&working),
                }),
            )
            .unwrap();
        assert_eq!(location.uuid, Some(uuid(15)));
        assert_eq!(location.authority, InputAuthority::Working);
        assert!(working[location.range.unwrap()].contains("Remaining table comment"));
        let reader = SourceReader::open(&source).unwrap();
        let path = SourcePath::parse(CATALOG).unwrap();
        for mode in [PropertiesMode::All, PropertiesMode::Summary] {
            let saved = reader.read_metadata(&path, mode).unwrap();
            assert_eq!(saved, reader.parse_metadata(&path, &working, mode).unwrap());
            for deleted in [13, 21, 22] {
                assert!(
                    !saved.documents[0]
                        .descriptor
                        .objects
                        .iter()
                        .any(|o| o.metadata.uuid == uuid(deleted).as_str())
                );
            }
        }
        for deleted in [13, 21, 22, 1130, 1131, 1132, 1133] {
            assert!(!xml.contains(uuid(deleted).as_str()));
        }
    }
}

/// The observed `LineNumber` defaults are removable; future standard fields, nested content and local bindings are not.
#[test]
fn native_line_number_standard_profile_is_closed_and_preserves_unknown_variants() {
    let standard = "<StandardAttributes><xr:StandardAttribute name=\"LineNumber\"><xr:LinkByType/><xr:FillChecking>DontCheck</xr:FillChecking><xr:MultiLine>false</xr:MultiLine><xr:FillFromFillingValue>false</xr:FillFromFillingValue><xr:CreateOnInput>Auto</xr:CreateOnInput><xr:TypeReductionMode>TransformValues</xr:TypeReductionMode><xr:MaxValue xsi:nil=\"true\"/><xr:ToolTip/><xr:ExtendedEdit>false</xr:ExtendedEdit><xr:Format/><xr:ChoiceForm/><xr:QuickChoice>Auto</xr:QuickChoice><xr:ChoiceHistoryOnInput>Auto</xr:ChoiceHistoryOnInput><xr:EditFormat/><xr:PasswordMode>false</xr:PasswordMode><xr:DataHistory>Use</xr:DataHistory><xr:MarkNegatives>false</xr:MarkNegatives><xr:MinValue xsi:nil=\"true\"/><xr:Synonym><v8:item><v8:lang>ru</v8:lang><v8:content>Номер</v8:content></v8:item></xr:Synonym><xr:Comment/><xr:FullTextSearch>Use</xr:FullTextSearch><xr:ChoiceParameterLinks/><xr:FillValue xsi:nil=\"true\"/><xr:Mask/><xr:ChoiceParameters/></xr:StandardAttribute></StandardAttributes>";
    for readable in [false, true] {
        for shape in [
            "native",
            "unknown_field",
            "unknown_property",
            "nested",
            "namespace",
        ] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[], |input| {
                let file = input.join("Catalogs/Records.xml");
                let native = fs::read_to_string(&file).unwrap();
                let standard = match shape {
                    "unknown_field" => {
                        standard.replace("name=\"LineNumber\"", "name=\"FutureField\"")
                    }
                    "unknown_property" => standard.replace("<xr:Mask/>", "<xr:Future/>"),
                    "nested" => standard.replace(
                        "<xr:ChoiceParameters/>",
                        "<xr:ChoiceParameters><xr:Future/></xr:ChoiceParameters>",
                    ),
                    "namespace" => standard.replace(
                        "<xr:StandardAttribute name=",
                        "<xr:StandardAttribute xmlns:local=\"urn:future\" name=",
                    ),
                    _ => standard.into(),
                };
                let start = native
                    .find(&format!("<TabularSection uuid=\"{}\"", uuid(13).as_str()))
                    .unwrap();
                let slot = start + native[start..].find("<Use>").unwrap();
                let mut changed = native;
                changed.insert_str(slot, &standard);
                changed = changed.replace(
                    "version=\"2.20\"",
                    "xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" version=\"2.20\"",
                );
                fs::write(file, changed).unwrap();
            });
            edit(&source, CATALOG, |owner| {
                children(owner).retain(
                    |child| !matches!(child,Child::Inline { object } if object.uuid==uuid(13)),
                );
            });
            let output = test.0.join("generations");
            if shape != "native" {
                assert!(prepare_protected(&source, &base, &output, true).is_err());
                assert!(!output.exists());
            }
            let selected = prepare_protected(&source, &base, &output, false).unwrap();
            let xml = fs::read_to_string(selected.xml().join("Catalogs/Records.xml")).unwrap();
            assert_eq!(xml.contains(uuid(13).as_str()), shape != "native");
        }
    }
}

/// Empty tables and stationary frozen siblings do not create extra authorities for the removed subtree.
#[test]
fn empty_table_and_stationary_frozen_table_allow_independent_removal() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, &[uuid(13)], |input| {
            let file = input.join("Catalogs/Records.xml");
            let native = fs::read_to_string(&file).unwrap();
            fs::write(
                file,
                native.replace(&attribute_xml(23, "ExtraColumn", BOOLEAN), ""),
            )
            .unwrap();
        });
        edit(&source, CATALOG, |owner| {
            children(owner)
                .retain(|child| !matches!(child,Child::Inline { object } if object.uuid==uuid(15)));
        });
        let selected =
            prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
        let xml = fs::read_to_string(selected.xml().join("Catalogs/Records.xml")).unwrap();
        assert!(xml.contains(uuid(13).as_str()));
        assert!(!xml.contains(uuid(15).as_str()));
    }
}

/// Typed table/row targets are checked against the final composition independently of the preserved text guard.
#[test]
fn typed_row_reference_requires_an_allowed_type_fix_with_table_deletion() {
    for readable in [false, true] {
        for fixed in [false, true] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[], |input| {
                let file = input.join("Catalogs/Records.xml");
                let native = fs::read_to_string(&file).unwrap();
                let original = attribute_xml(11, "Existing", BOOLEAN);
                let changed = attribute_xml(
                    11,
                    "Existing",
                    "<v8:Type>cfg:CatalogTabularSectionRow.Records.Rows</v8:Type>",
                );
                fs::write(file, native.replace(&original, &changed)).unwrap();
            });
            assert!(prepare_protected(&source, &base, &test.0.join("unchanged"), true).is_ok());
            edit(&source, CATALOG, |owner| {
                children(owner).retain(
                    |child| !matches!(child,Child::Inline { object } if object.uuid==uuid(13)),
                );
                if fixed {
                    let Child::Inline { object } = &mut children(owner)[0] else {
                        unreachable!()
                    };
                    let Section::Properties(properties) = &mut object.sections[0] else {
                        unreachable!()
                    };
                    let super::super::Property::Typed {
                        value: super::super::PropertyValue::TypeDescription(value),
                        ..
                    } = &mut properties[3]
                    else {
                        unreachable!()
                    };
                    value.data.alternatives[0].name = "xs:boolean".into();
                }
            });
            let output = test.0.join("generations");
            let result = prepare_protected(&source, &base, &output, true);
            if fixed {
                assert!(result.is_ok());
            } else {
                let error = result.unwrap_err().error;
                assert_eq!(error.code, "source_type_target_missing");
                assert!(!output.exists());
            }
        }
    }
}

/// Transfer, mixed creation/removal and reordered survivors are never inferred from a reduced subtree.
#[test]
fn mixed_table_membership_restores_composition_and_independent_comments() {
    for readable in [false, true] {
        for mode in ["transfer", "mixed_creation", "reorder"] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[], |_| {});
            let mut transferred = None;
            edit(&source, CATALOG, |owner| {
                let pos = children(owner)
                    .iter()
                    .position(
                        |child| matches!(child,Child::Inline { object } if object.uuid==uuid(13)),
                    )
                    .unwrap();
                transferred = Some(children(owner).remove(pos));
                if mode == "mixed_creation" {
                    children(owner).insert(
                        0,
                        super::super::child_creation::attribute_tests::attribute(
                            100, "Fresh", BOOLEAN,
                        ),
                    );
                }
                if mode == "reorder" {
                    children(owner).reverse();
                }
                comment(
                    &mut owner.document.descriptor.object,
                    "Independent root comment",
                );
            });
            if mode == "transfer" {
                edit(&source, DOCUMENT, |owner| {
                    children(owner).push(transferred.unwrap());
                });
            }
            let output = test.0.join("generations");
            let result = prepare_protected(&source, &base, &output, true);
            assert!(result.is_err());
            assert!(!output.exists());
            let selected = prepare_protected(&source, &base, &output, false).unwrap();
            let xml = fs::read_to_string(selected.xml().join("Catalogs/Records.xml")).unwrap();
            if mode == "transfer" {
                assert!(!xml.contains(uuid(13).as_str()));
            } else {
                assert!(xml.contains(uuid(13).as_str()));
            }
            assert!(xml.contains("Independent root comment"));
            let doc = fs::read_to_string(selected.xml().join("Documents/Entry.xml")).unwrap();
            assert!(!doc.contains(uuid(13).as_str()));
        }
    }
}
