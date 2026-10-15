//! Parameter targets inherit only through the current explicit visibility chain and exact original identity.

use super::*;
use crate::metadata::FunctionalOptionsParameterEntry;
use bsl_metadata::{
    FunctionalOptionParameterUse, FunctionalOptionsParameter, ObjectBelonging, Uuid,
};

/// A borrowed declaration may omit Use but cannot replace the original target set in this profile.
fn parameter(
    index: u128,
    original: u128,
    uses: Option<Vec<FunctionalOptionParameterUse>>,
) -> CanonicalMetadata {
    CanonicalMetadata::FunctionalOptionsParameter(Arc::new(FunctionalOptionsParameter::from_parts(
        Uuid::from_u128(index + 1),
        "Organization".into(),
        uses,
        if index == 0 { ObjectBelonging::Own } else { ObjectBelonging::Adopted },
        (index != 0).then_some(Uuid::from_u128(original)),
    )))
}

/// Equal paths in hidden roots cannot provide a current missing dependency contribution.
#[test]
fn functional_option_parameter_current_targets_and_failures_are_file_scoped() {
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
        fixture.file(root, id, "FunctionalOptionsParameters/Organization.xml", None);
        fixture.file(
            root,
            id + 1,
            "CommonModules/Tools/Ext/Module.bsl",
            Some("Procedure Run()\nEndProcedure"),
        );
        let uses = (index == 0)
            .then(|| vec![FunctionalOptionParameterUse::Catalog { name: "Items".into() }]);
        let slot = fixture.slot(Some(parameter(index as u128, 1, uses)));
        slots.push(slot);
        fixture.listing(
            root,
            MetadataListingData {
                functional_option_parameters: vec![FunctionalOptionsParameterEntry {
                    name: "Organization".into(),
                    main: FileId(id),
                }],
                ..Default::default()
            },
            &[(id, slot)],
        );
    }
    for file in [1, 3, 5, 7] {
        let value = fixture
            .database
            .resolve_functional_option_parameter_for_file(FileId(file), "ORGANIZATION")
            .unwrap();
        assert_eq!(
            value.uses().unwrap(),
            &[FunctionalOptionParameterUse::Catalog { name: "Items".into() }]
        );
    }
    let sibling =
        fixture.database.resolve_functional_option_parameter_for_file(FileId(7), "Organization");
    for bad in [
        None,
        Some(parameter(1, 99, None)),
        Some(parameter(1, 1, Some(Vec::new()))),
        Some(parameter(
            1,
            1,
            Some(vec![FunctionalOptionParameterUse::Catalog { name: "Other".into() }]),
        )),
    ] {
        update(&mut fixture, slots[1], bad);
        assert!(fixture.database.functional_option_parameters_for_file(FileId(5)).is_err());
        assert_eq!(
            fixture
                .database
                .resolve_functional_option_parameter_for_file(FileId(7), "Organization"),
            sibling
        );
    }
    update(&mut fixture, slots[1], Some(parameter(1, 1, None)));
    assert_eq!(fixture.database.functional_option_parameters_for_file(FileId(5)).unwrap().len(), 1);
    assert_eq!(fixture.database.salsa_family_churn("load_configuration").unwrap().0, 0);
}
