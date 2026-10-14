//! Per-file consumers distinguish an absent declaration from an unavailable current contribution.

use super::*;
use crate::metadata::{DefinedTypeEntry, MdoEntry};

/// Query files are real enrolled inputs; visibility never depends on a fabricated BSL body.
fn roots(fixture: &mut Fixture) -> Vec<PathBuf> {
    let roots = fixture.roots(&[
        ("base", RootKind::Base, &[]),
        ("dependency", RootKind::Extension, &[]),
        ("dependent", RootKind::Extension, &[1]),
        ("independent", RootKind::Extension, &[]),
    ]);
    for (index, root) in roots.iter().enumerate() {
        let id = u32::try_from(index).unwrap() * 2;
        fixture.file(root, id, "Descriptor.xml", None);
        fixture.file(root, id + 1, "Queries/actual.sdbl", Some("SELECT 1"));
    }
    roots
}

/// Base and adopted extension fields expose every required contribution in stable precedence.
fn object_overlay(index: usize, field: &str) -> CanonicalMetadata {
    let mut value = Arc::unwrap_or_clone(object(MdoType::Catalog, "Items", field));
    if index != 0 {
        value.set_uuid(bsl_metadata::Uuid::from_u128(index as u128 + 1));
        value.set_object_belonging(bsl_metadata::ObjectBelonging::Adopted);
        value.set_extends_uuid(bsl_metadata::Uuid::from_u128(1));
    }
    CanonicalMetadata::Object(Arc::new(value))
}

/// Direct metadata lookup must refuse missing current own, dependency and base payloads.
#[test]
fn canonical_per_file_object_resolution_refuses_unavailable_contributions() {
    let mut fixture = Fixture::new();
    let roots = roots(&mut fixture);
    let mut slots = Vec::new();
    for (index, root) in roots.iter().enumerate() {
        let id = u32::try_from(index).unwrap() * 2;
        let slot = fixture.slot(Some(object_overlay(index, &format!("Field{index}"))));
        slots.push(slot);
        fixture.listing(root, mdo_listing(MdoType::Catalog, "Items", id), &[(id, slot)]);
    }
    assert_eq!(object_fields(&fixture, 1), Some(vec!["Field0".into()]));
    assert_eq!(
        object_fields(&fixture, 5),
        Some(vec!["Field0".into(), "Field1".into(), "Field2".into()])
    );
    let independent = object_fields(&fixture, 7);
    for rejected in [
        None,
        Some(CanonicalMetadata::Object(object(MdoType::Catalog, "Other", "Wrong"))),
        Some(CanonicalMetadata::Object(object(MdoType::DataProcessor, "Items", "Wrong"))),
    ] {
        update(&mut fixture, slots[2], rejected);
        assert!(object_fields(&fixture, 5).is_none());
        assert_eq!(object_fields(&fixture, 7), independent);
    }
    update(&mut fixture, slots[2], Some(object_overlay(2, "Current")));
    update(&mut fixture, slots[1], None);
    assert!(object_fields(&fixture, 5).is_none());
    assert_eq!(object_fields(&fixture, 7), independent);
    update(&mut fixture, slots[1], Some(object_overlay(1, "Changed")));
    assert_eq!(
        object_fields(&fixture, 5),
        Some(vec!["Field0".into(), "Changed".into(), "Current".into()])
    );
    update(&mut fixture, slots[0], None);
    assert!(object_fields(&fixture, 5).is_none());
    assert!(object_fields(&fixture, 7).is_none());
    assert_eq!(fixture.database.salsa_family_churn("parse_mdo_query").unwrap().0, 0);
    assert_eq!(fixture.database.salsa_family_churn("load_configuration").unwrap().0, 0);
}

/// Register resources make both kind-aware and name-only lookup omissions observable.
#[test]
fn canonical_per_file_register_resolution_refuses_unavailable_contributions() {
    let mut fixture = Fixture::new();
    let roots = roots(&mut fixture);
    let mut slots = Vec::new();
    for (index, root) in roots.iter().enumerate() {
        let id = u32::try_from(index).unwrap() * 2;
        let slot = fixture.slot(Some(register("Rates", &format!("Field{index}"))));
        slots.push(slot);
        fixture.listing(
            root,
            mdo_listing(MdoType::InformationRegister, "Rates", id),
            &[(id, slot)],
        );
    }
    for by_name in [false, true] {
        assert_eq!(
            register_fields(&fixture, 5, by_name),
            Some(vec!["Field0".into(), "Field1".into(), "Field2".into()])
        );
    }
    for rejected in [None, Some(register("Other", "Wrong")), Some(object_overlay(1, "Wrong"))] {
        update(&mut fixture, slots[1], rejected);
        for by_name in [false, true] {
            assert!(register_fields(&fixture, 5, by_name).is_none());
            assert_eq!(
                register_fields(&fixture, 7, by_name),
                Some(vec!["Field0".into(), "Field3".into()])
            );
        }
    }
    update(&mut fixture, slots[1], Some(register("Rates", "Current")));
    for by_name in [false, true] {
        assert_eq!(
            register_fields(&fixture, 5, by_name),
            Some(vec!["Field0".into(), "Current".into(), "Field2".into()])
        );
    }
    update(&mut fixture, slots[2], None);
    for by_name in [false, true] {
        assert!(register_fields(&fixture, 5, by_name).is_none());
    }
    assert_eq!(fixture.database.salsa_family_churn("parse_register").unwrap().0, 0);
    assert_eq!(fixture.database.salsa_family_churn("load_configuration").unwrap().0, 0);
}

/// A declared replacement shadows its base even when invalid; a valid replacement needs no base value.
#[test]
fn canonical_per_file_defined_type_resolution_keeps_replacement_authority() {
    let mut fixture = Fixture::new();
    let roots = roots(&mut fixture);
    let mut slots = Vec::new();
    for (index, root) in roots.iter().enumerate() {
        let id = u32::try_from(index).unwrap() * 2;
        let slot = fixture.slot(Some(defined("Money", index as u32 + 10)));
        slots.push(slot);
        fixture.listing(
            root,
            MetadataListingData {
                defined_types: vec![DefinedTypeEntry { name: "Money".into(), main: FileId(id) }],
                ..Default::default()
            },
            &[(id, slot)],
        );
    }
    let resolve = |fixture: &Fixture, file| {
        fixture.database.resolve_defined_type_for_file(FileId(file), "MONEY")
    };
    assert_eq!(resolve(&fixture, 5), Some(AttributeType::String { length: Some(12) }));
    for rejected in [None, Some(defined("Other", 99)), Some(object_overlay(2, "Wrong"))] {
        update(&mut fixture, slots[2], rejected);
        assert!(resolve(&fixture, 5).is_none());
        assert_eq!(resolve(&fixture, 7), Some(AttributeType::String { length: Some(13) }));
    }
    update(&mut fixture, slots[2], Some(defined("Money", 40)));
    update(&mut fixture, slots[0], None);
    update(&mut fixture, slots[1], None);
    assert_eq!(resolve(&fixture, 5), Some(AttributeType::String { length: Some(40) }));
    assert_eq!(fixture.database.salsa_family_churn("parse_defined_type").unwrap().0, 0);
    assert_eq!(fixture.database.salsa_family_churn("load_configuration").unwrap().0, 0);
}

/// Each listing contains only the exact enrolled declaration being composed.
fn mdo_listing(kind: MdoType, name: &str, main: u32) -> MetadataListingData {
    MetadataListingData {
        entries: vec![MdoEntry { kind, name: name.into(), main: FileId(main), predefined: None }],
        ..Default::default()
    }
}

/// Caller identity and resource names are current metadata, independent of native XML.
fn register(name: &str, field: &str) -> CanonicalMetadata {
    CanonicalMetadata::Register(Arc::new(
        bsl_metadata::Register::builder()
            .name(name)
            .mdo_type(MdoType::InformationRegister)
            .add_resource(bsl_metadata::RegisterResource::new(
                bsl_metadata::Uuid::from_u128(1),
                field,
            ))
            .build(),
    ))
}

/// Replacement values retain the declared name and expose their exact underlying type.
fn defined(name: &str, length: u32) -> CanonicalMetadata {
    CanonicalMetadata::DefinedType(Arc::new(
        bsl_metadata::DefinedType::builder()
            .uuid(bsl_metadata::Uuid::from_u128(1))
            .name(name)
            .underlying_type(AttributeType::String { length: Some(length) })
            .build(),
    ))
}

/// Object fields reveal whether any unavailable canonical contribution was skipped.
fn object_fields(fixture: &Fixture, file: u32) -> Option<Vec<String>> {
    fixture
        .database
        .resolve_metadata_object_for_file(FileId(file), MdoType::Catalog, "ITEMS")
        .map(|value| value.attributes.iter().map(|field| field.name.to_string()).collect())
}

/// Both register entry points must produce the same current composed fields.
fn register_fields(fixture: &Fixture, file: u32, by_name: bool) -> Option<Vec<String>> {
    let value = if by_name {
        fixture.database.resolve_register_by_name_for_file(FileId(file), "RATES")
    } else {
        fixture.database.resolve_register_for_file(
            FileId(file),
            MdoType::InformationRegister,
            "RATES",
        )
    }?;
    Some(value.resources().iter().map(|field| field.name().to_string()).collect())
}
