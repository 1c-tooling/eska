//! New tables are one closed subtree under an accepted owner, with fresh metadata and service UUIDs.

use super::super::enum_test_support::{children, uuid};
use super::attribute_tests::{attribute, edit, fixture};
use super::*;
use crate::{
    project_core::{
        metadata_parser::PropertiesMode,
        source_format::{
            GeneratedIdentity, InputAuthority, ObjectSource, OriginTexts, PermissionReason,
            SourceReader, TabularSectionIdentities, XmlFragment, files::Inventory,
            prepare_protected,
        },
    },
    test_support::TestDir,
};
use std::fs;

const CATALOG: &str = "Catalogs/Records/object.yaml";
const DOCUMENT: &str = "Documents/Entry/object.yaml";
const BOOLEAN: &str = "<v8:Type>xs:boolean</v8:Type>";

/// Every authored name has one typed value; native declaration names follow the real parent.
fn table(index: u32, name: &str, columns: Vec<Child>) -> Child {
    Child::Inline {
        object: Box::new(MetadataNode {
            kind: MetadataKind::TabularSection,
            uuid: uuid(index),
            namespaces: Vec::new(),
            attributes: Vec::new(),
            sections: vec![
                Section::TabularSectionIdentities(TabularSectionIdentities {
                    prefix: "xr".into(),
                    table: GeneratedIdentity {
                        type_id: uuid(index + 1000),
                        value_id: uuid(index + 1001),
                    },
                    row: GeneratedIdentity {
                        type_id: uuid(index + 1002),
                        value_id: uuid(index + 1003),
                    },
                }),
                Section::Properties(vec![
                    Property::Typed {
                        name: PropertyName::Name,
                        value: PropertyValue::Text(name.into()),
                    },
                    Property::Typed {
                        name: PropertyName::Synonym,
                        value: PropertyValue::Empty,
                    },
                    Property::Typed {
                        name: PropertyName::Comment,
                        value: PropertyValue::Empty,
                    },
                ]),
                Section::Children(columns),
            ],
        }),
    }
}

/// Mutations in negative tests touch the new subtree, never an unrelated accepted object.
fn node(child: &mut Child) -> &mut MetadataNode {
    let Child::Inline { object } = child else {
        unreachable!()
    };
    object
}

/// Two proven owners create empty/populated tables, preserve input authority and expose current reader semantics.
#[test]
fn new_tables_keep_explicit_ids_columns_readers_and_inputs_in_both_envelopes() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, &[], |_| {});
        let inputs = [test.0.join("native"), test.0.join("accepted")];
        let before: Vec<_> = inputs.iter().map(|p| Inventory::read(p).unwrap()).collect();
        let manifest = fs::read(source.join("source.json")).unwrap();
        edit(&source, CATALOG, |owner| {
            let mut reference = attribute(102, "Reference", BOOLEAN);
            let Section::Properties(properties) = &mut node(&mut reference).sections[0] else {
                unreachable!()
            };
            let Property::Typed {
                value: PropertyValue::TypeDescription(value),
                ..
            } = &mut properties[3]
            else {
                unreachable!()
            };
            value.data.alternatives[0].name = "cfg:EnumRef.Status".into();
            children(owner).push(table(
                100,
                "Новые_Строки",
                vec![attribute(101, "Flag", BOOLEAN), reference],
            ));
        });
        edit(&source, DOCUMENT, |owner| {
            children(owner).push(table(200, "Rows", vec![]));
        });
        let authored = Inventory::read(&source).unwrap();
        let prepared =
            prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
        assert!(prepared.diagnostics().is_empty());
        for (path, index, parent, kind, name, count) in [
            (CATALOG, 100, "Records", "Catalog", "Новые_Строки", 6),
            (DOCUMENT, 200, "Entry", "Document", "Rows", 2),
        ] {
            let owner =
                ObjectSource::from_yaml(&fs::read_to_string(source.join(path)).unwrap()).unwrap();
            let native = &owner.document.path;
            let xml = fs::read_to_string(prepared.xml().join(native.as_path())).unwrap();
            assert!(xml.contains(&format!("name=\"{kind}TabularSection.{parent}.{name}\"")));
            assert!(xml.contains(&format!("name=\"{kind}TabularSectionRow.{parent}.{name}\"")));
            for identity in index + 1000..index + 1004 {
                assert!(xml.contains(uuid(identity).as_str()));
                assert_eq!(
                    base.permission(&uuid(identity), Operation::Resource).reason,
                    PermissionReason::UnknownObject
                );
            }
            assert_eq!(
                base.permission(&uuid(index), Operation::Create).reason,
                PermissionReason::UnknownObject
            );
            let reader = SourceReader::open(&source).unwrap();
            let source_path = SourcePath::parse(path).unwrap();
            let authored_yaml = fs::read_to_string(source.join(path)).unwrap();
            for mode in [PropertiesMode::All, PropertiesMode::Summary] {
                let saved = reader.read_metadata(&source_path, mode).unwrap();
                assert_eq!(
                    saved,
                    reader
                        .parse_metadata(&source_path, &authored_yaml, mode)
                        .unwrap()
                );
                assert_eq!(saved.documents[0].descriptor.objects.len(), count);
            }
        }
        let repeated =
            prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
        assert_eq!(prepared.generation(), repeated.generation());
        assert_eq!(authored, Inventory::read(&source).unwrap());
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

/// Table properties follow their actual native section; column values keep their own fresh object location.
#[test]
fn table_and_column_working_locations_use_actual_sections_and_exact_revisions() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, &[], |_| {});
        let original = fs::read_to_string(source.join(CATALOG)).unwrap();
        let working = edit(&source, CATALOG, |owner| {
            children(owner).push(table(
                100,
                "NewRows",
                vec![attribute(101, "NewFlag", BOOLEAN)],
            ));
        });
        let prepared =
            prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
        let path = SourcePath::parse("Catalogs/Records.xml").unwrap();
        let file = prepared.source_map().file(&path).unwrap();
        let xml = fs::read(prepared.xml().join(path.as_path())).unwrap();
        let native_xml = std::str::from_utf8(&xml).unwrap();
        for (needle, index, name) in [
            ("<Name>NewRows</Name>", 100, "NewRows"),
            ("<Name>NewFlag</Name>", 101, "NewFlag"),
            ("xs:boolean", 101, "xs:boolean"),
        ] {
            let owner_start = if needle == "xs:boolean" {
                native_xml
                    .find(&format!("<Attribute uuid=\"{}\"", uuid(index).as_str()))
                    .unwrap()
            } else {
                0
            };
            let start = owner_start + native_xml[owner_start..].find(needle).unwrap();
            let location = file
                .locate(
                    &xml,
                    start..start + needle.len(),
                    Some(OriginTexts {
                        accepted: &original,
                        working: Some(&working),
                    }),
                )
                .unwrap();
            assert_eq!(location.authority, InputAuthority::Working);
            assert_eq!(location.uuid, Some(uuid(index)));
            assert!(working[location.range.unwrap()].contains(name));
            let changed = working.clone() + "# Later revision\r\n";
            assert_eq!(
                file.locate(
                    &xml,
                    start..start + needle.len(),
                    Some(OriginTexts {
                        accepted: &original,
                        working: Some(&changed)
                    })
                )
                .unwrap_err()
                .code,
                "source_map_input_changed"
            );
        }
    }
}

/// A frozen owner or displaced old field refuses the whole table while stationary append and independent edits survive.
#[test]
fn new_table_permissions_and_displacement_do_not_discard_independent_fields() {
    for readable in [false, true] {
        for frozen in [1, 2, 11] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[uuid(frozen)], |_| {});
            edit(&source, CATALOG, |owner| {
                children(owner).push(table(100, "Rows", vec![attribute(101, "Flag", BOOLEAN)]));
            });
            edit(&source, DOCUMENT, |owner| {
                children(owner).push(table(200, "OtherRows", vec![]));
            });
            let output = test.0.join("generations");
            let prepared = prepare_protected(&source, &base, &output, true);
            if frozen == 11 {
                assert!(prepared.unwrap().diagnostics().is_empty());
            } else {
                let failure = prepared.unwrap_err();
                assert_eq!(failure.error.code, "source_changes_excluded");
                assert!(!output.exists());
                let recovered = prepare_protected(&source, &base, &output, false).unwrap();
                let catalog =
                    fs::read_to_string(recovered.xml().join("Catalogs/Records.xml")).unwrap();
                let document =
                    fs::read_to_string(recovered.xml().join("Documents/Entry.xml")).unwrap();
                assert!(!catalog.contains(uuid(100).as_str()));
                assert_eq!(document.contains(uuid(200).as_str()), frozen == 2);
            }
        }
    }
}

/// Unknown fields, namespaces, service layouts and non-attribute descendants have no proven inverse construction rule.
#[test]
fn malformed_new_table_profiles_refuse_before_generation() {
    for fault in [
        "missing_ids",
        "opaque_ids",
        "wrong_prefix",
        "extra_property",
        "namespace",
        "column_kind",
        "reference",
        "preserved",
        "column_namespace",
        "column_empty_type",
        "duplicate_column_name",
    ] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, true, &[], |_| {});
        edit(&source, CATALOG, |owner| {
            let mut created = table(100, "Rows", vec![attribute(101, "Flag", BOOLEAN)]);
            let table = node(&mut created);
            match fault {
                "missing_ids" => {
                    table.sections.remove(0);
                }
                "opaque_ids" => {
                    table.sections[0] = Section::Preserved(XmlFragment {
                        xml: "<InternalInfo/>".into(),
                        namespaces: vec![],
                    });
                }
                "wrong_prefix" => {
                    let Section::TabularSectionIdentities(ids) = &mut table.sections[0] else {
                        unreachable!()
                    };
                    ids.prefix = "v8".into();
                }
                "extra_property" => {
                    let Section::Properties(properties) = &mut table.sections[1] else {
                        unreachable!()
                    };
                    properties.push(Property::Typed {
                        name: PropertyName::FillChecking,
                        value: PropertyValue::Enumeration("DontCheck".into()),
                    });
                }
                "namespace" => table.namespaces.push(super::super::Namespace {
                    prefix: Some("other".into()),
                    uri: "urn:other".into(),
                }),
                _ => {
                    let Section::Children(columns) = &mut table.sections[2] else {
                        unreachable!()
                    };
                    match fault {
                        "column_kind" => node(&mut columns[0]).kind = MetadataKind::Dimension,
                        "reference" => columns.push(Child::Reference {
                            metadata_kind: MetadataKind::Form,
                            name: "Foreign".into(),
                        }),
                        "preserved" => columns.push(Child::Preserved {
                            fragment: XmlFragment {
                                xml: "<!-- unknown -->".into(),
                                namespaces: vec![],
                            },
                        }),
                        "column_namespace" => {
                            node(&mut columns[0])
                                .namespaces
                                .push(super::super::Namespace {
                                    prefix: Some("other".into()),
                                    uri: "urn:other".into(),
                                });
                        }
                        "column_empty_type" => {
                            let Section::Properties(props) = &mut node(&mut columns[0]).sections[0]
                            else {
                                unreachable!()
                            };
                            let Property::Typed {
                                value: PropertyValue::TypeDescription(t),
                                ..
                            } = &mut props[3]
                            else {
                                unreachable!()
                            };
                            t.data.alternatives.clear();
                            t.native.parts.clear();
                        }
                        "duplicate_column_name" => columns.push(attribute(102, "FLAG", BOOLEAN)),
                        _ => unreachable!(),
                    }
                }
            }
            children(owner).push(created);
        });
        let output = test.0.join("generations");
        assert!(
            prepare_protected(&source, &base, &output, false).is_err(),
            "{fault}"
        );
        assert!(!output.exists(), "{fault}");
    }
}

/// Known native generated UUIDs stay reserved even when the accepted YAML stores opaque declarations.
#[test]
fn new_table_ids_and_columns_cannot_reuse_accepted_metadata_or_opaque_service_ids() {
    for collision in [
        "metadata",
        "column_metadata",
        "service",
        "column_service",
        "own_role",
        "own_column",
        "padded_service",
    ] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, true, &[], |native| {
            let path = native.join("Enums/Status.xml");
            let native_xml=fs::read_to_string(&path).unwrap().replace("category=\"Ref\"/>",&format!("category=\"Ref\"><xr:TypeId>{}</xr:TypeId><xr:ValueId>{}</xr:ValueId></xr:GeneratedType>",uuid(51).as_str(),uuid(52).as_str()));
            let native_xml = if collision == "padded_service" {
                native_xml.replace(uuid(51).as_str(), &format!(" {} ", uuid(51).as_str()))
            } else {
                native_xml
            };
            fs::write(path, native_xml).unwrap();
        });
        edit(&source, CATALOG, |owner| {
            let mut created = table(100, "Rows", vec![attribute(101, "Flag", BOOLEAN)]);
            let table = node(&mut created);
            match collision {
                "column_metadata" | "column_service" | "own_column" => {
                    let Section::Children(columns) = &mut table.sections[2] else {
                        unreachable!()
                    };
                    node(&mut columns[0]).uuid = uuid(match collision {
                        "column_metadata" => 11,
                        "column_service" => 52,
                        _ => 1100,
                    });
                }
                _ => {
                    let Section::TabularSectionIdentities(ids) = &mut table.sections[0] else {
                        unreachable!()
                    };
                    ids.row.value_id = uuid(match collision {
                        "metadata" => 4,
                        "service" | "padded_service" => 51,
                        _ => 1100,
                    });
                }
            }
            children(owner).push(created);
        });
        let output = test.0.join("generations");
        assert_eq!(
            prepare_protected(&source, &base, &output, false)
                .unwrap_err()
                .error
                .code,
            "native_uuid_duplicate",
            "{collision}"
        );
        assert!(!output.exists());
    }
}

/// Inserting a table cannot displace a frozen accepted table of the same native class.
#[test]
fn shifted_frozen_table_refuses_creation_without_losing_another_owner() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, &[uuid(13)], |input| {
            let path = input.join("Catalogs/Records.xml");
            let old_table = format!(
                "<TabularSection uuid=\"{}\"><Properties><Name>ExistingRows</Name><Synonym/><Comment/></Properties><ChildObjects/></TabularSection>",
                uuid(13).as_str()
            );
            let native_xml = fs::read_to_string(&path)
                .unwrap()
                .replace("</ChildObjects>", &(old_table + "</ChildObjects>"));
            fs::write(path, native_xml).unwrap();
        });
        edit(&source, CATALOG, |owner| {
            children(owner).insert(2, table(100, "Rows", vec![]));
        });
        edit(&source, DOCUMENT, |owner| {
            children(owner).push(table(200, "OtherRows", vec![]));
        });
        let authored = Inventory::read(&source).unwrap();
        let output = test.0.join("generations");
        let denied = prepare_protected(&source, &base, &output, true).unwrap_err();
        assert_eq!(denied.error.code, "source_changes_excluded");
        assert_eq!(denied.diagnostics.len(), 1);
        assert_eq!(denied.diagnostics[0].uuid, Some(uuid(13)));
        assert_eq!(denied.diagnostics[0].operation, Operation::Create);
        assert_eq!(denied.diagnostics[0].reason, PermissionReason::EskaFrozen);
        assert!(!output.exists());
        let recovered = prepare_protected(&source, &base, &output, false).unwrap();
        assert!(
            !fs::read_to_string(recovered.xml().join("Catalogs/Records.xml"))
                .unwrap()
                .contains(uuid(100).as_str())
        );
        assert!(
            fs::read_to_string(recovered.xml().join("Documents/Entry.xml"))
                .unwrap()
                .contains(uuid(200).as_str())
        );
        assert_eq!(authored, Inventory::read(&source).unwrap());
    }
}

/// Unknown accepted composition remains intact, with an independently permitted root value still selected.
#[test]
fn preserved_child_prevents_table_creation_without_masking_allowed_properties() {
    let test = TestDir::new();
    let (source, base) = fixture(&test, true, &[], |input| {
        let path = input.join("Catalogs/Records.xml");
        let native_xml = fs::read_to_string(&path).unwrap().replace(
            "</ChildObjects>",
            "<!-- Unknown child group --></ChildObjects>",
        );
        fs::write(path, native_xml).unwrap();
    });
    edit(&source, CATALOG, |owner| {
        let Section::Properties(properties) = &mut owner.document.descriptor.object.sections[0]
        else {
            unreachable!()
        };
        let Property::Typed { value, .. } = &mut properties[2] else {
            unreachable!()
        };
        *value = PropertyValue::Text("Independent property".into());
        children(owner).push(table(100, "Rows", vec![]));
    });
    let output = test.0.join("generations");
    assert!(prepare_protected(&source, &base, &output, true).is_err());
    assert!(!output.exists());
    let recovered = prepare_protected(&source, &base, &output, false).unwrap();
    let native_xml = fs::read_to_string(recovered.xml().join("Catalogs/Records.xml")).unwrap();
    assert!(native_xml.contains("<!-- Unknown child group -->"));
    assert!(native_xml.contains("Independent property"));
    assert!(!native_xml.contains(uuid(100).as_str()));
}

/// A table profile cannot grant creation to another accepted root class or hide a missing column type target.
#[test]
fn unsupported_parent_and_missing_column_targets_refuse_before_publication() {
    for unsupported in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, true, &[], |input| {
            if unsupported {
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
                let path = input.join("DataProcessors/Records.xml");
                let native_xml = fs::read_to_string(&path)
                    .unwrap()
                    .replace("<Catalog uuid=", "<DataProcessor uuid=")
                    .replace("</Catalog>", "</DataProcessor>");
                fs::write(path, native_xml).unwrap();
            }
        });
        let path = if unsupported {
            "DataProcessors/Records/object.yaml"
        } else {
            CATALOG
        };
        edit(&source, path, |owner| {
            let mut column = attribute(101, "Field", BOOLEAN);
            let Section::Properties(properties) = &mut node(&mut column).sections[0] else {
                unreachable!()
            };
            let Property::Typed {
                value: PropertyValue::TypeDescription(value),
                ..
            } = &mut properties[3]
            else {
                unreachable!()
            };
            value.data.alternatives[0].name = "cfg:EnumRef.Missing".into();
            children(owner).push(table(100, "Rows", vec![column]));
        });
        let output = test.0.join("generations");
        let denied = prepare_protected(&source, &base, &output, true).unwrap_err();
        assert_eq!(
            denied.error.code,
            if unsupported {
                "source_changes_excluded"
            } else {
                "source_type_target_missing"
            }
        );
        assert!(!output.exists());
        if unsupported {
            assert_eq!(
                base.permission(&uuid(2), Operation::Create).reason,
                PermissionReason::UnsupportedOperation
            );
            let recovered = prepare_protected(&source, &base, &output, false).unwrap();
            assert!(
                !fs::read_to_string(recovered.xml().join("DataProcessors/Records.xml"))
                    .unwrap()
                    .contains(uuid(100).as_str())
            );
        } else {
            assert_eq!(
                prepare_protected(&source, &base, &output, false)
                    .unwrap_err()
                    .error
                    .code,
                "source_type_target_missing"
            );
        }
    }
}

/// Service roles from independently created owners are globally distinct in the final composition.
#[test]
fn new_tables_in_different_descriptors_cannot_share_generated_uuids() {
    let test = TestDir::new();
    let (source, base) = fixture(&test, true, &[], |_| {});
    edit(&source, CATALOG, |owner| {
        children(owner).push(table(100, "Rows", vec![]));
    });
    edit(&source, DOCUMENT, |owner| {
        let mut created = table(200, "Rows", vec![]);
        let Section::TabularSectionIdentities(ids) = &mut node(&mut created).sections[0] else {
            unreachable!()
        };
        ids.table.type_id = uuid(1100);
        children(owner).push(created);
    });
    let output = test.0.join("generations");
    assert_eq!(
        prepare_protected(&source, &base, &output, false)
            .unwrap_err()
            .error
            .code,
        "native_uuid_duplicate"
    );
    assert!(!output.exists());
}

/// Attributes and tables share names while columns have a separate local namespace.
#[test]
fn new_table_names_and_native_class_groups_retain_existing_composition() {
    for fault in [
        "name_collision",
        "before_attributes",
        "attribute_after_table",
        "bad_name",
        "column_matches_table",
    ] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, true, &[], |_| {});
        edit(&source, CATALOG, |owner| {
            let name = match fault {
                "name_collision" => "existing",
                "bad_name" => "Bad name",
                _ => "Rows",
            };
            let created = table(100, name, vec![attribute(101, "Rows", BOOLEAN)]);
            if fault == "before_attributes" {
                children(owner).insert(0, created);
            } else {
                children(owner).push(created);
            }
            if fault == "attribute_after_table" {
                children(owner).push(attribute(102, "Other", BOOLEAN));
            }
        });
        let output = test.0.join("generations");
        let prepared = prepare_protected(&source, &base, &output, true);
        if fault == "column_matches_table" {
            assert!(prepared.unwrap().diagnostics().is_empty());
        } else {
            assert!(prepared.is_err(), "{fault}");
            assert!(!output.exists(), "{fault}");
        }
    }
}
