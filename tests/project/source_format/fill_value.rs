//! Named filling values retain the supplier's per-object property authority.

use super::*;
use eska::project_core::source_format::{
    AcceptedBase, Operation, PermissionReason, accept_source, import_designer_readable,
    prepare_protected,
};

const FIRST: &str = "Enum.Status.EnumValue.First";
const SECOND: &str = "Enum.Status.EnumValue.Second";

/// Native supplier policy locks one field while a support-removed subtree keeps its own field editable.
#[test]
fn supplier_field_restores_filling_reference_but_keeps_independent_permitted_field() {
    for readable in [false, true] {
        let directory = TestDir::new();
        let input = super::baseline::fixture(&directory.0);
        let config = input.join("Configuration.xml");
        fs::write(
            &config,
            fs::read_to_string(&config)
                .unwrap()
                .replace("</ChildObjects>", "<Enum>Status</Enum></ChildObjects>"),
        )
        .unwrap();
        fs::create_dir(input.join("Enums")).unwrap();
        fs::write(input.join("Enums/Status.xml"),format!(
            "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><Enum uuid=\"{}\"><Properties><Name>Status</Name></Properties><ChildObjects><EnumValue uuid=\"{}\"><Properties><Name>First</Name><Synonym/><Comment/></Properties></EnumValue><EnumValue uuid=\"{}\"><Properties><Name>Second</Name><Synonym/><Comment/></Properties></EnumValue></ChildObjects></Enum></MetaDataObject>",
            "80000000-0000-4000-8000-000000000011", "80000000-0000-4000-8000-000000000012", "80000000-0000-4000-8000-000000000013",
        )).unwrap();
        let catalog = input.join("Catalogs/Sample.xml");
        let original=fs::read_to_string(&catalog).unwrap().replace("xmlns:v8=","xmlns:xr=\"http://v8.1c.ru/8.3/xcf/readable\" xmlns:v8=")
            .replace("<Comment>Editable part</Comment>",&format!("<Comment>Editable part</Comment><FillValue xsi:type=\"xr:DesignTimeRef\">{FIRST}</FillValue>"))
            .replace("<Comment>Nested attribute</Comment>",&format!("<Comment>Nested attribute</Comment><FillValue xsi:type=\"xr:DesignTimeRef\">{FIRST}</FillValue>"));
        fs::write(
            catalog,
            Descriptor::from_xml(&original).unwrap().to_xml().unwrap(),
        )
        .unwrap();
        let policy = input.join("Ext/ParentConfigurations.bin");
        let locked = format!("1,0,{0},{0}", super::baseline::uuid('3').as_str());
        let data = fs::read_to_string(&policy).unwrap();
        assert!(data.contains(&locked));
        fs::write(
            policy,
            data.replace(
                &locked,
                &format!("0,0,{0},{0}", super::baseline::uuid('3').as_str()),
            ),
        )
        .unwrap();
        let source = directory.0.join("source");
        if readable {
            import_designer_readable(&input, &source).unwrap();
        } else {
            import_designer(&input, &source).unwrap();
        }
        let storage = directory.0.join("accepted");
        let pin = accept_source(&source, &storage, &[]).unwrap();
        let base = AcceptedBase::open(&storage, &pin).unwrap();
        assert!(
            !base
                .permission(
                    &super::baseline::uuid('3'),
                    Operation::Property(PropertyName::FillValue)
                )
                .allowed
        );
        assert!(
            base.permission(
                &super::baseline::uuid('5'),
                Operation::Property(PropertyName::FillValue)
            )
            .allowed
        );
        let path = source.join("Catalogs/Sample/object.yaml");
        fs::write(
            &path,
            fs::read_to_string(&path).unwrap().replace(FIRST, SECOND),
        )
        .unwrap();
        let authored = super::baseline::tree(&source);
        let accepted = super::baseline::tree(&storage);
        let output = directory.0.join("generations");
        let failure = prepare_protected(&source, &base, &output, true).unwrap_err();
        assert_eq!(failure.diagnostics.len(), 1);
        assert_eq!(
            failure.diagnostics[0].uuid,
            Some(super::baseline::uuid('3'))
        );
        assert_eq!(
            failure.diagnostics[0].operation,
            Operation::Property(PropertyName::FillValue)
        );
        assert_eq!(
            failure.diagnostics[0].reason,
            PermissionReason::NativeRestriction
        );
        assert!(!output.exists());
        let selected = prepare_protected(&source, &base, &output, false).unwrap();
        let xml = fs::read_to_string(selected.xml().join("Catalogs/Sample.xml")).unwrap();
        assert_eq!(xml.matches(FIRST).count(), 1);
        assert_eq!(xml.matches(SECOND).count(), 1);
        assert_eq!(super::baseline::tree(&source), authored);
        assert_eq!(super::baseline::tree(&storage), accepted);
    }
}
