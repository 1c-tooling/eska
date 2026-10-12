//! Renaming an own inline value still requires the supplier owner's namespace permission.

use super::*;
use eska::project_core::source_format::{
    AcceptedBase, Operation, PermissionReason, Property, accept_source, import_designer_readable,
    prepare_protected,
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
