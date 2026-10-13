//! Table names have one editable authority; derived declarations retain accepted service identities.

use std::{fs, path::Path};

use super::*;
use crate::{
    project_core::{
        metadata_parser::PropertiesMode,
        source_format::{
            Descriptor, InputAuthority, OriginTexts, PermissionReason, SourceReader,
            attribute_test_support::{comment, member, rename, table},
            child_creation::attribute_tests::{attribute_xml, edit},
            enum_test_support::{children, uuid},
            files::Inventory,
            prepare_protected,
            tabular_test_support::{fixture, fresh_table},
        },
    },
    test_support::TestDir,
};

const CATALOG: &str = "Catalogs/Records/object.yaml";
const DOCUMENT: &str = "Documents/Entry/object.yaml";

/// Independent native control changes only the table Name and its two derived declaration names.
fn renamed_native(test: &TestDir, path: &str, kind: &str, parent: &str, name: &str) -> String {
    fs::read_to_string(test.0.join("native").join(path))
        .unwrap()
        .replace("<Name>Rows</Name>", &format!("<Name>{name}</Name>"))
        .replace(
            &format!("{kind}TabularSection.{parent}.Rows"),
            &format!("{kind}TabularSection.{parent}.{name}"),
        )
        .replace(
            &format!("{kind}TabularSectionRow.{parent}.Rows"),
            &format!("{kind}TabularSectionRow.{parent}.{name}"),
        )
}

/// Two parent classes retain all UUIDs, column types, sibling order, inputs and repeatable generation identity.
#[test]
fn table_rename_derives_native_names_without_replacing_identity_or_inputs() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, &[], |_| {});
        let before =
            [test.0.join("native"), test.0.join("accepted")].map(|p| Inventory::read(&p).unwrap());
        let manifest = fs::read(source.join("source.json")).unwrap();
        edit(&source, CATALOG, |owner| rename(owner, 13, "Новые_Строки"));
        edit(&source, DOCUMENT, |owner| rename(owner, 14, "Changed_Rows"));
        let authored = Inventory::read(&source).unwrap();
        let output = test.0.join("generations");
        let prepared = prepare_protected(&source, &base, &output, true).unwrap();
        assert!(prepared.diagnostics().is_empty());
        for (path, kind, parent, name) in [
            ("Catalogs/Records.xml", "Catalog", "Records", "Новые_Строки"),
            ("Documents/Entry.xml", "Document", "Entry", "Changed_Rows"),
        ] {
            assert_eq!(
                Descriptor::from_xml(&fs::read_to_string(prepared.xml().join(path)).unwrap())
                    .unwrap(),
                Descriptor::from_xml(&renamed_native(&test, path, kind, parent, name)).unwrap()
            );
        }
        assert!(base.permission(&uuid(13), Operation::Rename).allowed);
        assert_eq!(
            base.permission(&uuid(1130), Operation::Rename).reason,
            PermissionReason::UnknownObject
        );
        assert!(
            !base
                .permission(&uuid(13), Operation::Property(PropertyName::Name))
                .allowed
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

/// The parent namespace and table govern rename; unchanged descendants and stationary siblings keep their own authority.
#[test]
fn table_rename_freezes_only_actual_parent_and_table_and_recovers_independent_edits() {
    for readable in [false, true] {
        for frozen in [2, 13, 11, 15, 21, 22, 23] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[uuid(frozen)], |_| {});
            edit(&source, CATALOG, |owner| rename(owner, 13, "ChangedRows"));
            edit(&source, DOCUMENT, |owner| {
                rename(owner, 14, "IndependentRows");
                comment(
                    &mut owner.document.descriptor.object,
                    "Independent document comment",
                );
            });
            let output = test.0.join("generations");
            let denied = [2, 13].contains(&frozen);
            if denied {
                let failure = prepare_protected(&source, &base, &output, true).unwrap_err();
                assert_eq!(failure.diagnostics.len(), 1);
                assert_eq!(failure.diagnostics[0].uuid, Some(uuid(frozen)));
                assert_eq!(failure.diagnostics[0].operation, Operation::Rename);
                assert_eq!(failure.diagnostics[0].reason, PermissionReason::EskaFrozen);
                assert!(!output.exists());
            }
            let prepared = prepare_protected(&source, &base, &output, !denied).unwrap();
            let catalog = fs::read_to_string(prepared.xml().join("Catalogs/Records.xml")).unwrap();
            assert_eq!(
                catalog.contains("CatalogTabularSectionRow.Records.ChangedRows"),
                !denied
            );
            assert_eq!(catalog.contains("<Name>Rows</Name>"), denied);
            let document = fs::read_to_string(prepared.xml().join("Documents/Entry.xml")).unwrap();
            assert!(document.contains("DocumentTabularSectionRow.Entry.IndependentRows"));
            assert!(document.contains("Independent document comment"));
        }
    }
}

/// All root attributes/tables share one case-insensitive namespace; name swaps are validated after selection.
#[test]
fn table_names_reject_invalid_identifiers_and_collisions_but_allow_unreferenced_swaps() {
    for readable in [false, true] {
        for (next, code) in [
            ("", "source_metadata_name_invalid"),
            ("2Rows", "source_metadata_name_invalid"),
            ("New Rows", "source_metadata_name_invalid"),
            ("EXISTING", "source_metadata_name_collision"),
            ("remainingrows", "source_metadata_name_collision"),
        ] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[], |_| {});
            edit(&source, CATALOG, |owner| rename(owner, 13, next));
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
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, &[], |_| {});
        edit(&source, CATALOG, |owner| {
            rename(owner, 13, "RemainingRows");
            rename(owner, 15, "Rows");
        });
        let prepared =
            prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
        let xml = fs::read_to_string(prepared.xml().join("Catalogs/Records.xml")).unwrap();
        let mut owner = super::super::ObjectSource {
            form: None,
            document: super::super::NativeDocument {
                path: SourcePath::parse("Catalogs/Records.xml").unwrap(),
                descriptor: Descriptor::from_xml(&xml).unwrap(),
            },
            nested: vec![],
        };
        for (id, name) in [(13, "RemainingRows"), (15, "Rows")] {
            assert_eq!(
                table(&mut owner, id).property(PropertyName::Name),
                Some(&PropertyValue::Text(name.into()))
            );
        }
        for id in [1130, 1131, 1132, 1133, 1150, 1151, 1152, 1153] {
            assert!(xml.contains(uuid(id).as_str()));
        }
    }
}

/// Unknown native profiles restore the table Name and declarations while unrelated comments remain selected.
#[test]
fn unverified_table_rename_profiles_do_not_gain_structural_authority() {
    for readable in [false, true] {
        for shape in ["property", "column", "namespace", "identities"] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[], |input| {
                let file = input.join("Catalogs/Records.xml");
                let native = fs::read_to_string(&file).unwrap();
                let next = match shape {
                    "property" => native.replacen("<ToolTip/>", "<Future/><ToolTip/>", 1),
                    "column" => native.replace(
                        "<Name>FirstColumn</Name>",
                        "<Name>FirstColumn</Name><Future/>",
                    ),
                    "namespace" => native.replacen(
                        "<TabularSection uuid=",
                        "<TabularSection xmlns:future=\"urn:future\" uuid=",
                        1,
                    ),
                    _ => native.replacen("category=\"TabularSection\"", "category=\"Future\"", 1),
                };
                fs::write(file, next).unwrap();
            });
            edit(&source, CATALOG, |owner| {
                rename(owner, 13, "ChangedRows");
                comment(&mut owner.document.descriptor.object, "Independent comment");
            });
            let output = test.0.join("generations");
            assert!(prepare_protected(&source, &base, &output, true).is_err());
            assert!(!output.exists());
            let prepared = prepare_protected(&source, &base, &output, false).unwrap();
            let xml = fs::read_to_string(prepared.xml().join("Catalogs/Records.xml")).unwrap();
            assert!(!xml.contains("ChangedRows") && xml.contains("Independent comment"));
        }
    }
}

/// Existing name mentions refuse rename; table, column and generated UUID mentions remain bound to the same identities.
#[test]
fn table_name_dependencies_are_guarded_without_invalidating_uuid_references() {
    for readable in [false, true] {
        for kind in ["xml", "bsl", "uuid", "case_only"] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[], |input| {
                let directory = input.join("Documents/Entry/Ext");
                fs::create_dir_all(&directory).unwrap();
                if kind == "xml" {
                    fs::write(
                        directory.join("Extra.xml"),
                        "<Form><DataPath><![CDATA[Object.Rows]]></DataPath></Form>",
                    )
                    .unwrap();
                } else if kind == "uuid" {
                    fs::write(
                        directory.join("Extra.xml"),
                        format!(
                            "<Root refs=\"{}\"/>",
                            [13, 21, 1130, 1131, 1132, 1133]
                                .map(|id| uuid(id).as_str().to_owned())
                                .join(" ")
                        ),
                    )
                    .unwrap();
                } else {
                    fs::write(
                        directory.join("ObjectModule.bsl"),
                        "\u{feff}Object.Rows.Clear();\r\n",
                    )
                    .unwrap();
                }
                if kind == "case_only" {
                    fs::write(directory.join("Extra.xml"), "<unverified").unwrap();
                }
            });
            edit(&source, CATALOG, |owner| {
                rename(
                    owner,
                    13,
                    if kind == "case_only" {
                        "rOWS"
                    } else {
                        "ChangedRows"
                    },
                );
            });
            let authored = Inventory::read(&source).unwrap();
            let output = test.0.join("generations");
            for strict in [true, false] {
                let result = prepare_protected(&source, &base, &output, strict);
                if ["xml", "bsl"].contains(&kind) {
                    assert_eq!(
                        result.unwrap_err().error.code,
                        "source_structural_reference_preserved"
                    );
                    assert!(!output.exists());
                } else {
                    let prepared = result.unwrap();
                    let xml =
                        fs::read_to_string(prepared.xml().join("Catalogs/Records.xml")).unwrap();
                    assert!(xml.contains(if kind == "case_only" {
                        "CatalogTabularSectionRow.Records.rOWS"
                    } else {
                        "CatalogTabularSectionRow.Records.ChangedRows"
                    }));
                }
            }
            assert_eq!(Inventory::read(&source).unwrap(), authored);
        }
    }
}

/// A permitted resource fix clears the old name; a frozen resource still contributes its accepted dependency.
#[test]
fn selected_module_authority_controls_table_rename_dependencies() {
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
                        "\u{feff}Object.Rows.Clear();\r\n",
                    )
                    .unwrap();
                },
            );
            edit(&source, CATALOG, |owner| rename(owner, 13, "ChangedRows"));
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

/// The accepted row target is installed before pinning, without a guessed type binding.
fn row_reference(input: &Path) {
    let path = input.join("Catalogs/Records.xml");
    let native = fs::read_to_string(&path).unwrap();
    fs::write(
        path,
        native.replace(
            &attribute_xml(11, "Existing", "<v8:Type>xs:boolean</v8:Type>"),
            &attribute_xml(
                11,
                "Existing",
                "<v8:Type>cfg:CatalogTabularSectionRow.Records.Rows</v8:Type>",
            ),
        ),
    )
    .unwrap();
}

/// Typed references follow one explicitly authored alternative; generated names alone never establish old UUID authority.
fn row_target(owner: &mut super::super::ObjectSource, name: &str) {
    let node = member(&mut owner.document.descriptor.object, 11).unwrap();
    for section in &mut node.sections {
        if let Section::Properties(properties) = section {
            for property in properties {
                if let Property::Typed {
                    value: PropertyValue::TypeDescription(description),
                    ..
                } = property
                {
                    description.data.alternatives[0].name = name.into();
                }
            }
        }
    }
}

/// Missing/reused old row names cannot silently retarget; an explicitly selected fix to the renamed table remains valid.
#[test]
fn typed_row_dependencies_require_a_fix_even_when_a_fresh_table_reuses_the_name() {
    for readable in [false, true] {
        for (reuse, fix, code) in [
            (false, false, Some("source_type_target_missing")),
            (true, false, Some("source_structural_reference_preserved")),
            (false, true, None),
            (true, true, None),
        ] {
            let test = TestDir::new();
            let (source, base) = fixture(&test, readable, &[], row_reference);
            edit(&source, CATALOG, |owner| {
                rename(owner, 13, "ChangedRows");
                if reuse {
                    children(owner).push(fresh_table(100, "Rows", vec![]));
                }
                if fix {
                    row_target(owner, "cfg:CatalogTabularSectionRow.Records.ChangedRows");
                }
            });
            let output = test.0.join("generations");
            for strict in [true, false] {
                let result = prepare_protected(&source, &base, &output, strict);
                if let Some(code) = code {
                    let error = result.unwrap_err().error;
                    assert_eq!(error.code, code);
                    assert!(!output.exists());
                } else {
                    assert!(result.is_ok());
                }
            }
        }
    }
}

/// Derived names map to the exact working Name; service spans and refused renames retain accepted coordinates.
#[test]
fn table_generated_names_have_current_working_coordinates_and_accepted_service_origins() {
    for readable in [false, true] {
        for frozen in [false, true] {
            let test = TestDir::new();
            let (source, base) = fixture(
                &test,
                readable,
                &if frozen { vec![uuid(13)] } else { vec![] },
                |_| {},
            );
            edit(&source, CATALOG, |owner| {
                rename(owner, 13, "ChangedRows");
                if !frozen {
                    children(owner).remove(1);
                }
            });
            let prepared =
                prepare_protected(&source, &base, &test.0.join("generations"), !frozen).unwrap();
            let native = SourcePath::parse("Catalogs/Records.xml").unwrap();
            let xml = fs::read_to_string(prepared.xml().join(native.as_path())).unwrap();
            let file = prepared.source_map().file(&native).unwrap();
            let source_path = SourcePath::parse(CATALOG).unwrap();
            let accepted = base.read_file(&source_path).unwrap();
            let working = fs::read_to_string(source.join(CATALOG)).unwrap();
            let name = if frozen { "Rows" } else { "ChangedRows" };
            for value in [
                format!("<Name>{name}</Name>"),
                format!("CatalogTabularSection.Records.{name}"),
                format!("CatalogTabularSectionRow.Records.{name}"),
            ] {
                let offset =
                    xml.find(&value).unwrap() + usize::from(value.starts_with("<Name>")) * 6;
                let location = file
                    .locate(
                        xml.as_bytes(),
                        offset..offset + 1,
                        Some(OriginTexts {
                            accepted: &accepted,
                            working: Some(&working),
                        }),
                    )
                    .unwrap();
                assert_eq!(
                    location.authority,
                    if frozen {
                        InputAuthority::Accepted
                    } else {
                        InputAuthority::Working
                    }
                );
                assert_eq!(location.uuid, Some(uuid(13)));
                let authored = if frozen { &accepted } else { &working };
                assert!(authored[location.range.unwrap()].contains(name));
            }
            for id in [1130, 1131, 1132, 1133] {
                let offset = xml.find(uuid(id).as_str()).unwrap();
                let location = file
                    .locate(
                        xml.as_bytes(),
                        offset..offset + 36,
                        Some(OriginTexts {
                            accepted: &accepted,
                            working: Some(&working),
                        }),
                    )
                    .unwrap();
                assert_eq!(location.authority, InputAuthority::Accepted);
                assert_eq!(location.uuid, Some(uuid(13)));
                assert!(accepted[location.range.unwrap()].contains(uuid(id).as_str()));
            }
            for mode in [PropertiesMode::All, PropertiesMode::Summary] {
                let reader = SourceReader::open(&source).unwrap();
                assert_eq!(
                    reader.read_metadata(&source_path, mode).unwrap(),
                    reader.parse_metadata(&source_path, &working, mode).unwrap()
                );
            }
            if !frozen {
                let offset = xml.find("ChangedRows").unwrap();
                assert_eq!(
                    file.locate(
                        xml.as_bytes(),
                        offset..offset + 1,
                        Some(OriginTexts {
                            accepted: &accepted,
                            working: Some(&(working.clone() + "\n"))
                        })
                    )
                    .unwrap_err()
                    .code,
                    "source_map_input_changed"
                );
            }
        }
    }
}

/// Editing a service slot never substitutes its pinned identity, even when a separately permitted Name is recovered.
#[test]
fn table_rename_does_not_adopt_working_service_uuid_edits() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = fixture(&test, readable, &[], |_| {});
        edit(&source, CATALOG, |owner| {
            let node = table(owner, 13);
            for section in &mut node.sections {
                if let Section::TabularSectionIdentities(ids) = section {
                    ids.row.value_id = uuid(9000);
                }
            }
            rename(owner, 13, "ChangedRows");
        });
        let output = test.0.join("generations");
        assert!(prepare_protected(&source, &base, &output, true).is_err());
        assert!(!output.exists());
        let prepared = prepare_protected(&source, &base, &output, false).unwrap();
        let xml = fs::read_to_string(prepared.xml().join("Catalogs/Records.xml")).unwrap();
        assert!(xml.contains("CatalogTabularSectionRow.Records.ChangedRows"));
        assert!(xml.contains(uuid(1133).as_str()));
        assert!(!xml.contains(uuid(9000).as_str()));
    }
}
