//! Native restrictions and class slots remain authoritative when collection order is edited.

use std::fs;

use eska::project::source_format::{
    AcceptedBase, Child, ObjectSource, Operation, PermissionReason, Property, PropertyName,
    PropertyValue, Section, accept_source, import_designer, import_designer_readable,
    prepare_protected,
};

use super::baseline::{fixture, tree, uuid};
use crate::support::TestDir;

/// Add a same-class sibling only to the owned native fixture, retaining the real policy container.
fn extra_attribute(input: &std::path::Path) {
    let file = input.join("Catalogs/Sample.xml");
    let text = fs::read_to_string(&file).unwrap().replace(
        "<TabularSection uuid=",
        &format!(
            "<Attribute uuid=\"{}\"><Properties><Name>Extra</Name><Comment>Old</Comment></Properties></Attribute><TabularSection uuid=",
            uuid('8').as_str()
        ),
    );
    fs::write(file, text).unwrap();
}

/// Locked owners cannot reorder editable children; their independent property edit still survives.
#[test]
fn native_parent_policy_denies_order_in_both_envelopes_and_preserves_working_bytes() {
    for readable in [false, true] {
        let test = TestDir::new();
        let input = fixture(&test.0);
        extra_attribute(&input);
        let source = test.0.join("source");
        if readable {
            import_designer_readable(&input, &source).unwrap();
        } else {
            import_designer(&input, &source).unwrap();
        }
        let base_root = test.0.join("base");
        let pin = accept_source(&source, &base_root, &[]).unwrap();
        let base = AcceptedBase::open(&base_root, &pin).unwrap();
        let file = source.join("Catalogs/Sample/object.yaml");
        let mut value = ObjectSource::from_yaml(&fs::read_to_string(&file).unwrap()).unwrap();
        let items = value
            .document
            .descriptor
            .object
            .sections
            .iter_mut()
            .find_map(|section| {
                if let Section::Children(items) = section {
                    Some(items)
                } else {
                    None
                }
            })
            .unwrap();
        items.swap(0, 1);
        let Child::Inline { object } = &mut items[1] else {
            panic!("editable attribute");
        };
        for section in &mut object.sections {
            if let Section::Properties(properties) = section {
                for property in properties {
                    if let Property::Typed {
                        name: PropertyName::Comment,
                        value,
                    } = property
                    {
                        *value = PropertyValue::Text("Allowed child value".into());
                    }
                }
            }
        }
        fs::write(&file, value.to_yaml().unwrap()).unwrap();
        let before = tree(&source);
        let accepted = tree(&base_root);
        let output = test.0.join("generations");
        let failure = prepare_protected(&source, &base, &output, true).unwrap_err();
        assert_eq!(failure.error.code, "source_changes_excluded");
        assert_eq!(failure.diagnostics.len(), 1);
        assert_eq!(failure.diagnostics[0].operation, Operation::Reorder);
        assert_eq!(failure.diagnostics[0].uuid, Some(uuid('2')));
        assert_eq!(
            failure.diagnostics[0].reason,
            PermissionReason::NativeRestriction
        );
        assert!(!output.exists());
        let report = prepare_protected(&source, &base, &output, false).unwrap();
        let xml = fs::read_to_string(report.xml().join("Catalogs/Sample.xml")).unwrap();
        assert!(
            xml.find("<Name>Description</Name>").unwrap() < xml.find("<Name>Extra</Name>").unwrap()
        );
        assert!(xml.contains("Allowed child value"));
        assert_eq!(tree(&source), before);
        assert_eq!(tree(&base_root), accepted);
    }
}

/// A permission to order members does not move a tabular section into an attribute class slot.
#[test]
fn native_class_positions_remain_fixed_for_unrestricted_members() {
    let test = TestDir::new();
    let input = super::fixture(&test.0);
    fs::remove_file(input.join("Ext/ParentConfigurations.bin")).unwrap();
    let source = test.0.join("source");
    import_designer_readable(&input, &source).unwrap();
    let pin = accept_source(&source, &test.0.join("base"), &[]).unwrap();
    let base = AcceptedBase::open(&test.0.join("base"), &pin).unwrap();
    let file = source.join("Catalogs/Sample/object.yaml");
    let mut value = ObjectSource::from_yaml(&fs::read_to_string(&file).unwrap()).unwrap();
    let items = value
        .document
        .descriptor
        .object
        .sections
        .iter_mut()
        .find_map(|section| {
            if let Section::Children(items) = section {
                Some(items)
            } else {
                None
            }
        })
        .unwrap();
    items.swap(0, 1);
    fs::write(file, value.to_yaml().unwrap()).unwrap();
    let failure = prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap_err();
    assert_eq!(failure.diagnostics.len(), 1);
    assert_eq!(failure.diagnostics[0].operation, Operation::Reorder);
    assert_eq!(
        failure.diagnostics[0].reason,
        PermissionReason::UnsupportedOperation
    );
    let report = prepare_protected(&source, &base, &test.0.join("generations"), false).unwrap();
    let xml = fs::read_to_string(report.xml().join("Catalogs/Sample.xml")).unwrap();
    assert!(
        xml.find("<Name>Description</Name>").unwrap() < xml.find("<Name>Items</Name>").unwrap()
    );
}
