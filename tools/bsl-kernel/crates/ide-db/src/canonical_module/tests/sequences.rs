//! Current sequence ownership and module backlinks follow the same explicit visibility chain.

use super::*;
use crate::metadata::SequenceEntry;
use bsl_metadata::{AttributeType, ObjectBelonging, Sequence, SequenceDimension, Uuid};
use hir::DefDatabase;

/// Borrowed dimensions can be absent while the original sequence and its type remain current.
fn value(index: u128, original: u128) -> CanonicalMetadata {
    CanonicalMetadata::Sequence(Arc::new(Sequence::from_parts(
        Uuid::from_u128(index + 1),
        "Chain".into(),
        if index == 0 {
            vec![SequenceDimension {
                uuid: Uuid::from_u128(100),
                name: "KeyЯ".into(),
                attribute_type: Some(AttributeType::Boolean),
                belonging: ObjectBelonging::Own,
                extends: None,
            }]
        } else {
            Vec::new()
        },
        (index == 0).then(|| vec!["Orders".into()]),
        (index == 0).then(Vec::new),
        if index == 0 { ObjectBelonging::Own } else { ObjectBelonging::Adopted },
        (index != 0).then_some(Uuid::from_u128(original)),
    )))
}

/// A failed current dependency clears the selected record-set context without disturbing a hidden sibling.
#[test]
fn sequence_module_and_dimensions_share_current_file_scoped_composition() {
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
        fixture.file(root, id, "Sequences/Chain.xml", None);
        fixture.file(
            root,
            id + 1,
            "Sequences/Chain/Ext/RecordSetModule.bsl",
            Some("Procedure BeforeWrite(Cancel,Replace)\nEndProcedure\n"),
        );
        let slot = fixture.slot(Some(value(index as u128, 1)));
        slots.push(slot);
        fixture.listing(
            root,
            MetadataListingData {
                sequences: vec![SequenceEntry {
                    name: "Chain".into(),
                    main: FileId(id),
                    module_file: Some(FileId(id + 1)),
                }],
                ..Default::default()
            },
            &[(id, slot)],
        );
    }
    for file in [1, 3, 5, 7] {
        let current = fixture.database.resolve_sequence_for_file(FileId(file), "CHAIN").unwrap();
        assert_eq!(current.dimensions()[0].attribute_type, Some(AttributeType::Boolean));
        let metadata = fixture.database.module_metadata(hir::ModuleId::new(FileId(file)));
        assert_eq!(metadata.sequence.as_ref().unwrap(), &current);
        assert!(metadata.register.is_none() && metadata.mdo.is_none());
    }
    let sibling = fixture.database.resolve_sequence_for_file(FileId(7), "Chain");
    for bad in [None, Some(value(1, 99))] {
        update(&mut fixture, slots[1], bad);
        assert!(fixture.database.sequences_for_file(FileId(5)).is_err());
        assert!(fixture.database.module_metadata(hir::ModuleId::new(FileId(5))).sequence.is_none());
        assert_eq!(fixture.database.resolve_sequence_for_file(FileId(7), "Chain"), sibling);
    }
    update(&mut fixture, slots[1], Some(value(1, 1)));
    assert_eq!(fixture.database.sequences_for_file(FileId(5)).unwrap().len(), 1);
    assert_eq!(fixture.database.salsa_family_churn("load_configuration").unwrap().0, 0);
}
