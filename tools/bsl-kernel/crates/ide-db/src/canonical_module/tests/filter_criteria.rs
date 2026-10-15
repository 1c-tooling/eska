//! Named native criteria compose only current UUID-proven visible contributions.

use super::*;
use crate::metadata::FilterCriterionEntry;
use bsl_metadata::{FilterCriterion, FilterCriterionTarget, ObjectBelonging, Uuid};
use bsl_types::{builders::Builders, intern::TypeKernelDb, kind::TypeKind};

/// Search type inheritance is independent of added membership and generated manager spelling.
fn criterion(index: u128, original: u128, ty: Option<AttributeType>) -> CanonicalMetadata {
    CanonicalMetadata::FilterCriterion(Arc::new(FilterCriterion::from_parts(
        Uuid::from_u128(index + 1),
        "Linked".into(),
        ty,
        Some(vec![FilterCriterionTarget {
            kind: MdoType::Catalog,
            name: format!("Items{index}"),
            path: format!("Catalog.Items{index}.Attribute.Key"),
        }]),
        if index == 0 { ObjectBelonging::Own } else { ObjectBelonging::Adopted },
        (index != 0).then_some(Uuid::from_u128(original)),
    )))
}

/// The base, dependency, dependent and sibling use native topology rather than directory order.
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
        fixture.file(root, id, "FilterCriteria/Linked.xml", None);
        fixture.file(
            root,
            id + 1,
            "CommonModules/Tools/Ext/Module.bsl",
            Some("Procedure Run()\nEndProcedure"),
        );
        let slot = fixture.slot(Some(criterion(
            index as u128,
            1,
            (index == 0).then_some(AttributeType::Boolean),
        )));
        slots.push(slot);
        fixture.listing(
            root,
            MetadataListingData {
                filter_criteria: vec![FilterCriterionEntry {
                    name: "Linked".into(),
                    main: FileId(id),
                }],
                ..Default::default()
            },
            &[(id, slot)],
        );
    }
    (fixture, slots)
}

/// Real field lookup returns the native generated manager and retains original plus visible added content.
#[test]
fn filter_criterion_generated_manager_and_composition_are_file_scoped() {
    let (mut fixture, slots) = setup();
    let db = &fixture.database;
    for (file, count) in [(1, 1), (3, 2), (5, 3), (7, 2)] {
        let criterion = db.resolve_filter_criterion_for_file(FileId(file), "LINKED").unwrap();
        assert_eq!(criterion.content().unwrap().len(), count);
        assert_eq!(criterion.search_type(), Some(&AttributeType::Boolean));
    }
    let resolver = hir_ty::DbObjectResolver::new(db, FileId(5));
    let field = hir_ty::lookup_field(
        db,
        &resolver,
        db.platform_object("FilterCriteriaManager".into()),
        &hir::Name::new("Linked"),
    )
    .unwrap();
    assert!(
        matches!(db.lookup_type(field.ty), TypeKind::PlatformObject(facet) if facet.name == "FilterCriterionManager.Linked")
    );
    let sibling = db.resolve_filter_criterion_for_file(FileId(7), "Linked");
    for bad in [
        None,
        Some(criterion(1, 99, None)),
        Some(criterion(1, 1, Some(AttributeType::String { length: None }))),
    ] {
        update(&mut fixture, slots[1], bad);
        assert!(fixture.database.resolve_filter_criterion_for_file(FileId(5), "Linked").is_none());
        assert!(fixture.database.filter_criteria_for_file(FileId(5)).is_err());
        assert_eq!(
            fixture.database.resolve_filter_criterion_for_file(FileId(7), "Linked"),
            sibling
        );
    }
    update(&mut fixture, slots[1], Some(criterion(1, 1, None)));
    assert_eq!(
        fixture
            .database
            .resolve_filter_criterion_for_file(FileId(5), "Linked")
            .unwrap()
            .content()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(fixture.database.salsa_family_churn("load_configuration").unwrap().0, 0);
}

/// A declaration without its exact current slot cannot borrow an old XML or previously ready value.
#[test]
fn filter_criterion_pending_or_wrong_family_withdraws_the_native_namespace() {
    let (mut fixture, slots) = setup();
    for bad in [None, Some(CanonicalMetadata::Object(object(MdoType::Catalog, "Linked", "Key")))] {
        update(&mut fixture, slots[0], bad);
        let resolver = hir_ty::DbObjectResolver::new(&fixture.database, FileId(1));
        let receiver = fixture.database.platform_object("FilterCriteriaManager".into());
        assert!(hir_ty::lookup_field(
            &fixture.database,
            &resolver,
            receiver,
            &hir::Name::new("Linked")
        )
        .is_none());
        assert_eq!(fixture.database.filter_criteria_for_file(FileId(1)).unwrap_err(), FileId(0));
    }
}
