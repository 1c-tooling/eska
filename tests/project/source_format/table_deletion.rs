//! Native supplier authority applies to the root namespace, table and every deleted column separately.

use super::*;
use eska::project_core::source_format::{
    AcceptedBase, Operation, PermissionReason, accept_source, import_designer_readable,
    prepare_protected,
};

/// Upgrade only the private native table/column profile and install complete supplier rules for structural consumers.
pub(super) fn fixture(root: &Path, locked: char) -> PathBuf {
    let input = super::baseline::fixture(root);
    let file = input.join("Catalogs/Sample.xml");
    let mut native = fs::read_to_string(&file).unwrap();
    native=native.replace("xmlns:xsi=","xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" xmlns:xr=\"http://v8.1c.ru/8.3/xcf/readable\" xmlns:xsi=");
    let declarations=[("TabularSection",'b','c'),("TabularSectionRow",'d','e')].map(|(role,type_id,value_id)|format!("<xr:GeneratedType name=\"Catalog{role}.Sample.Items\" category=\"{role}\"><xr:TypeId>{}</xr:TypeId><xr:ValueId>{}</xr:ValueId></xr:GeneratedType>",super::baseline::uuid(type_id).as_str(),super::baseline::uuid(value_id).as_str())).concat();
    native=native.replace("<Properties><Name>Items</Name><Comment xsi:nil=\"true\"/></Properties>",&format!("<InternalInfo>{declarations}</InternalInfo><Properties><Name>Items</Name><Synonym/><Comment/></Properties>"));
    native=native.replace("<Name>Count</Name><Comment>Nested attribute</Comment>","<Name>Count</Name><Synonym/><Comment>Nested attribute</Comment><Type><v8:Type>xs:boolean</v8:Type></Type>");
    fs::write(file, native).unwrap();
    // This fixture isolates supplier permissions; XML/BSL dependency refusal is tested independently.
    let form = input.join("Catalogs/Sample/Forms/Main/Ext/Form.xml");
    fs::write(
        &form,
        fs::read_to_string(&form)
            .unwrap()
            .replace("name=\"Count\"", "name=\"IndependentFormField\""),
    )
    .unwrap();
    let file = input.join("Ext/ParentConfigurations.bin");
    let rules = ['2', '3', '4', '5']
        .map(|id| {
            let uuid = super::baseline::uuid(id);
            format!("{1},0,{0},{0}", uuid.as_str(), u8::from(id != locked))
        })
        .join(",");
    let suffix = std::iter::repeat_n("0", 13).collect::<Vec<_>>().join(",");
    fs::write(file, format!(
        "{{6,0,1,{0},0,{1},\"1.0.0.0\",\"Fixture vendor\",\"SampleConfiguration\",4,{rules},0,0,{suffix}}}\r\n",
        super::baseline::uuid('a').as_str(), super::baseline::uuid('1').as_str()
    )).unwrap();
    input
}

/// A locked column vetoes an otherwise editable table deletion without suppressing an independent permitted field comment.
#[test]
fn supplier_root_table_and_column_each_keep_their_native_deletion_authority() {
    for readable in [false, true] {
        for locked in ['2', '4', '5', '0'] {
            let test = TestDir::new();
            let input = fixture(&test.0, locked);
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
                base.permission(&uuid('4'), Operation::Delete).allowed,
                locked != '4'
            );
            assert_eq!(
                base.permission(&uuid('5'), Operation::Delete).allowed,
                locked != '5'
            );
            let path = source.join("Catalogs/Sample/object.yaml");
            let mut owner = ObjectSource::from_yaml(&fs::read_to_string(&path).unwrap()).unwrap();
            for section in &mut owner.document.descriptor.object.sections {
                if let Section::Children(children) = section {
                    children.retain(
                        |child| !matches!(child,Child::Inline { object } if object.uuid==uuid('4')),
                    );
                    for child in children {
                        if let Child::Inline { object } = child
                            && object.uuid == uuid('3')
                        {
                            for section in &mut object.sections {
                                if let Section::Properties(properties) = section {
                                    for property in properties {
                                        if let eska::project_core::source_format::Property::Typed {
                                            name: PropertyName::Comment,
                                            value,
                                        } = property {
                                            *value = PropertyValue::Text("Independent field comment".into());
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
            if locked != '0' {
                let failed = prepare_protected(&source, &base, &output, true).unwrap_err();
                assert_eq!(failed.diagnostics.len(), 1);
                assert_eq!(failed.diagnostics[0].uuid, Some(uuid(locked)));
                assert_eq!(failed.diagnostics[0].operation, Operation::Delete);
                assert_eq!(
                    failed.diagnostics[0].reason,
                    PermissionReason::NativeRestriction
                );
                assert!(!output.exists());
            }
            let selected = prepare_protected(&source, &base, &output, false).unwrap();
            let xml = fs::read_to_string(selected.xml().join("Catalogs/Sample.xml")).unwrap();
            assert_eq!(xml.contains(uuid('4').as_str()), locked != '0');
            assert_eq!(xml.contains(uuid('5').as_str()), locked != '0');
            assert!(xml.contains("Independent field comment"));
            assert_eq!(super::baseline::tree(&source), authored);
            assert_eq!(super::baseline::tree(&accepted), pinned);
            assert_eq!(super::baseline::tree(&input), original);
        }
    }
}
