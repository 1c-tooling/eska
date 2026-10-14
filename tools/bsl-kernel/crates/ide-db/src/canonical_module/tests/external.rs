//! External owners stay distinct from same-named internal objects and do not need Configuration.

use super::*;
use crate::metadata::MdoEntry;

/// Both external kinds carry their current attributes, invalidate on edits and fail closed.
#[test]
fn external_object_modules_use_current_own_slots_and_preserve_neighbors() {
    for external_kind in
        [bsl_metadata::ExternalObjectKind::DataProcessor, bsl_metadata::ExternalObjectKind::Report]
    {
        let mut fixture = Fixture::new();
        let roots = fixture.roots(&[
            ("base", RootKind::Base, &[]),
            // A root deliberately named like an internal collection must still be external.
            ("DataProcessors", RootKind::External(external_kind), &[]),
        ]);
        fixture.file(&roots[0], 0, "DataProcessors/АРМ.xml", None);
        fixture.file(&roots[0], 1, "DataProcessors/АРМ/Ext/ObjectModule.bsl", Some(""));
        fixture.file(&roots[1], 2, "АРМ.xml", None);
        fixture.file(&roots[1], 3, "АРМ/Ext/ObjectModule.bsl", Some(""));
        let internal = fixture.slot(Some(CanonicalMetadata::Object(object(
            MdoType::DataProcessor,
            "АРМ",
            "Internal",
        ))));
        let saved = object(external_kind.mdo_type(), "АРМ", "Saved");
        let external = fixture.slot(Some(CanonicalMetadata::Object(Arc::clone(&saved))));
        for (root, kind, main, slot) in [
            (&roots[0], MdoType::DataProcessor, 0, internal),
            (&roots[1], external_kind.mdo_type(), 2, external),
        ] {
            fixture.listing(
                root,
                MetadataListingData {
                    entries: vec![MdoEntry {
                        kind,
                        name: "АРМ".into(),
                        main: FileId(main),
                        predefined: None,
                    }],
                    ..Default::default()
                },
                &[(main, slot)],
            );
        }
        let module = fixture.module(3);
        assert_eq!(module.module_type, bsl_metadata::ModuleType::ObjectModule);
        assert_eq!(module.mdo.as_ref().expect("external owner"), &saved);
        assert!(module.form.is_none());
        let neighbor = fixture.module(1);
        let current = object(external_kind.mdo_type(), "АРМ", "Current");
        update(&mut fixture, external, Some(CanonicalMetadata::Object(Arc::clone(&current))));
        assert_eq!(fixture.module(3).mdo.as_ref().unwrap(), &current);
        assert!(Arc::ptr_eq(&neighbor, &fixture.module(1)));
        update(&mut fixture, external, None);
        assert!(fixture.module(3).mdo.is_none());
        update(
            &mut fixture,
            external,
            Some(CanonicalMetadata::Object(object(MdoType::DataProcessor, "АРМ", "WrongKind"))),
        );
        assert!(fixture.module(3).mdo.is_none());
        update(
            &mut fixture,
            external,
            Some(CanonicalMetadata::Object(object(external_kind.mdo_type(), "Other", "WrongName"))),
        );
        assert!(fixture.module(3).mdo.is_none());
        assert!(Arc::ptr_eq(&neighbor, &fixture.module(1)));
        assert_eq!(fixture.database.salsa_family_churn("load_configuration").unwrap().0, 0);
        assert_eq!(fixture.database.salsa_family_churn("parse_mdo_query").unwrap().0, 0);
    }
}

/// A standalone EPF/ERF needs no fake main configuration to resolve its own object metadata.
#[test]
fn standalone_external_owner_resolves_without_a_base() {
    for kind in
        [bsl_metadata::ExternalObjectKind::DataProcessor, bsl_metadata::ExternalObjectKind::Report]
    {
        let mut fixture = Fixture::new();
        let roots = fixture.roots(&[("external", RootKind::External(kind), &[])]);
        let root = &roots[0];
        fixture.file(root, 0, "Standalone.xml", None);
        fixture.file(root, 1, "Standalone/Ext/ObjectModule.bsl", Some(""));
        let slot = fixture.slot(Some(CanonicalMetadata::Object(object(
            kind.mdo_type(),
            "Standalone",
            "OwnField",
        ))));
        fixture.listing(
            root,
            MetadataListingData {
                entries: vec![MdoEntry {
                    kind: kind.mdo_type(),
                    name: "Standalone".into(),
                    main: FileId(0),
                    predefined: None,
                }],
                ..Default::default()
            },
            &[(0, slot)],
        );
        assert_eq!(fixture.module(1).mdo.as_ref().unwrap().mdo_type, kind.mdo_type());
        assert!(!fixture.database.workspace_configs_snapshot().has_base());
        assert_eq!(fixture.database.salsa_family_churn("load_configuration").unwrap().0, 0);
    }
}

/// Native collection names in the root path cannot change the kind of a bare external module.
#[test]
fn external_root_collection_names_never_change_object_module_classification() {
    for kind in
        [bsl_metadata::ExternalObjectKind::DataProcessor, bsl_metadata::ExternalObjectKind::Report]
    {
        for name in
            ["CommonModules", "HTTPServices", "WebServices", "IntegrationServices", "Commands"]
        {
            let mut fixture = Fixture::new();
            let roots = fixture.roots(&[(name, RootKind::External(kind), &[])]);
            let root = &roots[0];
            fixture.file(root, 0, "Owner.xml", None);
            fixture.file(root, 1, "Owner/Ext/ObjectModule.bsl", Some(""));
            let slot = fixture.slot(Some(CanonicalMetadata::Object(object(
                kind.mdo_type(),
                "Owner",
                "OwnField",
            ))));
            fixture.listing(
                root,
                MetadataListingData {
                    entries: vec![MdoEntry {
                        kind: kind.mdo_type(),
                        name: "Owner".into(),
                        main: FileId(0),
                        predefined: None,
                    }],
                    ..Default::default()
                },
                &[(0, slot)],
            );
            let module = fixture.module(1);
            assert_eq!(module.module_type, bsl_metadata::ModuleType::ObjectModule, "{name}");
            assert_eq!(module.mdo.as_ref().unwrap().mdo_type, kind.mdo_type(), "{name}");
        }
    }
}

/// Nested external form modules retain the current main-attribute type under the same canonical root.
#[test]
fn external_form_owner_type_uses_the_canonical_dialog_without_a_fake_configuration() {
    for kind in
        [bsl_metadata::ExternalObjectKind::DataProcessor, bsl_metadata::ExternalObjectKind::Report]
    {
        let mut fixture = Fixture::new();
        let roots = fixture.roots(&[("external", RootKind::External(kind), &[])]);
        let root = &roots[0];
        fixture.file(root, 0, "Owner.xml", None);
        fixture.file(root, 1, "Owner/Ext/ObjectModule.bsl", Some(""));
        fixture.file(root, 2, "Owner/Forms/Dialog.xml", None);
        fixture.file(root, 3, "Owner/Forms/Dialog/Ext/Form/Module.bsl", Some(""));
        let owner = fixture.slot(Some(CanonicalMetadata::Object(object(
            kind.mdo_type(),
            "Owner",
            "OwnField",
        ))));
        let mut form = bsl_metadata::Form::new(
            "Dialog".into(),
            bsl_metadata::FormType::Managed,
            bsl_metadata::Uuid::from_u128(2),
        );
        let main_type = AttributeType::Ref { mdo_type: kind.mdo_type(), name: "Owner".into() };
        let mut main = bsl_metadata::FormAttribute::new("Объект", main_type.clone());
        main.is_main = true;
        form.attributes.push(main);
        let form = Arc::new(form);
        let dialog = fixture.slot(Some(CanonicalMetadata::Form(Arc::clone(&form))));
        fixture.listing(
            root,
            MetadataListingData {
                entries: vec![MdoEntry {
                    kind: kind.mdo_type(),
                    name: "Owner".into(),
                    main: FileId(0),
                    predefined: None,
                }],
                ..Default::default()
            },
            &[(0, owner), (2, dialog)],
        );
        let module = fixture.module(3);
        assert!(Arc::ptr_eq(module.form.as_ref().unwrap(), &form));
        assert_eq!(module.form.as_ref().unwrap().main_attribute().unwrap().attr_type, main_type);
        assert_eq!(
            fixture
                .database
                .resolve_metadata_object_for_file(FileId(3), kind.mdo_type(), "Owner")
                .unwrap()
                .attributes[0]
                .name,
            "OwnField",
        );
        update(&mut fixture, dialog, None);
        assert!(fixture.module(3).form.is_none());
        assert!(fixture.module(1).mdo.is_some());
        assert_eq!(fixture.database.salsa_family_churn("load_configuration").unwrap().0, 0);
    }
}
