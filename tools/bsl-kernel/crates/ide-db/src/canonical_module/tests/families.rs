//! Object/register/common/service owners are checked through their native consumer API.

use super::*;
use crate::metadata::{
    CommonModuleEntry, HTTPServiceEntry, IntegrationServiceEntry, MdoEntry, WebServiceEntry,
};

#[derive(Clone, Copy)]
enum Family {
    Object,
    Register,
    Common,
    Command,
    Http,
    Web,
    Integration,
}

impl Family {
    /// Every matrix case uses a real conventional module path and one explicit owner descriptor.
    const fn collection(self) -> &'static str {
        match self {
            Self::Object => "DataProcessors",
            Self::Register => "InformationRegisters",
            Self::Common => "CommonModules",
            Self::Command => "CommonCommands",
            Self::Http => "HTTPServices",
            Self::Web => "WebServices",
            Self::Integration => "IntegrationServices",
        }
    }

    /// Only object and register modules have family-specific filenames.
    const fn body(self) -> &'static str {
        match self {
            Self::Object => "ObjectModule.bsl",
            Self::Register => "RecordSetModule.bsl",
            Self::Command => "CommandModule.bsl",
            _ => "Module.bsl",
        }
    }

    /// Explicit payload changes make a reused previous model observable.
    fn value(self, name: &str, field: &str) -> CanonicalMetadata {
        match self {
            Self::Object => CanonicalMetadata::Object(object(MdoType::DataProcessor, name, field)),
            Self::Register => CanonicalMetadata::Register(Arc::new(
                bsl_metadata::Register::builder()
                    .uuid(bsl_metadata::Uuid::from_u128(1))
                    .name(name)
                    .mdo_type(MdoType::InformationRegister)
                    .add_resource(bsl_metadata::RegisterResource::new(
                        bsl_metadata::Uuid::from_u128(2),
                        field,
                    ))
                    .build(),
            )),
            Self::Common => CanonicalMetadata::CommonModule(Arc::new(
                bsl_metadata::CommonModule::builder()
                    .uuid(bsl_metadata::Uuid::from_u128(1))
                    .name(name)
                    .server(field == "Current")
                    .build(),
            )),
            Self::Command => {
                CanonicalMetadata::CommonCommand(Arc::new(bsl_metadata::CommonCommand::from_parts(
                    bsl_metadata::Uuid::from_u128(1),
                    name.into(),
                    Some(if field == "Current" {
                        AttributeType::Boolean
                    } else {
                        AttributeType::String { length: None }
                    }),
                    Some(bsl_metadata::CommandParameterUseMode::Single),
                    bsl_metadata::ObjectBelonging::Own,
                    None,
                )))
            }
            Self::Http => CanonicalMetadata::HTTPService(Arc::new(
                bsl_metadata::HTTPServiceBuilder::new().name(name).root_url(field).build(),
            )),
            Self::Web => CanonicalMetadata::WebService(Arc::new(
                bsl_metadata::WebServiceBuilder::new().name(name).namespace(field).build(),
            )),
            Self::Integration => CanonicalMetadata::IntegrationService(Arc::new(
                bsl_metadata::IntegrationServiceBuilder::new()
                    .name(name)
                    .add_channel(bsl_metadata::IntegrationServiceChannel::new(field))
                    .build(),
            )),
        }
    }

    /// Native consumers expose their exact typed owner without an intermediate Configuration.
    fn owner(self, module: &hir::ModuleMetadata) -> Option<CanonicalMetadata> {
        match self {
            Self::Object => module.mdo.clone().map(CanonicalMetadata::Object),
            Self::Register => module.register.clone().map(CanonicalMetadata::Register),
            Self::Common => module.common_module.clone().map(CanonicalMetadata::CommonModule),
            Self::Command => module.common_command.clone().map(CanonicalMetadata::CommonCommand),
            Self::Http => module.http_service.clone().map(CanonicalMetadata::HTTPService),
            Self::Web => module.web_service.clone().map(CanonicalMetadata::WebService),
            Self::Integration => {
                module.integration_service.clone().map(CanonicalMetadata::IntegrationService)
            }
        }
    }

    /// Body backlinks name only enrolled files; the neighbor has an independent descriptor slot.
    fn listing(self) -> MetadataListingData {
        let mut listing = MetadataListingData::default();
        for (name, main, body) in [("Owner", 0, 1), ("Neighbor", 2, 3)] {
            match self {
                Self::Object | Self::Register => listing.entries.push(MdoEntry {
                    kind: if matches!(self, Self::Object) {
                        MdoType::DataProcessor
                    } else {
                        MdoType::InformationRegister
                    },
                    name: name.into(),
                    main: FileId(main),
                    predefined: None,
                }),
                Self::Common => listing.common_modules.push(CommonModuleEntry {
                    name: name.into(),
                    main: FileId(main),
                    module_file: Some(FileId(body)),
                    unread_module_file: None,
                }),
                Self::Command => {
                    listing.common_commands.push(crate::metadata::CommonCommandEntry {
                        name: name.into(),
                        main: FileId(main),
                        module_file: Some(FileId(body)),
                    })
                }
                Self::Http => listing.http_services.push(HTTPServiceEntry {
                    name: name.into(),
                    main: FileId(main),
                    module_file: Some(FileId(body)),
                }),
                Self::Web => listing.web_services.push(WebServiceEntry {
                    name: name.into(),
                    main: FileId(main),
                    module_file: Some(FileId(body)),
                }),
                Self::Integration => listing.integration_services.push(IntegrationServiceEntry {
                    name: name.into(),
                    main: FileId(main),
                    module_file: Some(FileId(body)),
                }),
            }
        }
        listing
    }
}

/// Current values update only their own module; missing, wrong-name/kind and unprovided slots fail closed.
#[test]
fn canonical_module_owner_family_matrix() {
    for family in [
        Family::Object,
        Family::Register,
        Family::Common,
        Family::Command,
        Family::Http,
        Family::Web,
        Family::Integration,
    ] {
        let mut fixture = Fixture::new();
        let roots = fixture.roots(&[("base", RootKind::Base, &[])]);
        let root = &roots[0];
        for (name, main, body) in [("Owner", 0, 1), ("Neighbor", 2, 3)] {
            fixture.file(root, main, &format!("{}/{name}.xml", family.collection()), None);
            fixture.file(
                root,
                body,
                &format!("{}/{name}/Ext/{}", family.collection(), family.body()),
                Some(""),
            );
        }
        let saved = family.value("Owner", "Saved");
        let slot = fixture.slot(Some(saved.clone()));
        let neighbor = fixture.slot(Some(family.value("Neighbor", "Other")));
        fixture.listing(root, family.listing(), &[(0, slot), (2, neighbor)]);
        assert_eq!(family.owner(&fixture.module(1)), Some(saved));
        let retained = fixture.module(3);
        let current = family.value("Owner", "Current");
        update(&mut fixture, slot, Some(current.clone()));
        assert_eq!(family.owner(&fixture.module(1)), Some(current));
        assert!(Arc::ptr_eq(&retained, &fixture.module(3)));
        for rejected in [
            None,
            Some(family.value("Other", "WrongName")),
            Some(CanonicalMetadata::Role(Arc::new(bsl_metadata::Role::new("Owner")))),
        ] {
            update(&mut fixture, slot, rejected);
            assert!(family.owner(&fixture.module(1)).is_none());
            assert!(Arc::ptr_eq(&retained, &fixture.module(3)));
        }
        fixture.listing(root, family.listing(), &[(2, neighbor)]);
        assert!(family.owner(&fixture.module(1)).is_none());
        assert_eq!(fixture.database.salsa_family_churn("load_configuration").unwrap().0, 0);
    }
}

/// Native XML is available in VFS and on disk, but an absent current owner slot cannot use it.
#[test]
fn canonical_object_owner_refuses_real_xml_while_native_mode_keeps_its_loader() {
    let mut fixture = Fixture::new();
    let roots = fixture.roots(&[("base", RootKind::Base, &[])]);
    let root = &roots[0];
    let xml = "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\"><DataProcessor uuid=\"10000000-0000-4000-8000-000000000001\"><Properties><Name>Owner</Name></Properties><ChildObjects><Attribute uuid=\"20000000-0000-4000-8000-000000000001\"><Properties><Name>FromDisk</Name><Type><Type xmlns=\"http://v8.1c.ru/8.1/data/core\">xs:string</Type></Type></Properties></Attribute></ChildObjects></DataProcessor></MetaDataObject>";
    fs::create_dir_all(root.join("DataProcessors")).unwrap();
    fs::write(root.join("Configuration.xml"), "<MetaDataObject><Configuration uuid=\"30000000-0000-4000-8000-000000000001\"><Properties><Name>Native</Name></Properties></Configuration></MetaDataObject>").unwrap();
    fs::write(root.join("DataProcessors/Owner.xml"), xml).unwrap();
    fixture.file(root, 0, "DataProcessors/Owner.xml", Some(xml));
    fixture.file(root, 1, "DataProcessors/Owner/Ext/ObjectModule.bsl", Some(""));
    let marker = fixture.slot(None);
    fixture.listing(root, Family::Object.listing(), &[(2, marker)]);
    assert!(fixture.module(1).mdo.is_none());
    assert_eq!(fixture.database.salsa_family_churn("load_configuration").unwrap().0, 0);
    assert_eq!(fixture.database.salsa_family_churn("parse_mdo_query").unwrap().0, 0);
    fixture.listing(root, Family::Object.listing(), &[]);
    let native = fixture.module(1);
    assert_eq!(native.mdo.as_ref().expect("native owner").name, "Owner");
    assert!(native.mdo.as_ref().unwrap().attributes.iter().any(|field| field.name == "FromDisk"));
}
