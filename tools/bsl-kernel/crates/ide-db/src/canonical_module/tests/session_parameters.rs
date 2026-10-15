//! Configured values preserve root identity and current input without whole-config loading.

use super::*;
use crate::metadata::SessionParameterEntry;
use bsl_metadata::{ObjectBelonging, SessionParameter, Uuid};
use bsl_types::{builders::Builders, intern::TypeKernelDb};

/// A configured current value has explicit ownership and a stable native source identity.
fn parameter(index: u128, length: u32) -> CanonicalMetadata {
    CanonicalMetadata::SessionParameter(Arc::new(SessionParameter::from_parts(
        Uuid::from_u128(index + 1),
        "CurrentUser".into(),
        AttributeType::String { length: Some(length) },
        if index == 0 { ObjectBelonging::Unknown } else { ObjectBelonging::Adopted },
        (index != 0).then_some(Uuid::from_u128(1)),
    )))
}

/// Shared canonical roots use explicit dependency closures rather than incidental file order.
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
        fixture.file(root, id, "SessionParameters/CurrentUser.xml", None);
        fixture.file(
            root,
            id + 1,
            "CommonModules/Tools/Ext/Module.bsl",
            Some("Procedure Run()\nEndProcedure"),
        );
        let slot = fixture.slot(Some(parameter(index as u128, id + 10)));
        slots.push(slot);
        fixture.listing(
            root,
            MetadataListingData {
                session_parameters: vec![SessionParameterEntry {
                    name: "CurrentUser".into(),
                    main: FileId(id),
                }],
                ..Default::default()
            },
            &[(id, slot)],
        );
    }
    (fixture, slots)
}

/// The static SessionParameters platform page has no configured members; real typed lookup must.
#[test]
fn session_parameter_bsl_container_exposes_current_writable_member() {
    let (fixture, _) = setup();
    let db = &fixture.database;
    let resolver = hir_ty::DbObjectResolver::new(db, FileId(5));
    for name in ["SessionParameters", "ПараметрыСеанса"] {
        let receiver = db.platform_object(name.into());
        let fields = hir_ty::enumerate_fields(db, &resolver, receiver);
        let field = fields
            .iter()
            .find(|field| field.name.as_str() == "CurrentUser")
            .expect("configured session member");
        assert!(
            !field.is_readonly,
            "the container is readonly, its configured values are writable"
        );
        assert!(matches!(db.lookup_type(field.ty), bsl_types::kind::TypeKind::String(_)));
        let found = hir_ty::lookup_field(db, &resolver, receiver, &hir::Name::new("CURRENTUSER"))
            .expect("same field lookup");
        assert_eq!(field, &found);
    }
    assert_eq!(db.salsa_family_churn("load_configuration").unwrap().0, 0);
}

/// Wrong UUIDs or unavailable visible contributions cannot reveal the previous base value.
#[test]
fn session_parameter_visibility_and_adoption_refuse_stale_or_sibling_values() {
    let (mut fixture, slots) = setup();
    let resolve = |fixture: &Fixture, file| {
        fixture.database.resolve_session_parameter_for_file(FileId(file), "CURRENTUSER")
    };
    assert_eq!(
        resolve(&fixture, 1).unwrap().attribute_type(),
        &AttributeType::String { length: Some(10) }
    );
    assert_eq!(
        resolve(&fixture, 5).unwrap().attribute_type(),
        &AttributeType::String { length: Some(14) }
    );
    let sibling = resolve(&fixture, 7);
    for refused in [
        None,
        Some(CanonicalMetadata::SessionParameter(Arc::new(SessionParameter::from_parts(
            Uuid::from_u128(3),
            "CurrentUser".into(),
            AttributeType::Boolean,
            ObjectBelonging::Adopted,
            Some(Uuid::from_u128(99)),
        )))),
    ] {
        update(&mut fixture, slots[1], refused);
        assert!(resolve(&fixture, 5).is_none());
        assert_eq!(resolve(&fixture, 7), sibling);
    }
    update(&mut fixture, slots[1], Some(parameter(1, 40)));
    update(&mut fixture, slots[2], None);
    assert!(resolve(&fixture, 5).is_none());
    update(&mut fixture, slots[2], Some(parameter(2, 50)));
    assert_eq!(
        resolve(&fixture, 5).unwrap().attribute_type(),
        &AttributeType::String { length: Some(50) }
    );
    assert_eq!(fixture.database.salsa_family_churn("load_configuration").unwrap().0, 0);
    assert_eq!(fixture.database.salsa_family_churn("parse_session_parameter").unwrap().0, 0);
}

/// An unrelated value/revision cannot invalidate a previously memoized typed member or its name index.
#[test]
fn session_parameter_inputs_invalidate_only_the_current_named_value() {
    let (mut fixture, slots) = setup();
    let db = &mut fixture.database;
    let root = db.workspace_configs_snapshot().paths[0].1.clone();
    let listing = db.metadata_listing(root.to_str().unwrap()).unwrap();
    let neighbor = CanonicalMetadataInput::new(
        db,
        Some(Arc::new(CanonicalMetadata::SessionParameter(Arc::new(
            SessionParameter::from_parts(
                Uuid::from_u128(9),
                "Neighbor".into(),
                AttributeType::Boolean,
                ObjectBelonging::Unknown,
                None,
            ),
        )))),
        "saved".into(),
    );
    listing.set_session_parameters(db).to(Arc::new(vec![
        SessionParameterEntry { name: "CurrentUser".into(), main: FileId(0) },
        SessionParameterEntry { name: "Neighbor".into(), main: FileId(10) },
    ]));
    listing
        .set_canonical_metadata(db)
        .to(Arc::new([(FileId(0), slots[0]), (FileId(10), neighbor)].into_iter().collect()));
    let original = db.resolve_session_parameter_for_file(FileId(1), "CurrentUser").unwrap();
    let index = Arc::clone(crate::metadata::session_parameter_index(db, listing));
    assert!(db.salsa_events_reset());
    neighbor.set_value(db).to(None);
    neighbor.set_source_revision(db).to("editor invalid".into());
    assert!(Arc::ptr_eq(
        &original,
        &db.resolve_session_parameter_for_file(FileId(1), "CurrentUser").unwrap()
    ));
    assert!(Arc::ptr_eq(&index, crate::metadata::session_parameter_index(db, listing)));
    assert_eq!(db.salsa_family_churn("resolve_session_parameter").unwrap().0, 0);
    slots[0].set_value(db).to(Some(Arc::new(parameter(0, 70))));
    slots[0].set_source_revision(db).to("editor current".into());
    assert_eq!(
        db.resolve_session_parameter_for_file(FileId(1), "CurrentUser").unwrap().attribute_type(),
        &AttributeType::String { length: Some(70) }
    );
    assert_eq!(db.salsa_family_churn("resolve_session_parameter").unwrap().0, 1);
}

/// A valid configured value in one CFE is invisible to the base and an unrelated sibling.
#[test]
fn session_parameter_extension_only_names_obey_dependency_visibility() {
    let (mut fixture, _) = setup();
    let db = &mut fixture.database;
    let root = db.workspace_configs_snapshot().paths[1].1.clone();
    let listing = db.metadata_listing(root.to_str().unwrap()).unwrap();
    let slot = CanonicalMetadataInput::new(
        db,
        Some(Arc::new(CanonicalMetadata::SessionParameter(Arc::new(
            SessionParameter::from_parts(
                Uuid::from_u128(11),
                "ExtensionOnly".into(),
                AttributeType::Boolean,
                ObjectBelonging::Own,
                None,
            ),
        )))),
        "saved".into(),
    );
    let mut declarations = listing.session_parameters(db).as_ref().clone();
    declarations.push(SessionParameterEntry { name: "ExtensionOnly".into(), main: FileId(10) });
    listing.set_session_parameters(db).to(Arc::new(declarations));
    let mut values = listing.canonical_metadata(db).as_ref().clone();
    values.insert(FileId(10), slot);
    listing.set_canonical_metadata(db).to(Arc::new(values));
    for (file, visible) in [(1, false), (3, true), (5, true), (7, false)] {
        assert_eq!(
            db.resolve_session_parameter_for_file(FileId(file), "ExtensionOnly").is_some(),
            visible
        );
        assert_eq!(
            db.session_parameter_names_for_file(FileId(file))
                .iter()
                .any(|name| name == "ExtensionOnly"),
            visible
        );
    }
    assert_eq!(db.salsa_family_churn("load_configuration").unwrap().0, 0);
}

/// Ambiguous names and unprovided canonical slots stay declared failures rather than base/XML fallbacks.
#[test]
fn session_parameter_conflicts_and_unprovided_slots_do_not_resurrect_base() {
    let (mut fixture, slots) = setup();
    let db = &mut fixture.database;
    let root = db.workspace_configs_snapshot().paths[1].1.clone();
    let listing = db.metadata_listing(root.to_str().unwrap()).unwrap();
    listing.set_session_parameters(db).to(Arc::new(vec![
        SessionParameterEntry { name: "CurrentUser".into(), main: FileId(2) },
        SessionParameterEntry { name: "CURRENTUSER".into(), main: FileId(10) },
    ]));
    assert!(db.resolve_session_parameter_for_file(FileId(5), "CurrentUser").is_none());
    listing
        .set_session_parameters(db)
        .to(Arc::new(vec![SessionParameterEntry { name: "CurrentUser".into(), main: FileId(2) }]));
    listing.set_canonical_metadata(db).to(Arc::new([(FileId(10), slots[1])].into_iter().collect()));
    assert!(db.resolve_session_parameter_for_file(FileId(5), "CurrentUser").is_none());
    assert_eq!(db.salsa_family_churn("parse_session_parameter").unwrap().0, 0);
    assert_eq!(db.salsa_family_churn("load_configuration").unwrap().0, 0);
}

/// Native producers discover this family explicitly and retain configured values in the cold model.
#[test]
fn session_parameter_native_discovery_and_directory_loader_preserve_type() {
    let mut fixture = Fixture::new();
    let roots = fixture.roots(&[("native", RootKind::Base, &[])]);
    let root = &roots[0];
    fs::create_dir(root.join("SessionParameters")).unwrap();
    fs::write(root.join("Configuration.xml"), r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20"><Configuration uuid="10000000-0000-4000-8000-000000000001"><Properties><Name>NativeFixture</Name></Properties></Configuration></MetaDataObject>"#).unwrap();
    fs::write(root.join("SessionParameters/CurrentUser.xml"), r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:v8="http://v8.1c.ru/8.1/data/core" xmlns:xs="http://www.w3.org/2001/XMLSchema" version="2.20"><SessionParameter uuid="20000000-0000-4000-8000-000000000001"><Properties><Name>CurrentUser</Name><Type><v8:Type>xs:boolean</v8:Type></Type></Properties></SessionParameter></MetaDataObject>"#).unwrap();
    let discovered =
        bsl_metadata::discover_session_parameter_structure(root, &bsl_conventions::RealFs);
    assert_eq!(discovered.len(), 1);
    assert_eq!(discovered[0].name, "CurrentUser");
    let config = bsl_metadata::load_from_directory(root).unwrap();
    assert_eq!(config.session_parameters().len(), 1);
    assert_eq!(
        config.find_session_parameter("CURRENTUSER").unwrap().attribute_type(),
        &AttributeType::Boolean
    );
}
