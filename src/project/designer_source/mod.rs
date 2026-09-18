//! Read-only Designer source resolution, separated from logical metadata values.

mod mapping;
mod opening;

use super::metadata_disk_cache::DiskCache;
use std::{cell::Cell, io, path::PathBuf};

use super::{
    Project,
    discovery::ContextDiscoveryError,
    metadata_model::{MetadataObject, MetadataProjectError, ModuleRole, ProjectScope},
    object_model::{AffectedObjectModel, ObjectModelError},
    selection::SelectionError,
};

pub use mapping::{LogicalLocation, SourceLocation, SourceRole};
pub use opening::open_projects;
pub(crate) use opening::open_projects_cached;

/// One manifest-backed project and its checked root descriptor, without a full object index.
#[derive(Debug)]
pub struct DesignerSource {
    project: Project,
    scope: ProjectScope,
    root: MetadataObject,
    descriptor: PathBuf,
    pub(crate) disk_cache: Option<DiskCache>,
    io_stats: Cell<SourceIoStats>,
}

/// Source operations only; cache-file IO is reported separately.
#[derive(Clone, Copy, Debug, Default)]
pub struct SourceIoStats {
    pub xml_reads: u64,
    pub xml_bytes: u64,
    pub xml_read_nanos: u128,
    pub root_hash_nanos: u128,
    pub path_checks: u64,
    pub metadata_checks: u64,
    pub directory_reads: u64,
    pub root_parses: u64,
}

impl DesignerSource {
    /// Observe actual descriptor reads and resolver probes, without filesystem IO.
    #[must_use]
    pub const fn io_stats(&self) -> SourceIoStats {
        self.io_stats.get()
    }

    /// Count containment/existence probes at the shared resolver boundary.
    fn checked_file(&self, relative: &std::path::Path) -> Result<Option<PathBuf>, SourceError> {
        let mut stats = self.io_stats.get();
        let result = opening::existing_file(self.project.source(), relative, &mut stats);
        self.io_stats.set(stats);
        result
    }

    /// Return the selected project with validated manifest settings.
    #[must_use]
    pub const fn project(&self) -> &Project {
        &self.project
    }

    /// Return the logical scope within this discovery context.
    #[must_use]
    pub const fn scope(&self) -> &ProjectScope {
        &self.scope
    }

    /// Return root metadata parsed from the checked root descriptor.
    #[must_use]
    pub const fn root(&self) -> &MetadataObject {
        &self.root
    }

    /// Return the root descriptor relative to the configured source directory.
    #[must_use]
    pub fn descriptor(&self) -> &std::path::Path {
        &self.descriptor
    }

    /// Read one bounded descriptor after checking containment again at the read boundary.
    pub(crate) fn read_xml(
        &self,
        relative: &std::path::Path,
    ) -> Result<Option<String>, SourceError> {
        let Some(path) = self.checked_file(relative)? else {
            return Ok(None);
        };
        let mut stats = self.io_stats.get();
        stats.xml_reads += 1;
        self.io_stats.set(stats);
        let started = std::time::Instant::now();
        let input = opening::read_descriptor(&path)?;
        let mut stats = self.io_stats.get();
        stats.xml_bytes += input.len() as u64;
        stats.xml_read_nanos += started.elapsed().as_nanos();
        self.io_stats.set(stats);
        Ok(Some(input))
    }

    /// Reuse changed-path ownership discovery, including partial failures and read counters.
    ///
    /// # Errors
    /// Returns a filesystem or ownership error without expanding unrelated source branches.
    pub fn changed_owners(
        &self,
        paths: &[PathBuf],
    ) -> Result<AffectedObjectModel, ObjectModelError> {
        super::object_model::discover_affected(&self.project, paths)
    }
}

/// Resolution failures retain structured context for the future IDE presentation layer.
#[derive(Debug)]
pub enum SourceError {
    Discovery(ContextDiscoveryError),
    Selection(SelectionError),
    Io {
        path: PathBuf,
        source: io::Error,
    },
    OutsideSource {
        path: PathBuf,
    },
    NotFile {
        path: PathBuf,
    },
    DescriptorTooLarge {
        path: PathBuf,
    },
    InvalidRoot {
        path: PathBuf,
        source: MetadataProjectError,
    },
    InvalidMetadata {
        path: PathBuf,
        reason: &'static str,
    },
    Parse {
        path: PathBuf,
        source: super::metadata_parser::ParseError,
    },
    MissingRoot,
    AmbiguousRoot {
        paths: Vec<PathBuf>,
    },
    InvalidIdentity {
        value: String,
    },
    UnsupportedLocation {
        value: String,
    },
    AmbiguousLocation {
        paths: Vec<PathBuf>,
    },
    UnsupportedModule {
        role: ModuleRole,
    },
}
