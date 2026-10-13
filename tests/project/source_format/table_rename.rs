//! Supplier authority for table rename does not become inherited authority over unchanged columns.

use super::*;
use eska::project_core::source_format::{
    AcceptedBase, Operation, PermissionReason, Property, accept_source, import_designer_readable,
    prepare_protected,
};

/// Complete supplier rules retain the actual parent/table gates while an independently permitted field comment survives.
#[test]
fn supplier_parent_and_table_gate_rename_without_requiring_unchanged_column_edits() {
    for readable in [false, true] {
        for locked in ['2', '4', '5', '0'] {
            let test = TestDir::new();
            let input = super::table_deletion::fixture(&test.0, locked);
            let source = test.0.join("source");
            if readable {
                import_designer_readable(&input, &source).unwrap();
            } else {
                import_designer(&input, &source).unwrap();
            }
            let accepted = test.0.join("accepted");
            let pin = accept_source(&source, &accepted, &[]).unwrap();
            let base = AcceptedBase::open(&accepted, &pin).unwrap();
            let uuid = super::baseline::uuid;
            assert_eq!(
                base.permission(&uuid('4'), Operation::Rename).allowed,
                locked != '4'
            );
            let path = source.join("Catalogs/Sample/object.yaml");
            let mut owner = ObjectSource::from_yaml(&fs::read_to_string(&path).unwrap()).unwrap();
            for section in &mut owner.document.descriptor.object.sections {
                if let Section::Children(children) = section {
                    for child in children {
                        if let Child::Inline { object } = child {
                            let change = if object.uuid == uuid('4') {
                                Some((PropertyName::Name, "ChangedItems"))
                            } else if object.uuid == uuid('3') {
                                Some((PropertyName::Comment, "Independent field comment"))
                            } else {
                                None
                            };
                            if let Some((name, text)) = change {
                                for section in &mut object.sections {
                                    if let Section::Properties(properties) = section {
                                        for property in properties {
                                            if let Property::Typed {
                                                name: candidate,
                                                value,
                                            } = property
                                                && *candidate == name
                                            {
                                                *value = PropertyValue::Text(text.into());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            fs::write(path, owner.to_yaml().unwrap()).unwrap();
            let authored = super::baseline::tree(&source);
            let pinned = super::baseline::tree(&accepted);
            let original = super::baseline::tree(&input);
            let output = test.0.join("generations");
            let denied = ['2', '4'].contains(&locked);
            if denied {
                let failure = prepare_protected(&source, &base, &output, true).unwrap_err();
                assert_eq!(failure.diagnostics.len(), 1);
                assert_eq!(failure.diagnostics[0].uuid, Some(uuid(locked)));
                assert_eq!(failure.diagnostics[0].operation, Operation::Rename);
                assert_eq!(
                    failure.diagnostics[0].reason,
                    PermissionReason::NativeRestriction
                );
                assert!(!output.exists());
            }
            let selected = prepare_protected(&source, &base, &output, !denied).unwrap();
            let xml = fs::read_to_string(selected.xml().join("Catalogs/Sample.xml")).unwrap();
            assert_eq!(
                xml.contains("CatalogTabularSectionRow.Sample.ChangedItems"),
                !denied
            );
            assert_eq!(xml.contains("<Name>Items</Name>"), denied);
            assert!(xml.contains("Independent field comment"));
            for id in ['2', '3', '4', '5', 'b', 'c', 'd', 'e'] {
                assert!(xml.contains(uuid(id).as_str()));
            }
            assert_eq!(super::baseline::tree(&source), authored);
            assert_eq!(super::baseline::tree(&accepted), pinned);
            assert_eq!(super::baseline::tree(&input), original);
        }
    }
}
