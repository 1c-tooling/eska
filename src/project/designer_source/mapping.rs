use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{DesignerSource, SourceError};
use crate::project::{
    ProjectType,
    metadata_model::{MetadataKind, ModuleRole, ObjectId},
};

/// Logical position inside an owning descriptor, before parser-specific byte coordinates exist.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogicalLocation {
    pub kind: MetadataKind,
    pub name: String,
}

/// Distinguish the descriptor, module and form/template payload of an object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceRole {
    Descriptor,
    Module(ModuleRole),
    Payload,
}

/// An existing source file and optional logical ancestry inside its XML descriptor.
///
/// `inline` records a logical lookup, not a verified text range. The parser must
/// validate that lookup before exposing editor coordinates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceLocation {
    pub path: PathBuf,
    pub role: SourceRole,
    pub inline: Vec<LogicalLocation>,
}

const MODULE_FILES: &[(ModuleRole, &str)] = &[
    (ModuleRole::Module, "Module.bsl"),
    (ModuleRole::Object, "ObjectModule.bsl"),
    (ModuleRole::Manager, "ManagerModule.bsl"),
    (ModuleRole::RecordSet, "RecordSetModule.bsl"),
    (ModuleRole::ValueManager, "ValueManagerModule.bsl"),
    (
        ModuleRole::ManagedApplication,
        "ManagedApplicationModule.bsl",
    ),
    (
        ModuleRole::OrdinaryApplication,
        "OrdinaryApplicationModule.bsl",
    ),
    (ModuleRole::Session, "SessionModule.bsl"),
    (
        ModuleRole::ExternalConnection,
        "ExternalConnectionModule.bsl",
    ),
    (ModuleRole::Command, "CommandModule.bsl"),
];

struct Locations {
    descriptors: Vec<PathBuf>,
    inline: Vec<LogicalLocation>,
    kind: MetadataKind,
    root: bool,
}

impl DesignerSource {
    /// Resolve lexical descriptor ownership even after a source file was removed.
    pub(crate) fn descriptor_candidates(
        &self,
        id: &ObjectId,
    ) -> Result<(ObjectId, Vec<PathBuf>), SourceError> {
        let locations = self.locations(id)?;
        let mut owner = id.clone();
        for _ in locations.inline {
            owner = owner.parent().ok_or_else(|| SourceError::InvalidIdentity {
                value: id.to_string(),
            })?;
        }
        Ok((owner, locations.descriptors))
    }

    /// Resolve only the owning descriptor and inline ancestry, without probing modules/payloads.
    ///
    /// # Errors
    /// Returns malformed identity, ambiguous layout or a source containment failure.
    pub fn object_descriptor(&self, id: &ObjectId) -> Result<Option<SourceLocation>, SourceError> {
        let locations = self.locations(id)?;
        Ok(self
            .unique_existing(&locations.descriptors)?
            .map(|path| SourceLocation {
                path,
                role: SourceRole::Descriptor,
                inline: locations.inline,
            }))
    }
    /// Resolve an object's existing descriptor, modules and primary payloads without reading BSL.
    ///
    /// # Errors
    /// Returns invalid identity, ambiguous layout, containment or filesystem failures.
    pub fn sources(&self, id: &ObjectId) -> Result<Vec<SourceLocation>, SourceError> {
        let locations = self.locations(id)?;
        let Some(descriptor) = self.unique_existing(&locations.descriptors)? else {
            return Ok(Vec::new());
        };
        let mut sources = vec![SourceLocation {
            path: descriptor.clone(),
            role: SourceRole::Descriptor,
            inline: locations.inline.clone(),
        }];
        if !locations.inline.is_empty() {
            return Ok(sources);
        }
        let bases = self.artifact_bases(&descriptor, locations.root);
        for &(role, file) in MODULE_FILES {
            if let Some(path) = self.module_path(&bases, locations.kind, role, file)? {
                sources.push(SourceLocation {
                    path,
                    role: SourceRole::Module(role),
                    inline: Vec::new(),
                });
            }
        }
        sources.extend(self.payloads(&bases, locations.kind)?);
        Ok(sources)
    }

    /// Resolve one existing BSL module directly; a binary-only module is absent.
    ///
    /// # Errors
    /// Returns malformed identity, ambiguous layout or a source containment failure.
    pub fn module(
        &self,
        owner: &ObjectId,
        role: ModuleRole,
    ) -> Result<Option<SourceLocation>, SourceError> {
        let locations = self.locations(owner)?;
        if !locations.inline.is_empty() {
            return Err(SourceError::UnsupportedModule { role });
        }
        let Some(descriptor) = self.unique_existing(&locations.descriptors)? else {
            return Ok(None);
        };
        let file = MODULE_FILES
            .iter()
            .find_map(|&(candidate, file)| (candidate == role).then_some(file))
            .ok_or(SourceError::UnsupportedModule { role })?;
        let bases = self.artifact_bases(&descriptor, locations.root);
        Ok(self
            .module_path(&bases, locations.kind, role, file)?
            .map(|path| SourceLocation {
                path,
                role: SourceRole::Module(role),
                inline: Vec::new(),
            }))
    }

    /// Translate logical ancestry to candidate descriptors without scanning object collections.
    fn locations(&self, id: &ObjectId) -> Result<Locations, SourceError> {
        let parts = decode(id)?;
        let first = parts.first().ok_or_else(|| SourceError::InvalidIdentity {
            value: id.to_string(),
        })?;
        let root_parts = decode(self.root.id())?;
        let external = matches!(
            self.project.configuration().project_type(),
            ProjectType::Processing | ProjectType::Report
        );
        let root = first == &root_parts[0];
        if external && !root {
            return Err(SourceError::UnsupportedLocation {
                value: id.to_string(),
            });
        }
        let mut descriptors =
            if root {
                vec![self.descriptor.clone()]
            } else {
                let folder = first.kind.collection_folder().ok_or_else(|| {
                    SourceError::UnsupportedLocation {
                        value: id.to_string(),
                    }
                })?;
                vec![Path::new(folder).join(format!("{}.xml", first.name))]
            };
        let mut kind = first.kind;
        let mut inline = Vec::new();
        for child in &parts[1..] {
            let folder = match child.kind {
                MetadataKind::Form => Some("Forms"),
                MetadataKind::Template => Some("Templates"),
                MetadataKind::Command => Some("Commands"),
                MetadataKind::Subsystem => Some("Subsystems"),
                _ => None,
            };
            if let Some(folder) = folder {
                if !inline.is_empty() {
                    return Err(SourceError::UnsupportedLocation {
                        value: id.to_string(),
                    });
                }
                let bases =
                    if kind == first.kind && root && descriptors == [self.descriptor.clone()] {
                        self.artifact_bases(&self.descriptor, true)
                    } else {
                        descriptors
                            .iter()
                            .map(|path| path.with_extension(""))
                            .collect()
                    };
                descriptors = bases
                    .iter()
                    .map(|base| base.join(folder).join(format!("{}.xml", child.name)))
                    .collect();
            } else {
                inline.push(child.clone());
            }
            kind = child.kind;
        }
        Ok(Locations {
            descriptors,
            inline,
            kind,
            root: root && parts.len() == 1,
        })
    }

    /// Root artifacts may be direct or wrapped; ordinary objects use their descriptor stem.
    fn artifact_bases(&self, descriptor: &Path, root: bool) -> Vec<PathBuf> {
        if !root {
            return vec![descriptor.with_extension("")];
        }
        match self.project.configuration().project_type() {
            ProjectType::Configuration | ProjectType::Extension => vec![PathBuf::new()],
            ProjectType::Processing | ProjectType::Report => {
                vec![PathBuf::new(), descriptor.with_extension("")]
            }
        }
    }

    /// Reject duplicate physical candidates instead of silently opening the wrong source.
    fn unique_existing(&self, candidates: &[PathBuf]) -> Result<Option<PathBuf>, SourceError> {
        let mut found = Vec::new();
        for path in candidates {
            if self.checked_file(path)?.is_some() {
                found.push(path.clone());
            }
        }
        if found.len() > 1 {
            return Err(SourceError::AmbiguousLocation { paths: found });
        }
        Ok(found.pop())
    }

    /// Managed form modules are below `Ext/Form`; other roles use the owner's `Ext`.
    fn module_path(
        &self,
        bases: &[PathBuf],
        kind: MetadataKind,
        role: ModuleRole,
        file: &str,
    ) -> Result<Option<PathBuf>, SourceError> {
        let paths: Vec<_> = bases
            .iter()
            .map(|base| {
                let ext = base.join("Ext");
                if role == ModuleRole::Module
                    && matches!(kind, MetadataKind::Form | MetadataKind::CommonForm)
                {
                    ext.join("Form").join(file)
                } else {
                    ext.join(file)
                }
            })
            .collect();
        self.unique_existing(&paths)
    }

    /// Inspect only the selected owner's payload directory, never the entire source tree.
    fn payloads(
        &self,
        bases: &[PathBuf],
        kind: MetadataKind,
    ) -> Result<Vec<SourceLocation>, SourceError> {
        if !matches!(
            kind,
            MetadataKind::Form
                | MetadataKind::CommonForm
                | MetadataKind::Template
                | MetadataKind::CommonTemplate
        ) {
            return Ok(Vec::new());
        }
        let prefix = if matches!(kind, MetadataKind::Form | MetadataKind::CommonForm) {
            "Form."
        } else {
            "Template."
        };
        let mut payloads = Vec::new();
        for base in bases {
            let relative = base.join("Ext");
            let directory = self.project.source().join(&relative);
            let mut stats = self.io_stats.get();
            stats.path_checks += 1;
            self.io_stats.set(stats);
            let physical = match fs::canonicalize(&directory) {
                Ok(path) => path,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(source) => {
                    return Err(SourceError::Io {
                        path: relative,
                        source,
                    });
                }
            };
            if !physical.starts_with(self.project.source()) {
                return Err(SourceError::OutsideSource { path: relative });
            }
            let mut stats = self.io_stats.get();
            stats.directory_reads += 1;
            self.io_stats.set(stats);
            for entry in fs::read_dir(&physical).map_err(|source| SourceError::Io {
                path: relative.clone(),
                source,
            })? {
                let entry = entry.map_err(|source| SourceError::Io {
                    path: relative.clone(),
                    source,
                })?;
                if entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.starts_with(prefix))
                {
                    let path = relative.join(entry.file_name());
                    if self.checked_file(&path)?.is_some() {
                        payloads.push(SourceLocation {
                            path,
                            role: SourceRole::Payload,
                            inline: Vec::new(),
                        });
                    }
                }
            }
        }
        payloads.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(payloads)
    }
}

/// Decode only identifiers constructed by the metadata model; path separators remain invalid names.
fn decode(id: &ObjectId) -> Result<Vec<LogicalLocation>, SourceError> {
    id.as_str()
        .split('/')
        .map(|part| {
            let invalid = || SourceError::InvalidIdentity {
                value: id.to_string(),
            };
            let (key, name) = part.split_once(':').ok_or_else(invalid)?;
            let kind = MetadataKind::ALL
                .iter()
                .copied()
                .find(|kind| kind.as_str() == key)
                .ok_or_else(invalid)?;
            let name = name
                .replace("%3A", ":")
                .replace("%2F", "/")
                .replace("%25", "%");
            if name.is_empty()
                || matches!(name.as_str(), "." | "..")
                || name.contains(['/', '\\', ':', '\0'])
            {
                return Err(SourceError::UnsupportedLocation {
                    value: id.to_string(),
                });
            }
            Ok(LogicalLocation { kind, name })
        })
        .collect()
}
