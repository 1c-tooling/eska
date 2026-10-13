//! Native service identities have one source and cannot grant policy or replace preserved unknown data.

use std::{fs, path::Path};

use super::super::enum_test_support::{children, native, uuid};
use super::*;
use crate::{
    project_core::{
        metadata_parser::PropertiesMode,
        source_format::{
            AcceptedBase, Child, Descriptor, ObjectSource, Operation, PermissionReason, Property,
            Section, SourcePath, SourceReader, XmlFragment, accept_source, export_designer,
            files::Inventory, import_designer, import_designer_readable, prepare_protected,
        },
    },
    test_support::TestDir,
};

const CATALOG: &str = "Catalogs/Records/object.yaml";

/// Exact generated names and roles bind four distinct UUIDs to a pre-existing table and row type.
fn info(kind: &str, owner: &str, offset: u32) -> String {
    format!(
        "<InternalInfo><xr:GeneratedType name=\"{kind}TabularSection.{owner}.Rows\" category=\"TabularSection\"><xr:TypeId>{}</xr:TypeId><xr:ValueId>{}</xr:ValueId></xr:GeneratedType><xr:GeneratedType name=\"{kind}TabularSectionRow.{owner}.Rows\" category=\"TabularSectionRow\"><xr:TypeId>{}</xr:TypeId><xr:ValueId>{}</xr:ValueId></xr:GeneratedType></InternalInfo>",
        uuid(offset).as_str(),
        uuid(offset + 1).as_str(),
        uuid(offset + 2).as_str(),
        uuid(offset + 3).as_str()
    )
}

/// The table stays inline under its actual root; a declared field consumes its generated row type.
fn descriptor(kind: &str, owner: &str, index: u32, offset: u32) -> String {
    let table = format!(
        "<TabularSection uuid=\"{}\">{}<Properties><Name>Rows</Name><Synonym/><Comment/></Properties><ChildObjects/></TabularSection>",
        uuid(index + 10).as_str(),
        info(kind, owner, offset)
    );
    let field = format!(
        "<Attribute uuid=\"{}\"><Properties><Name>TableType</Name><Type><v8:Type>cfg:{kind}TabularSectionRow.{owner}.Rows</v8:Type></Type></Properties></Attribute>",
        uuid(index + 20).as_str()
    );
    native(kind,index,owner,&(field+&table)).replace("version=\"2.20\"", "xmlns:xr=\"http://v8.1c.ru/8.3/xcf/readable\" xmlns:cfg=\"http://v8.1c.ru/8.1/data/enterprise/current-config\" version=\"2.20\"")
}

/// Both source envelopes share accepted declarations and have no implicit promotion when reopening YAML.
fn fixture(
    test: &TestDir,
    readable: bool,
    adjust: impl FnOnce(&Path),
) -> (std::path::PathBuf, AcceptedBase) {
    let input = test.0.join("native");
    fs::create_dir_all(input.join("Catalogs")).unwrap();
    fs::create_dir_all(input.join("Documents")).unwrap();
    fs::write(
        input.join("Configuration.xml"),
        native(
            "Configuration",
            1,
            "Sample",
            "<Catalog>Records</Catalog><Document>Entry</Document>",
        ),
    )
    .unwrap();
    fs::write(
        input.join("Catalogs/Records.xml"),
        descriptor("Catalog", "Records", 2, 31),
    )
    .unwrap();
    fs::write(
        input.join("Documents/Entry.xml"),
        descriptor("Document", "Entry", 3, 41),
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
    let pin = accept_source(&source, &accepted, &[]).unwrap();
    (source, AcceptedBase::open(&accepted, &pin).unwrap())
}

/// Locate the actual table without introducing a parallel UUID/path index in tests.
fn table(owner: &mut ObjectSource) -> &mut MetadataNode {
    children(owner)
        .iter_mut()
        .find_map(|child| {
            if let Child::Inline { object } = child
                && object.kind == MetadataKind::TabularSection
            {
                Some(object.as_mut())
            } else {
                None
            }
        })
        .unwrap()
}

/// Both parent kinds retain four UUIDs and derive native names from their sole editable Name values.
#[test]
fn native_ids_round_trip_without_duplicated_generated_names_in_either_envelope() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, |_| {});
        for (path, kind, owner) in [
            (CATALOG, "Catalog", "Records"),
            ("Documents/Entry/object.yaml", "Document", "Entry"),
        ] {
            let yaml = fs::read_to_string(source.join(path)).unwrap();
            assert!(!yaml.contains("GeneratedType"));
            assert!(!yaml.contains(&format!("{kind}TabularSection.{owner}.Rows")));
            let mut document = ObjectSource::from_yaml(&yaml).unwrap();
            assert!(matches!(
                table(&mut document).sections[0],
                Section::TabularSectionIdentities(_)
            ));
            let xml = document.document.descriptor.to_xml().unwrap();
            assert!(xml.contains(&format!("name=\"{kind}TabularSection.{owner}.Rows\"")));
            let native = Descriptor::from_xml(&xml).unwrap();
            assert_eq!(document.document.descriptor.object, native.object);
            assert_eq!(document.document.descriptor.xml, native.xml);
        }
        let output = test.0.join("exported");
        export_designer(&source, &output).unwrap();
        for path in [
            "Configuration.xml",
            "Catalogs/Records.xml",
            "Documents/Entry.xml",
        ] {
            assert_eq!(
                Descriptor::from_xml(&fs::read_to_string(output.join(path)).unwrap()).unwrap(),
                Descriptor::from_xml(
                    &fs::read_to_string(test.0.join("native").join(path)).unwrap()
                )
                .unwrap()
            );
        }
        let prepared =
            prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
        assert!(prepared.diagnostics().is_empty());
        assert_eq!(
            Inventory::read(prepared.xml()).unwrap(),
            Inventory::read(&output).unwrap()
        );
        assert_eq!(
            base.permission(&uuid(31), Operation::Resource).reason,
            PermissionReason::UnknownObject
        );
    }
}

/// Exact names, roles and `QName` syntax are required; unknown native information retains its original slot.
#[test]
fn unrecognized_native_identity_profiles_remain_preserved() {
    for fault in [
        "name",
        "category",
        "extra",
        "comment",
        "alias",
        "local_namespace",
        "cdata",
        "role_order",
        "uuid",
        "wrong_parent",
    ] {
        let mut xml = descriptor("Catalog", "Records", 2, 31);
        match fault {
            "name" => {
                xml = xml.replace(
                    "CatalogTabularSection.Records.Rows",
                    "CatalogTabularSection.Records.Other",
                );
            }
            "category" => {
                xml = xml.replace("category=\"TabularSectionRow\"", "category=\"FutureRole\"");
            }
            "extra" => xml = xml.replace("</InternalInfo>", "<Future/></InternalInfo>"),
            "comment" => xml = xml.replace("</InternalInfo>", "<!-- future --></InternalInfo>"),
            "alias" => {
                xml = xml
                    .replace(
                        "<xr:TypeId>",
                        "<other:TypeId xmlns:other=\"http://v8.1c.ru/8.3/xcf/readable\">",
                    )
                    .replace("</xr:TypeId>", "</other:TypeId>");
            }
            "uuid" => xml = xml.replace(uuid(31).as_str(), "invalid-guid"),
            "local_namespace" => {
                xml = xml.replace(
                    "<xr:GeneratedType name=",
                    "<xr:GeneratedType xmlns:future=\"urn:future\" name=",
                );
            }
            "cdata" => {
                xml = xml.replace(
                    uuid(31).as_str(),
                    &format!("<![CDATA[{}]]>", uuid(31).as_str()),
                );
            }
            "role_order" => {
                xml = xml
                    .replace("<xr:TypeId>", "<xr:Other>")
                    .replace("</xr:TypeId>", "</xr:Other>")
                    .replace("<xr:ValueId>", "<xr:TypeId>")
                    .replace("</xr:ValueId>", "</xr:TypeId>")
                    .replace("<xr:Other>", "<xr:ValueId>")
                    .replace("</xr:Other>", "</xr:ValueId>");
            }
            "wrong_parent" => {
                xml = xml
                    .replace("<Catalog uuid=", "<DataProcessor uuid=")
                    .replace("</Catalog>", "</DataProcessor>");
            }
            _ => unreachable!(),
        }
        let mut owner = ObjectSource {
            form: None,
            document: super::super::NativeDocument {
                path: SourcePath::parse("Catalogs/Records.xml").unwrap(),
                descriptor: Descriptor::from_xml(&xml).unwrap(),
            },
            nested: Vec::new(),
        };
        assert!(
            matches!(table(&mut owner).sections[0], Section::Preserved(_)),
            "{fault}"
        );
        let output = owner.document.descriptor.to_xml().unwrap();
        assert_eq!(
            Descriptor::from_xml(&output).unwrap(),
            owner.document.descriptor
        );
    }
}

/// Service identity edits are restored before independent permitted values are selected.
#[test]
fn changing_accepted_service_ids_cannot_escape_protection_or_discard_allowed_comments() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, |_| {});
        let path = source.join(CATALOG);
        let mut owner = ObjectSource::from_yaml(&fs::read_to_string(&path).unwrap()).unwrap();
        let node = table(&mut owner);
        let Section::TabularSectionIdentities(ids) = &mut node.sections[0] else {
            unreachable!()
        };
        ids.table.type_id = uuid(99);
        let Section::Properties(properties) = &mut node.sections[1] else {
            unreachable!()
        };
        let Property::Typed { value, .. } = &mut properties[2] else {
            unreachable!()
        };
        *value = PropertyValue::Text("Independent edit".into());
        fs::write(&path, owner.to_yaml().unwrap()).unwrap();
        let authored = Inventory::read(&source).unwrap();
        let accepted = Inventory::read(&test.0.join("accepted")).unwrap();
        let output = test.0.join("generations");
        let failure = prepare_protected(&source, &base, &output, true).unwrap_err();
        assert_eq!(failure.error.code, "source_changes_excluded");
        assert!(!output.exists());
        let prepared = prepare_protected(&source, &base, &output, false).unwrap();
        let xml = fs::read_to_string(prepared.xml().join("Catalogs/Records.xml")).unwrap();
        assert!(xml.contains(uuid(31).as_str()));
        assert!(!xml.contains(uuid(99).as_str()));
        assert!(xml.contains("Independent edit"));
        assert_eq!(Inventory::read(&source).unwrap(), authored);
        assert_eq!(Inventory::read(&test.0.join("accepted")).unwrap(), accepted);
    }
}

/// Identities have no standalone owner, foreign namespace capability or alternative root-parent contract.
#[test]
fn ids_reject_missing_parent_foreign_namespace_and_other_owner_classes() {
    for fault in ["parent", "namespace", "kind", "duplicate_section"] {
        let test = TestDir::new();
        let (source, _) = fixture(&test, true, |_| {});
        let mut owner =
            ObjectSource::from_yaml(&fs::read_to_string(source.join(CATALOG)).unwrap()).unwrap();
        match fault {
            "parent" => owner.document.descriptor.object = table(&mut owner).clone(),
            "namespace" => {
                let Section::TabularSectionIdentities(ids) = &mut table(&mut owner).sections[0]
                else {
                    unreachable!()
                };
                ids.prefix = "v8".into();
            }
            "kind" => owner.document.descriptor.object.kind = MetadataKind::DataProcessor,
            "duplicate_section" => {
                let node = table(&mut owner);
                node.sections.insert(1, node.sections[0].clone());
            }
            _ => unreachable!(),
        }
        assert!(owner.document.descriptor.to_xml().is_err(), "{fault}");
    }
}

/// Service UUID collisions are checked across the table's roles and all modeled native objects in a document.
#[test]
fn service_identity_collisions_refuse_materialization() {
    for collision in [2, 12, 31, 32, 22] {
        let test = TestDir::new();
        let (source, _) = fixture(&test, true, |_| {});
        let mut owner =
            ObjectSource::from_yaml(&fs::read_to_string(source.join(CATALOG)).unwrap()).unwrap();
        let Section::TabularSectionIdentities(ids) = &mut table(&mut owner).sections[0] else {
            unreachable!()
        };
        ids.row.value_id = uuid(collision);
        assert_eq!(
            owner.document.descriptor.to_xml().unwrap_err().code,
            "native_uuid_duplicate"
        );
    }
}

/// YAML reopening and migration keep old opaque service fields intact instead of inferring a new profile.
#[test]
fn old_preserved_service_information_stays_opaque_when_read_and_migrated() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, _) = fixture(&test, readable, |_| {});
        let path = source.join(CATALOG);
        let mut owner = ObjectSource::from_yaml(&fs::read_to_string(&path).unwrap()).unwrap();
        table(&mut owner).sections[0] = Section::Preserved(XmlFragment {
            xml: info("Catalog", "Records", 31),
            namespaces: Vec::new(),
        });
        let yaml = owner.to_yaml().unwrap();
        fs::write(&path, &yaml).unwrap();
        let mut reopened = ObjectSource::from_yaml(&yaml).unwrap();
        assert!(matches!(
            table(&mut reopened).sections[0],
            Section::Preserved(_)
        ));
        let reader = SourceReader::open(&source).unwrap();
        let source_path = SourcePath::parse(CATALOG).unwrap();
        assert_eq!(
            reader
                .read_metadata(&source_path, PropertiesMode::All)
                .unwrap(),
            reader
                .parse_metadata(&source_path, &yaml, PropertiesMode::All)
                .unwrap()
        );
        let destination = test.0.join("migrated");
        let plan = super::super::prepare_readable_layout(&source).unwrap();
        plan.publish(&destination).unwrap();
        let mut migrated =
            ObjectSource::from_yaml(&fs::read_to_string(destination.join(CATALOG)).unwrap())
                .unwrap();
        assert!(matches!(
            table(&mut migrated).sections[0],
            Section::Preserved(_)
        ));
    }
}

/// Derived type names resolve in the common target index, native projection and saved/overlay readers.
#[test]
fn table_and_row_types_use_actual_parent_names_in_saved_and_overlay_semantics() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, _) = fixture(&test, readable, |_| {});
        let reader = SourceReader::open(&source).unwrap();
        let path = SourcePath::parse(CATALOG).unwrap();
        let yaml = fs::read_to_string(source.join(CATALOG)).unwrap();
        for mode in [PropertiesMode::All, PropertiesMode::Summary] {
            let saved = reader.read_metadata(&path, mode).unwrap();
            assert_eq!(saved, reader.parse_metadata(&path, &yaml, mode).unwrap());
            assert_eq!(saved.documents[0].descriptor.objects.len(), 3);
        }
        let mut owner = ObjectSource::from_yaml(&yaml).unwrap();
        let node = table(&mut owner);
        let Section::Properties(properties) = &mut node.sections[1] else {
            unreachable!()
        };
        let Property::Typed { value, .. } = &mut properties[0] else {
            unreachable!()
        };
        *value = PropertyValue::Text("OtherRows".into());
        let xml = owner.document.descriptor.to_xml().unwrap();
        assert!(xml.contains("CatalogTabularSection.Records.OtherRows"));
        assert!(xml.contains("CatalogTabularSectionRow.Records.OtherRows"));
        assert!(!xml.contains("name=\"CatalogTabularSection.Records.Rows\""));
    }
}

/// Service UUID annotations and quoting follow their native fields through both migration entry layouts.
#[test]
fn service_identity_annotations_survive_readable_migration_without_value_promotion() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, _) = fixture(&test, readable, |_| {});
        let path = source.join(CATALOG);
        let original = fs::read_to_string(&path).unwrap();
        let styled: Vec<_> = original
            .lines()
            .map(|line| {
                let Some((head, value)) = line.split_once(':') else {
                    return line.to_owned();
                };
                let key = head.trim().trim_matches('"');
                if key == "type_id" && value.contains(uuid(31).as_str()) {
                    format!("{head}: '{}' # Native type identity", uuid(31).as_str())
                } else if key == "prefix" && value.trim().trim_matches('"') == "xr" {
                    format!("{head}: xr # Native namespace")
                } else {
                    line.to_owned()
                }
            })
            .collect();
        let authored = styled.join("\r\n") + "\r\n";
        fs::write(&path, &authored).unwrap();
        let before = Inventory::read(&source).unwrap();
        let exported = test.0.join("original-export");
        export_designer(&source, &exported).unwrap();
        let plan = super::super::prepare_readable_layout(&source).unwrap();
        let destination = test.0.join("migrated");
        plan.publish(&destination).unwrap();
        plan.publish(&destination).unwrap();
        let migrated = fs::read_to_string(destination.join(CATALOG)).unwrap();
        assert!(migrated.contains(&format!("'{}' # Native type identity", uuid(31).as_str())));
        assert!(migrated.contains("xr # Native namespace"));
        assert!(migrated.contains("\r\n"));
        let output = test.0.join("migrated-export");
        export_designer(&destination, &output).unwrap();
        assert_eq!(
            Inventory::read(&exported).unwrap(),
            Inventory::read(&output).unwrap()
        );
        assert_eq!(before, Inventory::read(&source).unwrap());
    }
}

/// XML prefixes with dots remain valid bindings and are not confused with generated type name separators.
#[test]
fn exact_readable_namespace_alias_with_dot_round_trips() {
    let xml = descriptor("Catalog", "Records", 2, 31)
        .replace("xmlns:xr=", "xmlns:x.r=")
        .replace("xr:", "x.r:");
    let model = Descriptor::from_xml(&xml).unwrap();
    let output = model.to_xml().unwrap();
    assert!(output.contains("<x.r:GeneratedType"));
    assert_eq!(model, Descriptor::from_xml(&output).unwrap());
}
