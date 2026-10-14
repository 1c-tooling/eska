//! Canonical module metadata composes only checked declared dependencies and current own owners.

use super::*;
use crate::metadata::{CommonModuleEntry, MdoEntry};

/// A borrowed object's body inherits base fields and loses its owner when any required contribution fails.
#[test]
fn canonical_object_owner_composes_declared_overlays_without_incomplete_fallback() {
    let mut fixture = Fixture::new();
    let roots = fixture.roots(&[
        ("base", RootKind::Base, &[]),
        ("dependency", RootKind::Extension, &[]),
        ("dependent", RootKind::Extension, &[1]),
        ("independent", RootKind::Extension, &[]),
    ]);
    let mut slots = Vec::new();
    for (index, root) in roots.iter().enumerate() {
        let id = u32::try_from(index).unwrap() * 2;
        fixture.file(root, id, "Catalogs/Товары.xml", None);
        fixture.file(root, id + 1, "Catalogs/Товары/Ext/ObjectModule.bsl", Some(""));
        let value = overlay(index, &format!("Field{index}"));
        let slot = fixture.slot(Some(CanonicalMetadata::Object(value)));
        slots.push(slot);
        fixture.listing(
            root,
            MetadataListingData {
                entries: vec![MdoEntry {
                    kind: MdoType::Catalog,
                    name: "Товары".into(),
                    main: FileId(id),
                    predefined: None,
                }],
                ..Default::default()
            },
            &[(id, slot)],
        );
    }
    assert_eq!(fields(&fixture, 1), ["Field0"]);
    assert_eq!(fields(&fixture, 3), ["Field0", "Field1"]);
    assert_eq!(fields(&fixture, 5), ["Field0", "Field1", "Field2"]);
    assert_eq!(fields(&fixture, 7), ["Field0", "Field3"]);
    let base = fixture.module(1);
    let independent = fixture.module(7);
    update(&mut fixture, slots[2], None);
    assert!(fixture.module(5).mdo.is_none(), "own unavailable is never rescued by the base");
    assert!(Arc::ptr_eq(&base, &fixture.module(1)));
    assert!(Arc::ptr_eq(&independent, &fixture.module(7)));
    update(&mut fixture, slots[2], Some(CanonicalMetadata::Object(overlay(2, "Current"))));
    update(&mut fixture, slots[1], None);
    assert!(fixture.module(5).mdo.is_none(), "unavailable dependency cannot be silently skipped");
    assert!(Arc::ptr_eq(&independent, &fixture.module(7)));
    update(&mut fixture, slots[1], Some(CanonicalMetadata::Object(overlay(1, "Changed"))));
    assert_eq!(fields(&fixture, 5), ["Field0", "Changed", "Current"]);
    assert_eq!(fixture.database.salsa_family_churn("load_configuration").unwrap().0, 0);
}

/// Adopted UUIDs are actual identities; a same-name own object is not an inherited overlay.
fn overlay(index: usize, field: &str) -> Arc<MetadataObject> {
    let mut value = Arc::unwrap_or_clone(object(MdoType::Catalog, "Товары", field));
    if index != 0 {
        value.set_uuid(bsl_metadata::Uuid::from_u128(u128::try_from(index).unwrap() + 1));
        value.set_object_belonging(bsl_metadata::ObjectBelonging::Adopted);
        value.set_extends_uuid(bsl_metadata::Uuid::from_u128(1));
    }
    Arc::new(value)
}

/// Stable field order reveals forward composition and prevents a dependent from leaking into a sibling.
fn fields(fixture: &Fixture, file: u32) -> Vec<String> {
    fixture
        .module(file)
        .mdo
        .as_ref()
        .expect("checked current owner")
        .attributes
        .iter()
        .map(|field| field.name.to_string())
        .collect()
}

/// Common-module execution context uses inherited canonical flags and the current extension identity.
#[test]
fn canonical_common_module_owner_inherits_base_flags_under_its_own_uuid() {
    let mut fixture = Fixture::new();
    let roots =
        fixture.roots(&[("base", RootKind::Base, &[]), ("extension", RootKind::Extension, &[])]);
    let base = bsl_metadata::Uuid::from_u128(1);
    let extension = bsl_metadata::Uuid::from_u128(2);
    let mut slots = Vec::new();
    for (index, root) in roots.iter().enumerate() {
        let id = u32::try_from(index).unwrap() * 2;
        fixture.file(root, id, "CommonModules/Tools.xml", None);
        fixture.file(root, id + 1, "CommonModules/Tools/Ext/Module.bsl", Some(""));
        let owner = if index == 0 {
            bsl_metadata::CommonModule::builder().uuid(base).name("Tools").server(true).build()
        } else {
            bsl_metadata::CommonModule::builder()
                .uuid(extension)
                .name("Tools")
                .object_belonging(bsl_metadata::ObjectBelonging::Adopted)
                .extended_configuration_object(Some(base))
                .build()
        };
        let slot = fixture.slot(Some(CanonicalMetadata::CommonModule(Arc::new(owner))));
        slots.push(slot);
        fixture.listing(
            root,
            MetadataListingData {
                common_modules: vec![CommonModuleEntry {
                    name: "Tools".into(),
                    main: FileId(id),
                    module_file: Some(FileId(id + 1)),
                    unread_module_file: None,
                }],
                ..Default::default()
            },
            &[(id, slot)],
        );
    }
    use bsl_metadata::traits::MdObject;
    let module = fixture.module(3);
    let owner = module.common_module.as_ref().unwrap();
    assert_eq!(owner.uuid(), &extension);
    assert!(owner.is_server());
    assert_eq!(module.execution_context.as_ref().unwrap(), &hir::compute_execution_context(owner));
    let retained = fixture.module(1);
    update(&mut fixture, slots[1], None);
    let pending = fixture.module(3);
    assert!(pending.common_module.is_none());
    assert!(pending.execution_context.is_none());
    assert!(Arc::ptr_eq(&retained, &fixture.module(1)));
    assert_eq!(fixture.database.salsa_family_churn("load_configuration").unwrap().0, 0);
}

/// A missing declared dependency fails closed; a reopened fully enrolled composition resolves it.
#[test]
fn missing_dependency_listing_requires_a_fully_enrolled_reopened_database() {
    let mut fixture = Fixture::new();
    let roots =
        fixture.roots(&[("base", RootKind::Base, &[]), ("extension", RootKind::Extension, &[])]);
    for (index, root) in roots.iter().enumerate() {
        let id = u32::try_from(index).unwrap() * 2;
        fixture.file(root, id, "Catalogs/Товары.xml", None);
        fixture.file(root, id + 1, "Catalogs/Товары/Ext/ObjectModule.bsl", Some(""));
    }
    let owner = fixture.slot(Some(CanonicalMetadata::Object(overlay(1, "Current"))));
    fixture.listing(&roots[1], object_listing(2), &[(2, owner)]);
    assert!(fixture.module(3).mdo.is_none());
    assert_eq!(fixture.database.salsa_family_churn("load_configuration").unwrap().0, 0);

    let composition = fixture.database.workspace_configs_snapshot().as_ref().clone();
    fixture.database = RootDatabaseImpl::new_with_salsa_events();
    fixture.database.set_workspace_configs_snapshot(composition);
    fixture.database.set_source_root(SourceRootId(0), SourceRoot::new_local(fixture.files.clone()));
    for id in 0..4 {
        fixture.database.set_file_source_root(FileId(id), SourceRootId(0));
        fixture.database.set_file_unreadable(FileId(id));
    }
    for (index, root) in roots.iter().enumerate() {
        let main = u32::try_from(index).unwrap() * 2;
        let slot = fixture.slot(Some(CanonicalMetadata::Object(overlay(
            index,
            if index == 0 { "Base" } else { "Current" },
        ))));
        fixture.listing(root, object_listing(main), &[(main, slot)]);
    }
    assert_eq!(fields(&fixture, 3), ["Base", "Current"]);
    assert_eq!(fixture.database.salsa_family_churn("load_configuration").unwrap().0, 0);
}

/// The same declaration remains bound to its stable descriptor id in each reopened composition.
fn object_listing(main: u32) -> MetadataListingData {
    MetadataListingData {
        entries: vec![MdoEntry {
            kind: MdoType::Catalog,
            name: "Товары".into(),
            main: FileId(main),
            predefined: None,
        }],
        ..Default::default()
    }
}
