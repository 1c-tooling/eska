//! Whole-configuration consumers never fabricate a base for canonical external slots.

use super::*;
use hir::ConfigsDatabase;

use crate::{
    metadata::{intern_configuration_path, MdoEntry},
    AnalysisProvider, RootDatabase, SalsaProvider,
};

/// All public/provider whole-context surfaces retain absence while the own typed object stays usable.
#[test]
fn canonical_external_roots_have_no_whole_configuration_on_any_consumer_surface() {
    for kind in
        [bsl_metadata::ExternalObjectKind::DataProcessor, bsl_metadata::ExternalObjectKind::Report]
    {
        let mut fixture = Fixture::new();
        let roots = fixture.roots(&[("external", RootKind::External(kind), &[])]);
        let root = &roots[0];
        fixture.file(root, 0, "Owner.xml", None);
        fixture.file(root, 1, "Owner/Ext/ObjectModule.bsl", Some(""));
        let slot = fixture.slot(Some(CanonicalMetadata::Object(object(
            kind.mdo_type(),
            "Owner",
            "Current",
        ))));
        fixture.listing(root, listing(kind), &[(0, slot)]);
        assert!(fixture.module(1).mdo.is_some());
        for unavailable in [false, true] {
            if unavailable {
                update(&mut fixture, slot, None);
            }
            assert!(fixture.database.get_configuration(FileId(1)).is_none());
            assert!(fixture.database.get_all_configurations(FileId(1)).is_empty());
            assert!(fixture.database.all_configurations_inventory().is_empty());
            assert!(fixture.database.merged_visible_configuration(FileId(1)).is_none());
            assert!(fixture.database.configuration_for_root(root).is_none());
            let path = intern_configuration_path(&fixture.database, root.to_str().unwrap(), 0);
            let provider = SalsaProvider::new(&fixture.database, Some(path));
            assert!(provider.configuration().is_none());
            assert!(provider.visible_configurations(FileId(1)).is_empty());
            fixture.database.warm_config_roots(&[hir::ModuleId { file_id: FileId(1) }]);
            assert!(fixture
                .database
                .resolve_common_module_for_file(FileId(1), "Foreign")
                .is_none());
            assert!(fixture.database.common_module_for_file_id(FileId(1)).is_none());
            assert!(fixture
                .database
                .resolve_common_module_files_for_file(FileId(1), "Foreign")
                .is_empty());
        }
        assert_eq!(fixture.database.salsa_family_churn("load_configuration").unwrap().0, 0);
    }
}

/// A legacy native external export still contributes its own real object through the old container loader.
#[test]
fn noncanonical_native_external_whole_configuration_surfaces_keep_their_real_object() {
    for kind in
        [bsl_metadata::ExternalObjectKind::DataProcessor, bsl_metadata::ExternalObjectKind::Report]
    {
        let mut fixture = Fixture::new();
        let roots = fixture.roots(&[("external", RootKind::External(kind), &[])]);
        let root = &roots[0];
        fs::create_dir(root.join("Owner")).unwrap();
        fs::write(root.join("Owner.xml"), format!("<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\"><{} uuid=\"10000000-0000-4000-8000-000000000001\"><Properties><Name>Owner</Name></Properties></{}></MetaDataObject>",kind.element_name(),kind.element_name())).unwrap();
        fixture.file(root, 0, "Owner.xml", None);
        fixture.file(root, 1, "Owner/Ext/ObjectModule.bsl", Some(""));
        fixture.listing(root, listing(kind), &[]);
        let config = fixture.database.get_configuration(FileId(1)).expect("native container");
        assert!(config.find_metadata_object(kind.mdo_type(), "Owner").is_some());
        assert_eq!(fixture.database.get_all_configurations(FileId(1)).len(), 1);
        assert_eq!(fixture.database.all_configurations_inventory().len(), 1);
        assert!(fixture.database.merged_visible_configuration(FileId(1)).is_some());
        assert!(fixture.database.configuration_for_root(root).is_some());
        let path = intern_configuration_path(&fixture.database, root.to_str().unwrap(), 0);
        let provider = SalsaProvider::new(&fixture.database, Some(path));
        assert!(provider.configuration().is_some());
        assert_eq!(provider.visible_configurations(FileId(1)).len(), 1);
    }
}

/// Native and canonical modes share the same descriptor identity; only provider authority differs.
fn listing(kind: bsl_metadata::ExternalObjectKind) -> MetadataListingData {
    MetadataListingData {
        entries: vec![MdoEntry {
            kind: kind.mdo_type(),
            name: "Owner".into(),
            main: FileId(0),
            predefined: None,
        }],
        ..Default::default()
    }
}
