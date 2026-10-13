//! Renaming an own inline value still requires the supplier owner's namespace permission.

use super::*;
use eska::project_core::source_format::{
    AcceptedBase, MetadataNode, Operation, PermissionReason, Property, accept_source,
    import_designer_readable, prepare_protected,
};

/// A locked native parent blocks Name while preserving the unrestricted value's independent Comment.
#[test]
fn supplier_parent_denies_rename_but_keeps_an_independent_value_edit() {
    for readable in [false, true] {
        let test = TestDir::new();
        let input = super::creation::locked_enum_fixture(&test.0);
        let native = input.join("Enums/Sample.xml");
        let mut descriptor = Descriptor::from_xml(&fs::read_to_string(&native).unwrap()).unwrap();
        for section in &mut descriptor.object.sections {
            if let Section::Children(children) = section {
                children.push(super::creation::addition());
            }
        }
        fs::write(native, descriptor.to_xml().unwrap()).unwrap();
        let source = test.0.join("source");
        if readable {
            import_designer_readable(&input, &source).unwrap();
        } else {
            import_designer(&input, &source).unwrap();
        }
        let accepted = test.0.join("accepted");
        let pin = accept_source(&source, &accepted, &[]).unwrap();
        let base = AcceptedBase::open(&accepted, &pin).unwrap();
        assert!(
            base.permission(&super::baseline::uuid('9'), Operation::Rename)
                .allowed
        );
        let path = source.join("Enums/Sample/object.yaml");
        let mut owner = ObjectSource::from_yaml(&fs::read_to_string(&path).unwrap()).unwrap();
        for section in &mut owner.document.descriptor.object.sections {
            if let Section::Children(children) = section {
                for child in children {
                    if let Child::Inline { object } = child
                        && object.uuid == super::baseline::uuid('9')
                    {
                        for section in &mut object.sections {
                            if let Section::Properties(properties) = section {
                                for property in properties {
                                    if let Property::Typed { name, value } = property {
                                        match name {
                                            PropertyName::Name => {
                                                *value = PropertyValue::Text("Renamed".into());
                                            }
                                            PropertyName::Comment => {
                                                *value = PropertyValue::Text(
                                                    "Independent value edit".into(),
                                                );
                                            }
                                            _ => {}
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
        let output = test.0.join("generations");
        let failure = prepare_protected(&source, &base, &output, true).unwrap_err();
        assert_eq!(failure.diagnostics.len(), 1);
        assert_eq!(failure.diagnostics[0].operation, Operation::Rename);
        assert_eq!(
            failure.diagnostics[0].uuid,
            Some(super::baseline::uuid('2'))
        );
        assert_eq!(
            failure.diagnostics[0].reason,
            PermissionReason::NativeRestriction
        );
        assert!(!output.exists());
        let restored = prepare_protected(&source, &base, &output, false).unwrap();
        let xml = fs::read_to_string(restored.xml().join("Enums/Sample.xml")).unwrap();
        assert!(xml.contains("NewValue") && !xml.contains("Renamed"));
        assert!(xml.contains("Independent value edit"));
        assert_eq!(super::baseline::tree(&source), authored);
        assert_eq!(super::baseline::tree(&accepted), pinned);
    }
}

/// Two editable native member UUIDs receive Name changes; the root field also has an independent permitted Comment.
fn author_supplier_fields(node: &mut MetadataNode) {
    let uuid = super::baseline::uuid;
    for section in &mut node.sections {
        match section {
            Section::Properties(properties) => {
                for property in properties {
                    if let Property::Typed { name, value } = property {
                        match name {
                            PropertyName::Name if node.uuid == uuid('3') => {
                                *value = PropertyValue::Text("ChangedRootField".into());
                            }
                            PropertyName::Name if node.uuid == uuid('5') => {
                                *value = PropertyValue::Text("ChangedColumn".into());
                            }
                            PropertyName::Comment if node.uuid == uuid('3') => {
                                *value = PropertyValue::Text("Independent field edit".into());
                            }
                            _ => {}
                        }
                    }
                }
            }
            Section::Children(children) => {
                for child in children {
                    if let Child::Inline { object } = child {
                        author_supplier_fields(object);
                    }
                }
            }
            _ => {}
        }
    }
}

/// A locked catalog namespace retains its root field name while an independently editable table column can change.
#[test]
fn supplier_root_namespace_and_table_columns_keep_independent_rename_permissions() {
    for readable in [false, true] {
        let test = TestDir::new();
        let input = super::baseline::fixture(&test.0);
        let native = input.join("Catalogs/Sample.xml");
        let mut xml = fs::read_to_string(&native).unwrap().replace(
            "xmlns:xsi=",
            "xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" xmlns:xsi=",
        );
        for (name, comment) in [
            ("Description", "Editable part"),
            ("Count", "Nested attribute"),
        ] {
            xml = xml.replace(
                &format!("<Name>{name}</Name><Comment>{comment}</Comment>"),
                &format!("<Name>{name}</Name><Synonym/><Comment>{comment}</Comment><Type><v8:Type>xs:boolean</v8:Type></Type>"),
            );
        }
        fs::write(native, xml).unwrap();
        // This fixture isolates supplier authority; real form and BSL dependencies have refusal coverage.
        let form = input.join("Catalogs/Sample/Forms/Main/Ext/Form.xml");
        fs::write(
            &form,
            fs::read_to_string(&form)
                .unwrap()
                .replace("name=\"Count\"", "name=\"IndependentFormField\""),
        )
        .unwrap();
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
            base.permission(&uuid('2'), Operation::Reorder).reason,
            PermissionReason::NativeRestriction
        );
        assert!(base.permission(&uuid('3'), Operation::Rename).allowed);
        assert!(base.permission(&uuid('4'), Operation::Reorder).allowed);
        assert!(base.permission(&uuid('5'), Operation::Rename).allowed);
        let path = source.join("Catalogs/Sample/object.yaml");
        let mut owner = ObjectSource::from_yaml(&fs::read_to_string(&path).unwrap()).unwrap();
        author_supplier_fields(&mut owner.document.descriptor.object);
        fs::write(path, owner.to_yaml().unwrap()).unwrap();
        let authored = super::baseline::tree(&source);
        let pinned = super::baseline::tree(&accepted);
        let original = super::baseline::tree(&input);
        let output = test.0.join("generations");
        let failure = prepare_protected(&source, &base, &output, true).unwrap_err();
        assert_eq!(failure.diagnostics.len(), 1);
        assert_eq!(failure.diagnostics[0].operation, Operation::Rename);
        assert_eq!(failure.diagnostics[0].uuid, Some(uuid('2')));
        assert_eq!(
            failure.diagnostics[0].reason,
            PermissionReason::NativeRestriction
        );
        assert!(!output.exists());
        let prepared = prepare_protected(&source, &base, &output, false).unwrap();
        let xml = fs::read_to_string(prepared.xml().join("Catalogs/Sample.xml")).unwrap();
        assert!(xml.contains("<Name>Description</Name>") && !xml.contains("ChangedRootField"));
        assert!(
            xml.contains("<Name>ChangedColumn</Name>") && xml.contains("Independent field edit")
        );
        assert_eq!(super::baseline::tree(&source), authored);
        assert_eq!(super::baseline::tree(&accepted), pinned);
        assert_eq!(super::baseline::tree(&input), original);
    }
}
