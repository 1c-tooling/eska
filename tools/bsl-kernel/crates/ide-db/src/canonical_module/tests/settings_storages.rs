//! Storage managers use current canonical identities and explicitly enrolled body backlinks.

use super::*;
use crate::metadata::SettingsStorageEntry;
use bsl_metadata::{ObjectBelonging, SettingsStorage, Uuid};
use hir::{BodySearch, ConfigsDatabase};

/// Each contribution retains the original storage UUID independently of its own descriptor UUID.
fn value(index: u128, original: u128) -> CanonicalMetadata {
    CanonicalMetadata::SettingsStorage(Arc::new(SettingsStorage::from_parts(
        Uuid::from_u128(index + 1),
        "PreferencesЯ".into(),
        if index == 0 { ObjectBelonging::Own } else { ObjectBelonging::Adopted },
        (index != 0).then_some(Uuid::from_u128(original)),
    )))
}

/// Four explicit roots enroll real body files without consulting a path-derived owner index.
fn composition_fixture() -> (Fixture, Vec<CanonicalMetadataInput>) {
    let mut fixture = Fixture::new();
    let roots = fixture.roots(&[
        ("base", RootKind::Base, &[]),
        ("dependency", RootKind::Extension, &[]),
        ("dependent", RootKind::Extension, &[1]),
        ("sibling", RootKind::Extension, &[]),
    ]);
    let mut slots = Vec::new();
    for (index, root) in roots.iter().enumerate() {
        let id = u32::try_from(index).unwrap() * 2;
        fixture.file(root, id, "SettingsStorages/PreferencesЯ.xml", None);
        fixture.file(
            root,
            id + 1,
            "SettingsStorages/PreferencesЯ/Ext/ManagerModule.bsl",
            Some("Function Current() Export\nReturn True;\nEndFunction\n"),
        );
        let slot = fixture.slot(Some(value(index as u128, 1)));
        slots.push(slot);
        fixture.listing(
            root,
            MetadataListingData {
                settings_storages: vec![SettingsStorageEntry {
                    name: "PreferencesЯ".into(),
                    main: FileId(id),
                    module_file: Some(FileId(id + 1)),
                }],
                ..Default::default()
            },
            &[(id, slot)],
        );
    }
    (fixture, slots)
}

/// A current owner controls both its module context and visible manager-body candidate surface.
#[test]
fn settings_storage_modules_and_bodies_share_current_scoped_composition() {
    let (mut fixture, slots) = composition_fixture();
    for (file, expected) in [(1, vec![1]), (3, vec![1, 3]), (5, vec![1, 3, 5]), (7, vec![1, 7])] {
        let owner =
            fixture.database.resolve_settings_storage_for_file(FileId(file), "PREFERENCESя");
        let module = fixture.module(file);
        assert_eq!(module.settings_storage, owner);
        assert!(module.settings_storage.is_some());
        assert!(module.mdo.is_none() && module.register.is_none());
        let bodies = fixture
            .database
            .resolve_settings_storage_manager_module_file_candidates(FileId(file), "PreferencesЯ")
            .unwrap();
        assert_eq!(
            bodies.all_for_reference().collect::<Vec<_>>(),
            expected.into_iter().map(FileId).collect::<Vec<_>>()
        );
    }
    let sibling = fixture.module(7).settings_storage.clone();
    for bad in [None, Some(value(1, 99))] {
        update(&mut fixture, slots[1], bad);
        assert!(fixture.database.settings_storages_for_file(FileId(5)).is_err());
        assert!(fixture.module(5).settings_storage.is_none());
        assert!(fixture
            .database
            .resolve_settings_storage_manager_module_file_candidates(FileId(5), "PreferencesЯ")
            .is_none());
        assert_eq!(fixture.module(7).settings_storage, sibling);
    }
    update(&mut fixture, slots[1], Some(value(1, 1)));
    fixture.database.set_file_unreadable(FileId(3));
    let bodies = fixture
        .database
        .resolve_settings_storage_manager_module_file_candidates(FileId(5), "PreferencesЯ")
        .unwrap();
    assert_eq!(bodies.unread().collect::<Vec<_>>(), [FileId(3)]);
    assert_eq!(bodies.search(|file| (file == FileId(5)).then_some(file)), BodySearch::Unread);
    fixture.database.set_file_text(FileId(3), "");
    let bodies = fixture
        .database
        .resolve_settings_storage_manager_module_file_candidates(FileId(5), "PreferencesЯ")
        .unwrap();
    assert_eq!(
        bodies.search(|file| (file == FileId(5)).then_some(file)),
        BodySearch::Found(FileId(5))
    );
    assert!(fixture.module(5).settings_storage.is_some());
    assert_eq!(fixture.database.salsa_family_churn("load_configuration").unwrap().0, 0);
}

/// Optional body absence is complete, while an unlisted lookalike never enters the manager surface.
#[test]
fn settings_storage_optional_module_never_falls_back_to_path_index() {
    let mut fixture = Fixture::new();
    let roots = fixture.roots(&[("base", RootKind::Base, &[])]);
    fixture.file(&roots[0], 0, "SettingsStorages/PreferencesЯ.xml", None);
    fixture.file(&roots[0], 1, "SettingsStorages/PreferencesЯ/Ext/ManagerModule.bsl", Some(""));
    let slot = fixture.slot(Some(value(0, 1)));
    fixture.listing(
        &roots[0],
        MetadataListingData {
            settings_storages: vec![SettingsStorageEntry {
                name: "PreferencesЯ".into(),
                main: FileId(0),
                module_file: None,
            }],
            ..Default::default()
        },
        &[(0, slot)],
    );
    for name in ["PreferencesЯ", "Absent"] {
        let bodies = fixture
            .database
            .resolve_settings_storage_manager_module_file_candidates(FileId(1), name)
            .unwrap();
        assert!(bodies.is_empty());
        assert!(!bodies.has_missing_expected_body());
        assert_eq!(bodies.search(Some), BodySearch::Absent);
    }
    assert!(fixture.module(1).settings_storage.is_none());
    assert_eq!(fixture.database.salsa_family_churn("load_configuration").unwrap().0, 0);
}
