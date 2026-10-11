//! Regression tests for canonical metadata families; no XML file is registered.

use std::sync::Arc;

use bsl_metadata::{AttributeType, CommonModule, DefinedType, MdoType, Register};
use salsa::Setter;
use vfs::FileId;

use crate::{
    metadata::{
        common_module_index, config_index, defined_type_index, event_subscription_index,
        http_service_index, resolve_common_module, resolve_common_module_by_file,
        resolve_defined_type, resolve_event_subscription, resolve_http_service,
        resolve_http_service_by_file, resolve_register, resolve_register_by_name,
        resolve_scheduled_job, resolve_subsystem, scheduled_job_index, subsystem_index,
        CanonicalMetadata, CanonicalMetadataInput, CanonicalMetadataInputs, CommonModuleEntry,
        DefinedTypeEntry, EventSubscriptionEntry, HTTPServiceEntry, MdoEntry, MetadataListingData,
        MetadataListingInput, ScheduledJobEntry, SubsystemEntry,
    },
    RootDatabaseImpl,
};

/// Register only structural identities; canonical slots are the sole data providers.
fn database() -> (RootDatabaseImpl, MetadataListingInput) {
    let mut database = RootDatabaseImpl::new_with_salsa_events();
    database.set_metadata_listing(
        "/canonical-families",
        MetadataListingData {
            entries: vec![MdoEntry {
                kind: MdoType::InformationRegister,
                name: "Rates".into(),
                main: FileId(0),
                predefined: None,
            }],
            common_modules: vec![CommonModuleEntry {
                name: "Tools".into(),
                main: FileId(1),
                module_file: Some(FileId(3)),
                unread_module_file: None,
            }],
            defined_types: vec![DefinedTypeEntry { name: "Money".into(), main: FileId(2) }],
            ..MetadataListingData::default()
        },
    );
    let listing = database.metadata_listing("/canonical-families").expect("registered listing");
    (database, listing)
}

/// Bind one stable file identity to a host-provided tracked value.
fn input(database: &RootDatabaseImpl, value: CanonicalMetadata) -> CanonicalMetadataInput {
    CanonicalMetadataInput::new(database, Some(Arc::new(value)), "saved".into())
}

/// Current subscription bindings replace only their own slot and never read an XML fallback.
#[test]
fn canonical_event_subscription_preserves_neighbors_and_rejects_unavailable_or_wrong_identity() {
    let (mut database, listing) = database();
    listing.set_event_subscriptions(&mut database).to(Arc::new(vec![
        EventSubscriptionEntry { name: "Подписка".into(), main: FileId(4) },
        EventSubscriptionEntry { name: "Neighbor".into(), main: FileId(5) },
    ]));
    let make = |name: &str, handler: &str| {
        Arc::new(bsl_metadata::EventSubscription::from_parts(
            bsl_metadata::Uuid::nil(),
            name.into(),
            None,
            "cfg:CatalogObject.Items".into(),
            "BeforeWrite".into(),
            handler.into(),
        ))
    };
    let saved = make("Подписка", "CommonModule.Tools.Saved");
    let neighbor = make("Neighbor", "CommonModule.Tools.Other");
    let provided = input(&database, CanonicalMetadata::EventSubscription(Arc::clone(&saved)));
    let untouched = input(&database, CanonicalMetadata::EventSubscription(Arc::clone(&neighbor)));
    listing
        .set_canonical_metadata(&mut database)
        .to(Arc::new([(FileId(4), provided), (FileId(5), untouched)].into_iter().collect()));
    let index = Arc::clone(event_subscription_index(&database, listing));
    assert!(Arc::ptr_eq(
        &saved,
        &resolve_event_subscription(&database, listing, "ПОДПИСКА".into()).expect("saved")
    ));
    assert!(Arc::ptr_eq(
        &neighbor,
        &resolve_event_subscription(&database, listing, "Neighbor".into()).expect("neighbor")
    ));
    assert!(database.salsa_events_reset());
    let edited = make("Подписка", "CommonModule.Tools.Current");
    provided
        .set_value(&mut database)
        .to(Some(Arc::new(CanonicalMetadata::EventSubscription(Arc::clone(&edited)))));
    provided.set_source_revision(&mut database).to("editor".into());
    assert!(Arc::ptr_eq(
        &edited,
        &resolve_event_subscription(&database, listing, "подписка".into()).expect("edited")
    ));
    assert!(Arc::ptr_eq(
        &neighbor,
        &resolve_event_subscription(&database, listing, "Neighbor".into())
            .expect("unchanged neighbor")
    ));
    assert_eq!(database.salsa_family_churn("resolve_event_subscription").expect("events").0, 1);
    assert!(Arc::ptr_eq(&index, event_subscription_index(&database, listing)));
    provided.set_value(&mut database).to(None);
    assert!(resolve_event_subscription(&database, listing, "Подписка".into()).is_none());
    provided.set_value(&mut database).to(Some(Arc::new(CanonicalMetadata::EventSubscription(
        make("Other", "CommonModule.Tools.Wrong"),
    ))));
    assert!(resolve_event_subscription(&database, listing, "Подписка".into()).is_none());
    provided.set_value(&mut database).to(Some(Arc::new(CanonicalMetadata::CommonModule(
        Arc::new(CommonModule::builder().name("Подписка").build()),
    ))));
    assert!(resolve_event_subscription(&database, listing, "Подписка".into()).is_none());
    assert_eq!(database.salsa_family_churn("parse_event_subscription_query").expect("events").0, 0);
}

/// A job callback and its flags publish one current value without recomputing an unrelated job.
#[test]
fn canonical_scheduled_job_preserves_neighbors_and_rejects_unavailable_or_wrong_identity() {
    let (mut database, listing) = database();
    listing.set_scheduled_jobs(&mut database).to(Arc::new(vec![
        ScheduledJobEntry { name: "Задание".into(), main: FileId(6) },
        ScheduledJobEntry { name: "Neighbor".into(), main: FileId(7) },
    ]));
    let make = |name: &str, method: &str, predefined, enabled| {
        Arc::new(bsl_metadata::ScheduledJob::from_parts(
            bsl_metadata::Uuid::nil(),
            name.into(),
            method.into(),
            predefined,
            enabled,
        ))
    };
    let saved = make("Задание", "CommonModule.Tools.Saved", false, true);
    let neighbor = make("Neighbor", "CommonModule.Tools.Other", false, true);
    let provided = input(&database, CanonicalMetadata::ScheduledJob(Arc::clone(&saved)));
    let untouched = input(&database, CanonicalMetadata::ScheduledJob(Arc::clone(&neighbor)));
    listing
        .set_canonical_metadata(&mut database)
        .to(Arc::new([(FileId(6), provided), (FileId(7), untouched)].into_iter().collect()));
    let index = Arc::clone(scheduled_job_index(&database, listing));
    assert!(Arc::ptr_eq(
        &saved,
        &resolve_scheduled_job(&database, listing, "ЗАДАНИЕ".into()).expect("saved")
    ));
    assert!(Arc::ptr_eq(
        &neighbor,
        &resolve_scheduled_job(&database, listing, "Neighbor".into()).expect("neighbor")
    ));
    assert!(database.salsa_events_reset());
    let current = make("Задание", "CommonModule.Tools.Current", true, false);
    provided
        .set_value(&mut database)
        .to(Some(Arc::new(CanonicalMetadata::ScheduledJob(Arc::clone(&current)))));
    provided.set_source_revision(&mut database).to("editor".into());
    assert!(Arc::ptr_eq(
        &current,
        &resolve_scheduled_job(&database, listing, "задание".into()).expect("current")
    ));
    assert!(Arc::ptr_eq(
        &neighbor,
        &resolve_scheduled_job(&database, listing, "Neighbor".into()).expect("unchanged neighbor")
    ));
    assert_eq!(database.salsa_family_churn("resolve_scheduled_job").expect("events").0, 1);
    assert!(Arc::ptr_eq(&index, scheduled_job_index(&database, listing)));
    provided.set_value(&mut database).to(None);
    assert!(resolve_scheduled_job(&database, listing, "Задание".into()).is_none());
    provided.set_value(&mut database).to(Some(Arc::new(CanonicalMetadata::ScheduledJob(make(
        "Other",
        "CommonModule.Tools.Wrong",
        true,
        false,
    )))));
    assert!(resolve_scheduled_job(&database, listing, "Задание".into()).is_none());
    provided.set_value(&mut database).to(Some(Arc::new(CanonicalMetadata::CommonModule(
        Arc::new(CommonModule::builder().name("Задание").build()),
    ))));
    assert!(resolve_scheduled_job(&database, listing, "Задание".into()).is_none());
    assert_eq!(database.salsa_family_churn("parse_scheduled_job_query").expect("events").0, 0);
}

/// Subsystem members and child names publish one current value without recomputing a neighbor.
#[test]
fn canonical_subsystem_preserves_neighbors_and_rejects_unavailable_or_wrong_identity() {
    let (mut database, listing) = database();
    listing.set_subsystems(&mut database).to(Arc::new(vec![
        SubsystemEntry { name: "Подсистема".into(), main: FileId(8) },
        SubsystemEntry { name: "Neighbor".into(), main: FileId(9) },
    ]));
    let make = |name: &str, member: &str, child: &str| {
        Arc::new(
            bsl_metadata::Subsystem::new(name)
                .with_content(vec![(MdoType::Catalog, member.into())])
                .with_child_subsystems(vec![child.into()]),
        )
    };
    let saved = make("Подсистема", "Saved", "First");
    let neighbor = make("Neighbor", "Other", "Untouched");
    let provided = input(&database, CanonicalMetadata::Subsystem(Arc::clone(&saved)));
    let untouched = input(&database, CanonicalMetadata::Subsystem(Arc::clone(&neighbor)));
    listing
        .set_canonical_metadata(&mut database)
        .to(Arc::new([(FileId(8), provided), (FileId(9), untouched)].into_iter().collect()));
    let index = Arc::clone(subsystem_index(&database, listing));
    assert!(Arc::ptr_eq(
        &saved,
        &resolve_subsystem(&database, listing, "ПОДСИСТЕМА".into()).expect("saved")
    ));
    assert!(Arc::ptr_eq(
        &neighbor,
        &resolve_subsystem(&database, listing, "Neighbor".into()).expect("neighbor")
    ));
    assert!(database.salsa_events_reset());
    let current = make("Подсистема", "Current", "Edited");
    provided
        .set_value(&mut database)
        .to(Some(Arc::new(CanonicalMetadata::Subsystem(Arc::clone(&current)))));
    provided.set_source_revision(&mut database).to("editor".into());
    assert!(Arc::ptr_eq(
        &current,
        &resolve_subsystem(&database, listing, "подсистема".into()).expect("current")
    ));
    assert!(Arc::ptr_eq(
        &neighbor,
        &resolve_subsystem(&database, listing, "Neighbor".into()).expect("unchanged neighbor")
    ));
    assert_eq!(database.salsa_family_churn("resolve_subsystem").expect("events").0, 1);
    assert!(Arc::ptr_eq(&index, subsystem_index(&database, listing)));
    provided.set_value(&mut database).to(None);
    assert!(resolve_subsystem(&database, listing, "Подсистема".into()).is_none());
    provided
        .set_value(&mut database)
        .to(Some(Arc::new(CanonicalMetadata::Subsystem(make("Other", "Wrong", "Wrong")))));
    assert!(resolve_subsystem(&database, listing, "Подсистема".into()).is_none());
    provided.set_value(&mut database).to(Some(Arc::new(CanonicalMetadata::CommonModule(
        Arc::new(CommonModule::builder().name("Подсистема").build()),
    ))));
    assert!(resolve_subsystem(&database, listing, "Подсистема".into()).is_none());
    assert_eq!(database.salsa_family_churn("parse_subsystem_query").expect("events").0, 0);
}

/// Name and module-file resolvers share one current service while a neighbor remains memoised.
#[test]
fn canonical_http_service_is_current_by_name_and_module_without_xml_fallback() {
    let (mut database, listing) = database();
    listing.set_http_services(&mut database).to(Arc::new(vec![
        HTTPServiceEntry {
            name: "Сервис".into(), main: FileId(10), module_file: Some(FileId(12))
        },
        HTTPServiceEntry {
            name: "Neighbor".into(),
            main: FileId(11),
            module_file: Some(FileId(13)),
        },
    ]));
    let make = |name: &str, root: &str, route: &str, handler: &str| {
        Arc::new(
            bsl_metadata::HTTPServiceBuilder::new()
                .name(name)
                .root_url(root)
                .uri(format!("HTTPServices/{name}/Ext/Module.bsl"))
                .add_url_template(
                    bsl_metadata::HTTPServiceURLTemplateBuilder::new()
                        .name("Route")
                        .template(route)
                        .add_method(
                            bsl_metadata::HTTPServiceMethodBuilder::new()
                                .name("Get")
                                .http_method("GET")
                                .handler(handler)
                                .build(),
                        )
                        .build(),
                )
                .build(),
        )
    };
    let saved = make("Сервис", "saved", "/saved", "Saved");
    let neighbor = make("Neighbor", "neighbor", "/untouched", "Other");
    let provided = input(&database, CanonicalMetadata::HTTPService(Arc::clone(&saved)));
    let untouched = input(&database, CanonicalMetadata::HTTPService(Arc::clone(&neighbor)));
    listing
        .set_canonical_metadata(&mut database)
        .to(Arc::new([(FileId(10), provided), (FileId(11), untouched)].into_iter().collect()));
    let index = Arc::clone(http_service_index(&database, listing));
    assert!(Arc::ptr_eq(
        &saved,
        &resolve_http_service(&database, listing, "СЕРВИС".into()).expect("saved")
    ));
    assert!(Arc::ptr_eq(
        &saved,
        &resolve_http_service_by_file(&database, listing, FileId(12)).expect("saved by file")
    ));
    assert!(Arc::ptr_eq(
        &neighbor,
        &resolve_http_service_by_file(&database, listing, FileId(13)).expect("neighbor")
    ));
    assert!(database.salsa_events_reset());
    let current = make("Сервис", "current", "/edited", "Current");
    provided
        .set_value(&mut database)
        .to(Some(Arc::new(CanonicalMetadata::HTTPService(Arc::clone(&current)))));
    provided.set_source_revision(&mut database).to("editor".into());
    assert!(Arc::ptr_eq(
        &current,
        &resolve_http_service_by_file(&database, listing, FileId(12)).expect("current by file")
    ));
    assert!(Arc::ptr_eq(
        &neighbor,
        &resolve_http_service_by_file(&database, listing, FileId(13)).expect("unchanged neighbor")
    ));
    assert_eq!(database.salsa_family_churn("resolve_http_service").expect("events").0, 1);
    assert!(Arc::ptr_eq(&index, http_service_index(&database, listing)));
    provided.set_value(&mut database).to(None);
    assert!(resolve_http_service_by_file(&database, listing, FileId(12)).is_none());
    provided.set_value(&mut database).to(Some(Arc::new(CanonicalMetadata::HTTPService(make(
        "Other", "wrong", "/wrong", "Wrong",
    )))));
    assert!(resolve_http_service(&database, listing, "сервис".into()).is_none());
    provided.set_value(&mut database).to(Some(Arc::new(CanonicalMetadata::CommonModule(
        Arc::new(CommonModule::builder().name("Сервис").build()),
    ))));
    assert!(resolve_http_service_by_file(&database, listing, FileId(12)).is_none());
    assert_eq!(database.salsa_family_churn("parse_http_service_query").expect("events").0, 0);
}

/// Supply typed register semantics directly, not through an XML-shaped VFS string.
#[test]
fn canonical_register_uses_both_entry_points_and_never_falls_back_when_unavailable() {
    let (mut database, listing) = database();
    let register = Arc::new(
        Register::builder()
            .name("Rates")
            .mdo_type(MdoType::InformationRegister)
            .recorder_subordinate(false)
            .build(),
    );
    let provided = input(&database, CanonicalMetadata::Register(Arc::clone(&register)));
    let inputs: CanonicalMetadataInputs = [(FileId(0), provided)].into_iter().collect();
    listing.set_canonical_metadata(&mut database).to(Arc::new(inputs));
    let index = Arc::clone(config_index(&database, listing));
    assert!(Arc::ptr_eq(
        &register,
        &resolve_register(&database, listing, MdoType::InformationRegister, "rates".into())
            .expect("by kind")
    ));
    assert!(Arc::ptr_eq(
        &register,
        &resolve_register_by_name(&database, listing, "RATES".into()).expect("by name")
    ));
    let changed = Arc::new(
        Register::builder()
            .name("Rates")
            .mdo_type(MdoType::InformationRegister)
            .recorder_subordinate(true)
            .build(),
    );
    provided.set_value(&mut database).to(Some(Arc::new(CanonicalMetadata::Register(changed))));
    provided.set_source_revision(&mut database).to("draft".into());
    assert!(resolve_register_by_name(&database, listing, "Rates".into())
        .expect("changed register")
        .is_recorder_subordinate());
    assert!(!register.is_recorder_subordinate());
    assert!(Arc::ptr_eq(&index, config_index(&database, listing)));
    provided.set_value(&mut database).to(None);
    assert!(resolve_register(&database, listing, MdoType::InformationRegister, "Rates".into())
        .is_none());
    assert!(resolve_register_by_name(&database, listing, "Rates".into()).is_none());
    assert_eq!(database.salsa_family_churn("parse_register_query").expect("events").0, 0);
}

/// Both common-module name lookup and the BSL-body backlink use the same flags and input.
#[test]
fn canonical_common_module_by_name_and_body_share_current_flags() {
    let (mut database, listing) = database();
    let module =
        Arc::new(CommonModule::builder().name("Tools").server(true).server_call(false).build());
    let provided = input(&database, CanonicalMetadata::CommonModule(Arc::clone(&module)));
    let inputs: CanonicalMetadataInputs = [(FileId(1), provided)].into_iter().collect();
    listing.set_canonical_metadata(&mut database).to(Arc::new(inputs));
    let index = Arc::clone(common_module_index(&database, listing));
    assert!(Arc::ptr_eq(
        &module,
        &resolve_common_module(&database, listing, "TOOLS".into()).expect("by name")
    ));
    assert!(Arc::ptr_eq(
        &module,
        &resolve_common_module_by_file(&database, listing, FileId(3)).expect("by body")
    ));
    let changed =
        Arc::new(CommonModule::builder().name("Tools").server(true).server_call(true).build());
    provided.set_value(&mut database).to(Some(Arc::new(CanonicalMetadata::CommonModule(changed))));
    provided.set_source_revision(&mut database).to("draft".into());
    assert!(resolve_common_module_by_file(&database, listing, FileId(3))
        .expect("changed flags")
        .is_server_call());
    assert!(!module.is_server_call());
    assert!(Arc::ptr_eq(&index, common_module_index(&database, listing)));
    provided.set_value(&mut database).to(None);
    assert!(resolve_common_module(&database, listing, "Tools".into()).is_none());
    assert!(resolve_common_module_by_file(&database, listing, FileId(3)).is_none());
    assert_eq!(database.salsa_family_churn("parse_common_module_query").expect("events").0, 0);
}

/// A changed defined type replaces its underlying type without reparsing XML or the structure.
#[test]
fn canonical_defined_type_replaces_composite_type_at_one_slot() {
    let (mut database, listing) = database();
    let value = Arc::new(
        DefinedType::builder()
            .uuid("30000000-0000-4000-8000-000000000001".parse().expect("fixed test UUID"))
            .name("Money")
            .underlying_type(AttributeType::Number { precision: 15, scale: 2 })
            .build(),
    );
    let provided = input(&database, CanonicalMetadata::DefinedType(value));
    let inputs: CanonicalMetadataInputs = [(FileId(2), provided)].into_iter().collect();
    listing.set_canonical_metadata(&mut database).to(Arc::new(inputs));
    let index = Arc::clone(defined_type_index(&database, listing));
    let saved = resolve_defined_type(&database, listing, "money".into()).expect("saved type");
    assert_eq!(saved.as_ref(), &AttributeType::Number { precision: 15, scale: 2 });
    let draft = AttributeType::Composite {
        types: vec![AttributeType::Boolean, AttributeType::String { length: Some(20) }],
    };
    let changed = Arc::new(
        DefinedType::builder()
            .uuid("30000000-0000-4000-8000-000000000001".parse().expect("fixed test UUID"))
            .name("Money")
            .underlying_type(draft.clone())
            .build(),
    );
    provided.set_value(&mut database).to(Some(Arc::new(CanonicalMetadata::DefinedType(changed))));
    provided.set_source_revision(&mut database).to("draft".into());
    assert_eq!(
        resolve_defined_type(&database, listing, "Money".into()).expect("changed type").as_ref(),
        &draft
    );
    assert_eq!(saved.as_ref(), &AttributeType::Number { precision: 15, scale: 2 });
    assert!(Arc::ptr_eq(&index, defined_type_index(&database, listing)));
    provided.set_value(&mut database).to(None);
    assert!(resolve_defined_type(&database, listing, "Money".into()).is_none());
    assert_eq!(database.salsa_family_churn("parse_defined_type_query").expect("events").0, 0);
}

/// A provider cannot silently supply a different family, structural name or register kind.
#[test]
fn canonical_family_and_identity_mismatches_are_unavailable() {
    let (mut database, listing) = database();
    let wrong_name = input(
        &database,
        CanonicalMetadata::Register(Arc::new(
            Register::builder().name("Other").mdo_type(MdoType::InformationRegister).build(),
        )),
    );
    let wrong_family = input(
        &database,
        CanonicalMetadata::DefinedType(Arc::new(
            DefinedType::builder()
                .uuid("30000000-0000-4000-8000-000000000001".parse().expect("fixed test UUID"))
                .name("Tools")
                .underlying_type(AttributeType::Boolean)
                .build(),
        )),
    );
    let inputs: CanonicalMetadataInputs =
        [(FileId(0), wrong_name), (FileId(1), wrong_family), (FileId(2), wrong_family)]
            .into_iter()
            .collect();
    listing.set_canonical_metadata(&mut database).to(Arc::new(inputs));
    assert!(resolve_register_by_name(&database, listing, "Rates".into()).is_none());
    assert!(resolve_common_module(&database, listing, "Tools".into()).is_none());
    assert!(resolve_defined_type(&database, listing, "Money".into()).is_none());
    wrong_name.set_value(&mut database).to(Some(Arc::new(CanonicalMetadata::Register(Arc::new(
        Register::builder().name("Rates").mdo_type(MdoType::AccumulationRegister).build(),
    )))));
    assert!(resolve_register(&database, listing, MdoType::InformationRegister, "Rates".into())
        .is_none());
    assert!(resolve_register_by_name(&database, listing, "Rates".into()).is_none());
    wrong_family.set_value(&mut database).to(Some(Arc::new(CanonicalMetadata::CommonModule(
        Arc::new(CommonModule::builder().name("Other").build()),
    ))));
    assert!(resolve_common_module_by_file(&database, listing, FileId(3)).is_none());
    for family in ["parse_register_query", "parse_common_module_query", "parse_defined_type_query"]
    {
        assert_eq!(database.salsa_family_churn(family).expect("events").0, 0);
    }
}

/// Invalidating one family leaves the other family's memo and the root structure untouched.
#[test]
fn unrelated_canonical_family_does_not_execute_register_resolution() {
    let (mut database, listing) = database();
    let register = input(
        &database,
        CanonicalMetadata::Register(Arc::new(
            Register::builder().name("Rates").mdo_type(MdoType::InformationRegister).build(),
        )),
    );
    let module = input(
        &database,
        CanonicalMetadata::CommonModule(Arc::new(CommonModule::builder().name("Tools").build())),
    );
    let inputs: CanonicalMetadataInputs =
        [(FileId(0), register), (FileId(1), module)].into_iter().collect();
    listing.set_canonical_metadata(&mut database).to(Arc::new(inputs));
    let first = resolve_register_by_name(&database, listing, "Rates".into()).expect("register");
    let before = database.salsa_family_churn("resolve_register_by_name").expect("events").0;
    assert_eq!(before, 1);
    module.set_value(&mut database).to(None);
    module.set_source_revision(&mut database).to("invalid".into());
    let second =
        resolve_register_by_name(&database, listing, "Rates".into()).expect("memoized register");
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(database.salsa_family_churn("resolve_register_by_name").expect("events").0, before);
}
