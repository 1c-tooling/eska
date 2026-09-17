//! Read-only Designer source resolution, separated from logical metadata values.

mod mapping;
mod opening;

use std::{io, path::PathBuf};

use super::{
    Project,
    discovery::ContextDiscoveryError,
    metadata_model::{MetadataObject, MetadataProjectError, ModuleRole, ProjectScope},
    object_model::{AffectedObjectModel, ObjectModelError},
    selection::SelectionError,
};

pub use mapping::{LogicalLocation, SourceLocation, SourceRole};
pub use opening::open_projects;

/// One manifest-backed project and its checked root descriptor, without a full object index.
#[derive(Debug)]
pub struct DesignerSource {
    project: Project,
    scope: ProjectScope,
    root: MetadataObject,
    descriptor: PathBuf,
}

impl DesignerSource {
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
