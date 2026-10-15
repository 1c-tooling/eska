//! Native command composition rejects required current failures before type inference can use a base.

use super::*;
use bsl_metadata::{CommandParameterUseMode, CommonCommand, ObjectBelonging};

/// The base supplies type/use while adopted declarations may omit either property independently.
fn command(index: usize, original: u128) -> CanonicalMetadata {
    CanonicalMetadata::CommonCommand(Arc::new(CommonCommand::from_parts(
        bsl_metadata::Uuid::from_u128(index as u128 + 1),
        "Run".into(),
        match index {
            0 => Some(AttributeType::String { length: None }),
            2 => Some(AttributeType::Boolean),
            _ => None,
        },
        match index {
            0 => Some(CommandParameterUseMode::Single),
            1 => Some(CommandParameterUseMode::Multiple),
            _ => None,
        },
        if index == 0 { ObjectBelonging::Own } else { ObjectBelonging::Adopted },
        (index != 0).then_some(bsl_metadata::Uuid::from_u128(original)),
    )))
}

/// Base, dependencies and selected CFE compose in order; a hidden pending namesake stays excluded.
#[test]
fn common_command_owner_composes_current_uuid_proven_parameters_without_hidden_fallback() {
    let mut fixture = Fixture::new();
    let roots = fixture.roots(&[
        ("base", RootKind::Base, &[]),
        ("dependency", RootKind::Extension, &[]),
        ("selected", RootKind::Extension, &[1]),
        ("hidden", RootKind::Extension, &[]),
    ]);
    let mut slots = Vec::new();
    for (index, root) in roots.iter().enumerate() {
        let id = u32::try_from(index).unwrap() * 2;
        fixture.file(root, id, "CommonCommands/Run.xml", None);
        fixture.file(root, id + 1, "CommonCommands/Run/Ext/CommandModule.bsl", Some(""));
        let slot = fixture.slot((index != 3).then(|| command(index, 1)));
        slots.push(slot);
        fixture.listing(
            root,
            MetadataListingData {
                common_commands: vec![crate::metadata::CommonCommandEntry {
                    name: "Run".into(),
                    main: FileId(id),
                    module_file: Some(FileId(id + 1)),
                }],
                ..Default::default()
            },
            &[(id, slot)],
        );
    }
    let current = fixture.module(5);
    let value = current.common_command.as_ref().unwrap();
    assert_eq!(value.uuid(), bsl_metadata::Uuid::from_u128(3));
    assert_eq!(value.parameter_type(), Some(&AttributeType::Boolean));
    assert_eq!(value.parameter_use(), Some(CommandParameterUseMode::Multiple));
    let base = fixture.module(1);
    update(&mut fixture, slots[1], None);
    assert_eq!(fixture.database.common_command_for_file(FileId(5)), Err(FileId(2)));
    assert!(fixture.module(5).common_command.is_none());
    assert!(Arc::ptr_eq(&base, &fixture.module(1)));
    update(&mut fixture, slots[1], Some(command(1, 1)));
    update(&mut fixture, slots[2], Some(command(2, 999)));
    assert_eq!(fixture.database.common_command_for_file(FileId(5)), Err(FileId(4)));
    update(&mut fixture, slots[2], Some(command(2, 1)));
    assert!(fixture.module(5).common_command.is_some());
    update(&mut fixture, slots[0], None);
    assert_eq!(fixture.database.common_command_for_file(FileId(5)), Err(FileId(0)));
    assert_eq!(fixture.database.salsa_family_churn("load_configuration").unwrap().0, 0);
}
