//! Functional-option metadata inherits only through current visible UUID-proven contributions.

use super::*;
use crate::metadata::FunctionalOptionEntry;
use bsl_metadata::{FunctionalOption, FunctionalOptionLocation, ObjectBelonging, Uuid};

/// Contributions add interface membership without inventing a storage type or runtime Boolean value.
fn option(
    index: u128,
    original: u128,
    location: Option<FunctionalOptionLocation>,
) -> CanonicalMetadata {
    CanonicalMetadata::FunctionalOption(Arc::new(FunctionalOption::from_parts(
        Uuid::from_u128(index + 1),
        "Feature".into(),
        location,
        Some(true),
        Some(vec![format!("Catalog.Items{index}")]),
        if index == 0 { ObjectBelonging::Own } else { ObjectBelonging::Adopted },
        (index != 0).then_some(Uuid::from_u128(original)),
    )))
}

/// File scope comes from explicit topology rather than a same-name directory or source-root sibling.
fn setup() -> (Fixture, Vec<CanonicalMetadataInput>) {
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
        fixture.file(root, id, "FunctionalOptions/Feature.xml", None);
        fixture.file(
            root,
            id + 1,
            "CommonModules/Tools/Ext/Module.bsl",
            Some("Procedure Run()\nEndProcedure"),
        );
        let location =
            (index == 0).then_some(FunctionalOptionLocation::Constant { name: "Storage".into() });
        let slot = fixture.slot(Some(option(index as u128, 1, location)));
        slots.push(slot);
        fixture.listing(
            root,
            MetadataListingData {
                functional_options: vec![FunctionalOptionEntry {
                    name: "Feature".into(),
                    main: FileId(id),
                }],
                ..Default::default()
            },
            &[(id, slot)],
        );
    }
    (fixture, slots)
}

/// Wrong UUID, changed or explicitly empty borrowed storage cannot restore an old/base/sibling value.
#[test]
fn functional_option_current_visibility_storage_and_adoption_are_file_scoped() {
    let (mut fixture, slots) = setup();
    for (file, count) in [(1, 1), (3, 2), (5, 3), (7, 2)] {
        let value =
            fixture.database.resolve_functional_option_for_file(FileId(file), "FEATURE").unwrap();
        assert_eq!(value.content().unwrap().len(), count);
        assert_eq!(
            value.location(),
            Some(&FunctionalOptionLocation::Constant { name: "Storage".into() })
        );
    }
    let sibling = fixture.database.resolve_functional_option_for_file(FileId(7), "Feature");
    for bad in [
        None,
        Some(option(1, 99, None)),
        Some(option(1, 1, Some(FunctionalOptionLocation::Empty))),
        Some(option(1, 1, Some(FunctionalOptionLocation::Constant { name: "Other".into() }))),
    ] {
        update(&mut fixture, slots[1], bad);
        assert!(fixture.database.functional_option_for_file(FileId(5), "Feature").is_err());
        assert_eq!(
            fixture.database.resolve_functional_option_for_file(FileId(7), "Feature"),
            sibling
        );
    }
    update(&mut fixture, slots[1], Some(option(1, 1, None)));
    assert_eq!(
        fixture
            .database
            .resolve_functional_option_for_file(FileId(5), "Feature")
            .unwrap()
            .content()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(fixture.database.salsa_family_churn("load_configuration").unwrap().0, 0);
}
