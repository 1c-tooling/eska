//! Filesystem binding of the syntax-only external-object version contract.

mod eska;
mod paths;

pub(super) use eska::{bump as bump_eska, load as load_eska};

use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

use super::{LoadedDescriptor, ProjectVersion, VersionError};
use crate::project_core::{
    Project, ProjectType, SourceFormat, native,
    object_module_version::{self, ContractError, ContractErrorKind, MigrationPlan, TextEdit},
    publish::{self, PublishError},
};

/// A reviewable module change owns the inspected native descriptor and module snapshots.
#[derive(Debug)]
pub struct VersionMigration {
    path: PathBuf,
    original: Option<String>,
    plan: MigrationPlan,
    binding: MigrationBinding,
}

/// Only the selected source adapter owns authority to publish its inspected bytes.
#[derive(Debug)]
enum MigrationBinding {
    Native {
        source: PathBuf,
        descriptor: PathBuf,
        descriptor_text: String,
    },
    Eska(Box<eska::Binding>),
}

impl VersionMigration {
    /// Return the exact source file, including for explicit creation of a missing module.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// A preview distinguishes file creation from edits to an existing module.
    #[must_use]
    pub const fn creates_module(&self) -> bool {
        self.original.is_none()
    }

    /// Expose the selected authoritative value for CLI/IDE output.
    #[must_use]
    pub const fn version(&self) -> &ProjectVersion {
        self.plan.version()
    }

    /// Keep coordinates relative to the original module rather than the preview output.
    #[must_use]
    pub fn edits(&self) -> &[TextEdit] {
        self.plan.edits()
    }

    /// CLI previews show only the inspected ranges, rather than serializing the entire module.
    #[must_use]
    pub fn original(&self) -> &str {
        self.original.as_deref().unwrap_or_default()
    }

    /// Publish only the inspected descriptor/module state and preserve file permissions.
    ///
    /// # Errors
    /// Returns a stale snapshot, protected/unsafe source or filesystem publication failure.
    pub fn publish(&self) -> Result<(), VersionError> {
        let (source, descriptor, descriptor_text) = match &self.binding {
            MigrationBinding::Native {
                source,
                descriptor,
                descriptor_text,
            } => (source, descriptor, descriptor_text),
            MigrationBinding::Eska(binding) => return binding.publish(),
        };
        paths::existing(source, descriptor)?;
        if read_xml(descriptor)? != *descriptor_text {
            return Err(publication(descriptor, PublishError::Conflict));
        }
        let current = module_path(source, descriptor)?;
        if current != self.path {
            return Err(publication(&self.path, PublishError::Conflict));
        }
        let actual = read_module(&self.path)?;
        if actual != self.original {
            return Err(publication(&self.path, PublishError::Conflict));
        }
        let original = actual.as_deref().unwrap_or_default();
        let updated = self
            .plan
            .apply(original)
            .map_err(|error| contract(&self.path, error))?;
        if self.plan.edits().is_empty() {
            return Ok(());
        }
        let relative = self
            .path
            .strip_prefix(source)
            .map_err(|_| publication(&self.path, PublishError::UnsafePath))?;
        if self.original.is_some() {
            publish::publish_snapshot(source, relative, original, &updated)
                .map_err(|error| publication(&self.path, error))
        } else {
            let _directories = paths::create_parents(source, &self.path)?;
            publish::publish_new(source, relative, &updated)
                .map_err(|error| publication(&self.path, error))
        }
    }
}

/// Prepare an external project's explicit migration without changing its source or configuration.
///
/// # Errors
/// Requires a Designer XML external object and a syntax-confirmed migration, including its policy.
pub fn plan_migration(
    project: &Project,
    initial: Option<&ProjectVersion>,
) -> Result<VersionMigration, VersionError> {
    plan_migration_with_base(project, initial, None)
}

/// Bind an ESKA preview and publication to caller-selected accepted bytes and native permissions.
///
/// # Errors
/// Returns missing trust, unsupported scope, invalid migration or restricted source failures.
pub fn plan_migration_with_base(
    project: &Project,
    initial: Option<&ProjectVersion>,
    expected: Option<&crate::project_core::source_format::AcceptedRef>,
) -> Result<VersionMigration, VersionError> {
    if matches!(
        project.configuration().source_format(),
        SourceFormat::Eska { .. }
    ) {
        return eska::plan_migration(project, initial, expected);
    }
    if expected.is_some() {
        return Err(VersionError::AcceptedBaseUnexpected);
    }
    if !matches!(
        project.configuration().source_format(),
        SourceFormat::DesignerXml
    ) {
        return Err(VersionError::SourceFormatUnsupported);
    }
    let kind = project.configuration().project_type();
    if !matches!(kind, ProjectType::Processing | ProjectType::Report) {
        return Err(VersionError::MigrationUnsupported);
    }
    let (descriptor, descriptor_text) = root_descriptor(project.source(), kind)?;
    let path = module_path(project.source(), &descriptor)?;
    let original = read_module(&path)?;
    let plan =
        object_module_version::plan_migration(original.as_deref().unwrap_or_default(), initial)
            .map_err(|error| contract(&path, error))?;
    if let Some(scheme) = project.configuration().version_scheme() {
        plan.version().validate_scheme(scheme)?;
    }
    Ok(VersionMigration {
        path,
        original,
        plan,
        binding: MigrationBinding::Native {
            source: project.source().to_owned(),
            descriptor,
            descriptor_text,
        },
    })
}

/// The XML descriptor identifies the object; it cannot supply an external version number.
pub(super) fn load(source: &Path, kind: ProjectType) -> Result<LoadedDescriptor, VersionError> {
    let (descriptor, _) = root_descriptor(source, kind)?;
    let path = module_path(source, &descriptor)?;
    let input =
        read_module(&path)?.ok_or_else(|| VersionError::ModuleMissing { path: path.clone() })?;
    let info =
        object_module_version::inspect_contract(&input).map_err(|error| contract(&path, error))?;
    Ok(LoadedDescriptor {
        path,
        input: input.into_bytes(),
        version: info.version,
        value_range: info.value_range,
    })
}

/// Existing module bumps share exact-byte publication with metadata edits.
pub(super) fn publish_existing(
    source: &Path,
    path: &Path,
    original: &[u8],
    updated: &[u8],
) -> Result<(), VersionError> {
    let relative = path
        .strip_prefix(source)
        .map_err(|_| publication(path, PublishError::UnsafePath))?;
    let original = std::str::from_utf8(original)
        .map_err(|error| io_error(path, io::Error::new(io::ErrorKind::InvalidData, error)))?;
    let updated = std::str::from_utf8(updated)
        .map_err(|error| io_error(path, io::Error::new(io::ErrorKind::InvalidData, error)))?;
    publish::publish_snapshot(source, relative, original, updated)
        .map_err(|error| publication(path, error))
}

/// Resolve the single actual descriptor, without assuming the imported artifact's filename.
fn root_descriptor(source: &Path, kind: ProjectType) -> Result<(PathBuf, String), VersionError> {
    let mut found = None;
    for entry in fs::read_dir(source).map_err(|error| io_error(source, error))? {
        let entry = entry.map_err(|error| io_error(source, error))?;
        let path = entry.path();
        if entry.file_name() == "ConfigDumpInfo.xml"
            || !path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
        {
            continue;
        }
        let metadata = paths::existing(source, &path)?;
        if metadata.is_none_or(|metadata| !metadata.is_file()) {
            continue;
        }
        let text = read_xml(&path)?;
        if native::project_type(&text).map_err(|source| VersionError::InvalidXml {
            path: path.clone(),
            source,
        })? != Some(kind)
        {
            continue;
        }
        if found.replace((path, text)).is_some() {
            return Err(VersionError::DescriptorAmbiguous {
                path: source.to_owned(),
            });
        }
    }
    found.ok_or_else(|| VersionError::DescriptorMissing {
        path: source.to_owned(),
    })
}

/// Direct and wrapped native exports are supported, but never selected by directory iteration order.
fn module_path(source: &Path, descriptor: &Path) -> Result<PathBuf, VersionError> {
    let candidates = [
        source.join("Ext/ObjectModule.bsl"),
        descriptor.with_extension("").join("Ext/ObjectModule.bsl"),
    ];
    let mut found = None;
    for path in &candidates {
        let binary = path.with_extension("bin");
        if paths::existing(source, &binary)?.is_some() {
            return Err(VersionError::ModuleProtected { path: binary });
        }
        if let Some(metadata) = paths::existing(source, path)? {
            if !metadata.is_file() {
                return Err(publication(path, PublishError::UnsafePath));
            }
            if found.replace(path.clone()).is_some() {
                return Err(VersionError::ModuleAmbiguous {
                    path: source.to_owned(),
                });
            }
        }
    }
    if let Some(path) = found {
        return Ok(path);
    }
    let mut parents = Vec::new();
    for path in &candidates {
        if let Some(parent) = path.parent()
            && paths::existing(source, parent)?.is_some()
        {
            parents.push(path.clone());
        }
    }
    match parents.as_slice() {
        [path] => Ok(path.clone()),
        [] => Ok(candidates[1].clone()),
        _ => Err(VersionError::ModuleAmbiguous {
            path: source.to_owned(),
        }),
    }
}

/// A bounded native descriptor has no executable role in a migration.
fn read_xml(path: &Path) -> Result<String, VersionError> {
    let bytes = read_bytes(path, super::MAX_DESCRIPTOR_BYTES)?;
    if bytes.len() as u64 > super::MAX_DESCRIPTOR_BYTES {
        return Err(VersionError::DescriptorTooLarge {
            path: path.to_owned(),
        });
    }
    String::from_utf8(bytes)
        .map_err(|error| io_error(path, io::Error::new(io::ErrorKind::InvalidData, error)))
}

/// Absence is accepted only by the explicit creation plan, not inspection or bump.
fn read_module(path: &Path) -> Result<Option<String>, VersionError> {
    let bytes = match read_bytes(path, object_module_version::MAX_MODULE_BYTES as u64) {
        Ok(bytes) => bytes,
        Err(VersionError::Io { source, .. }) if source.kind() == io::ErrorKind::NotFound => {
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    if bytes.len() > object_module_version::MAX_MODULE_BYTES {
        return Err(contract(
            path,
            ContractError {
                kind: ContractErrorKind::ModuleTooLarge,
                range: 0..bytes.len(),
            },
        ));
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|error| io_error(path, io::Error::new(io::ErrorKind::InvalidData, error)))
}

/// Bound allocation even if a concurrent writer grows a native file during the read.
fn read_bytes(path: &Path, limit: u64) -> Result<Vec<u8>, VersionError> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(limit + 1).read_to_end(&mut bytes))
        .map_err(|error| io_error(path, error))?;
    Ok(bytes)
}

/// Preserve the original module path and byte coordinates in every syntax diagnostic.
fn contract(path: &Path, error: ContractError) -> VersionError {
    VersionError::ModuleContract {
        path: path.to_owned(),
        error,
    }
}

/// Keep OS errors structured rather than returning localized strings from core.
fn io_error(path: &Path, source: io::Error) -> VersionError {
    VersionError::Io {
        path: path.to_owned(),
        source,
    }
}

/// Publication failures retain both the operation's source path and the shared safety reason.
fn publication(path: &Path, error: PublishError) -> VersionError {
    VersionError::Publication {
        path: path.to_owned(),
        error,
    }
}
