//! Compose kernel identities from declared metadata, never from a directory scan.

use std::collections::BTreeMap;

use bsl_metadata::MdoType;
use eska::project_core::{MetadataKind, source_format::SourcePath};
use ide_db::metadata::{
    CommonAttributeEntry, CommonCommandEntry, CommonModuleEntry, DefinedTypeEntry,
    EventSubscriptionEntry, FilterCriterionEntry, HTTPServiceEntry, IntegrationServiceEntry,
    MdoEntry, MetadataListingData, RoleEntry, ScheduledJobEntry, SessionParameterEntry,
    SubsystemEntry, WebServiceEntry,
};
use vfs::FileId;

/// Every owner gets a stable slot; unsupported families keep an explicit unavailable value.
#[derive(Clone, Debug)]
pub struct OwnerBinding {
    pub kind: MetadataKind,
    pub name: Option<String>,
    pub main: FileId,
    pub native_path: SourcePath,
    /// Only top-level owners enter global resolver collections; nested slots retain their context.
    pub nested: bool,
}

impl OwnerBinding {
    /// Register supported resolver families without activating unrelated files.
    pub fn add_to_listing(&self, listing: &mut MetadataListingData) {
        if self.nested {
            return;
        }
        let Some(name) = &self.name else {
            return;
        };
        if self.add_configured_to_listing(listing, name) {
            return;
        }
        match self.kind {
            MetadataKind::Role => {
                listing.roles.push(RoleEntry {
                    name: name.clone(),
                    main: self.main,
                    rights: None,
                });
            }
            MetadataKind::IntegrationService => {
                listing.integration_services.push(IntegrationServiceEntry {
                    name: name.clone(),
                    main: self.main,
                    module_file: None,
                });
            }
            MetadataKind::WebService => {
                listing.web_services.push(WebServiceEntry {
                    name: name.clone(),
                    main: self.main,
                    module_file: None,
                });
            }
            MetadataKind::HTTPService => {
                listing.http_services.push(HTTPServiceEntry {
                    name: name.clone(),
                    main: self.main,
                    module_file: None,
                });
            }
            MetadataKind::Subsystem => {
                listing.subsystems.push(SubsystemEntry {
                    name: name.clone(),
                    main: self.main,
                });
            }
            MetadataKind::ScheduledJob => {
                listing.scheduled_jobs.push(ScheduledJobEntry {
                    name: name.clone(),
                    main: self.main,
                });
            }
            MetadataKind::EventSubscription => {
                listing.event_subscriptions.push(EventSubscriptionEntry {
                    name: name.clone(),
                    main: self.main,
                });
            }
            MetadataKind::CommonModule => listing.common_modules.push(CommonModuleEntry {
                name: name.clone(),
                main: self.main,
                module_file: None,
                unread_module_file: None,
            }),
            MetadataKind::SessionParameter => {
                listing.session_parameters.push(SessionParameterEntry {
                    name: name.clone(),
                    main: self.main,
                });
            }
            MetadataKind::DefinedType => listing.defined_types.push(DefinedTypeEntry {
                name: name.clone(),
                main: self.main,
            }),
            _ => {
                if let Ok(kind) = self.kind.designer_tag().parse::<MdoType>()
                    && MdoType::all().contains(&kind)
                {
                    listing.entries.push(MdoEntry {
                        kind,
                        name: name.clone(),
                        main: self.main,
                        // Canonical objects carry current items in their slot; this XML path is unused.
                        predefined: None,
                    });
                }
            }
        }
    }
    /// Dedicated configured families keep stable native slots without being fabricated MDOs.
    fn add_configured_to_listing(&self, listing: &mut MetadataListingData, name: &str) -> bool {
        match self.kind {
            MetadataKind::Sequence => {
                listing.sequences.push(ide_db::metadata::SequenceEntry {
                    name: name.into(),
                    main: self.main,
                    module_file: None,
                });
            }
            MetadataKind::FunctionalOptionsParameter => {
                listing.functional_option_parameters.push(
                    ide_db::metadata::FunctionalOptionsParameterEntry {
                        name: name.into(),
                        main: self.main,
                    },
                );
            }
            MetadataKind::FunctionalOption => {
                listing
                    .functional_options
                    .push(ide_db::metadata::FunctionalOptionEntry {
                        name: name.into(),
                        main: self.main,
                    });
            }
            MetadataKind::FilterCriterion => listing.filter_criteria.push(FilterCriterionEntry {
                name: name.into(),
                main: self.main,
            }),
            MetadataKind::CommonCommand => {
                listing.common_commands.push(CommonCommandEntry {
                    name: name.into(),
                    main: self.main,
                    module_file: None,
                });
            }
            MetadataKind::CommonAttribute => {
                listing.common_attributes.push(CommonAttributeEntry {
                    name: name.into(),
                    main: self.main,
                });
            }
            _ => return false,
        }
        true
    }
}

/// Allocate declared bodies after metadata IDs without reading their bytes or owner YAML.
pub fn register_modules(
    reader: &eska::project_core::source_format::SourceReader,
    root: base_db::SourceRootId,
    allocator: &mut crate::composition::FileIdAllocator,
    database: &mut ide_db::RootDatabaseImpl,
    file_set: &mut vfs::file_set::FileSet,
) -> Result<std::collections::BTreeMap<SourcePath, crate::modules::ModuleEntry>, crate::SessionError>
{
    use crate::{ModuleInput, ModuleState, SessionError, SessionRevision, modules::ModuleEntry};
    use base_db::SourceDatabase;
    use vfs::VfsPath;
    let mut modules = BTreeMap::new();
    for path in reader.module_paths() {
        let file = allocator
            .allocate()
            .map_err(|_| SessionError::TooManyFiles)?;
        let identity = reader
            .declared_module(path)
            .ok_or(SessionError::MissingListing)?;
        // Kernel conventions use virtual native paths; transport and disk reads use source paths.
        file_set.insert(
            file,
            VfsPath::new(reader.root().join(identity.native_path.as_path())),
        );
        database.set_file_source_root(file, root);
        database.set_file_unreadable(file);
        let model = ModuleInput {
            revision: SessionRevision::default(),
            origin: eska::analysis_core::InputOrigin::Saved,
            source_sha256: None,
            state: if identity.binary {
                ModuleState::Binary
            } else {
                ModuleState::Pending
            },
        };
        modules.insert(
            path.clone(),
            ModuleEntry {
                file,
                owner: identity.owner.clone(),
                native_path: identity.native_path.clone(),
                model,
            },
        );
    }
    Ok(modules)
}

/// Keep a stable body backlink; its tracked unread flag supplies current readability.
pub fn connect_common_bodies(
    structure: &mut MetadataListingData,
    modules: &std::collections::BTreeMap<SourcePath, crate::modules::ModuleEntry>,
) -> Result<(), crate::SessionError> {
    use crate::SessionError;
    let bodies = modules
        .values()
        .map(|module| (&module.native_path, module))
        .collect::<BTreeMap<_, _>>();
    for entry in &mut structure.common_modules {
        let prefix = format!("CommonModules/{}/Ext/Module", entry.name);
        if let Some(module) =
            bodies.get(&SourcePath::parse(&format!("{prefix}.bsl")).map_err(SessionError::Source)?)
        {
            entry.module_file = Some(module.file);
        } else if let Some(module) =
            bodies.get(&SourcePath::parse(&format!("{prefix}.bin")).map_err(SessionError::Source)?)
        {
            entry.unread_module_file = Some(module.file);
        }
    }
    for entry in &mut structure.sequences {
        let path = SourcePath::parse(&format!("Sequences/{}/Ext/RecordSetModule.bsl", entry.name))
            .map_err(SessionError::Source)?;
        entry.module_file = bodies.get(&path).map(|module| module.file);
    }
    for entry in &mut structure.common_commands {
        let path = SourcePath::parse(&format!(
            "CommonCommands/{}/Ext/CommandModule.bsl",
            entry.name
        ))
        .map_err(SessionError::Source)?;
        entry.module_file = bodies.get(&path).map(|module| module.file);
    }
    Ok(())
}

/// Service module identities stay stable while tracked module inputs determine current readability.
pub fn connect_service_bodies(
    structure: &mut MetadataListingData,
    modules: &BTreeMap<SourcePath, crate::modules::ModuleEntry>,
) -> Result<(), crate::SessionError> {
    let bodies = modules
        .values()
        .map(|module| (&module.native_path, module))
        .collect::<BTreeMap<_, _>>();
    for entry in &mut structure.http_services {
        entry.module_file = declared_body(&bodies, "HTTPServices", &entry.name)?;
    }
    for entry in &mut structure.web_services {
        entry.module_file = declared_body(&bodies, "WebServices", &entry.name)?;
    }
    for entry in &mut structure.integration_services {
        entry.module_file = declared_body(&bodies, "IntegrationServices", &entry.name)?;
    }
    Ok(())
}

/// Lookup is limited to the validated manifest's BSL identities; absent and binary bodies stay unlinked.
fn declared_body(
    modules: &BTreeMap<&SourcePath, &crate::modules::ModuleEntry>,
    collection: &str,
    name: &str,
) -> Result<Option<FileId>, crate::SessionError> {
    let path = SourcePath::parse(&format!("{collection}/{name}/Ext/Module.bsl"))
        .map_err(crate::SessionError::Source)?;
    Ok(modules.get(&path).map(|module| module.file))
}

/// Navigation maps registered kernel identities back to physical authoring inputs in logarithmic time.
pub fn source_paths_by_file(
    modules: &BTreeMap<SourcePath, crate::modules::ModuleEntry>,
) -> BTreeMap<FileId, SourcePath> {
    modules
        .iter()
        .map(|(path, entry)| (entry.file, path.clone()))
        .collect()
}
