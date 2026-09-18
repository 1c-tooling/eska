//! Read-only, manifest-backed metadata sessions; no CLI, platform, watcher or disk cache.

mod session;

use std::path::Path;

use super::{
    configurator::TreeError,
    designer_source::{SourceError, open_projects},
    metadata_model::{NodeId, ObjectId, ProjectScope},
    metadata_parser::LoadError,
};

pub use session::{ObjectSummary, ProjectSession};

/// One discovery context; identical object IDs in different members remain isolated.
#[derive(Debug)]
pub struct MetadataWorkspace {
    projects: Vec<ProjectSession>,
}

/// Failures preserve machine-readable categories and the underlying source/parse context.
#[derive(Debug)]
pub enum WorkspaceError {
    Source(SourceError),
    Load(LoadError),
    Tree(TreeError),
    UnknownProject(ProjectScope),
    UnknownNode(NodeId),
    UnknownObject(ObjectId),
    MissingSource(NodeId),
    BrokenAncestry(NodeId),
}

impl MetadataWorkspace {
    /// Discover selected manifest-backed projects and read only their root structure.
    ///
    /// # Errors
    /// Returns discovery, selection, source, root parsing or projection failures.
    pub fn open(
        start: &Path,
        names: &[String],
        entire_workspace: bool,
    ) -> Result<Self, WorkspaceError> {
        let projects = open_projects(start, names, entire_workspace)
            .map_err(WorkspaceError::Source)?
            .into_iter()
            .map(ProjectSession::open)
            .collect::<Result<_, _>>()?;
        Ok(Self { projects })
    }

    /// Inspect the selected sessions in discovery order.
    #[must_use]
    pub fn projects(&self) -> &[ProjectSession] {
        &self.projects
    }

    /// Locate a selected project without performing IO.
    ///
    /// # Errors
    /// Returns an unknown scope, never another project's matching object ID.
    pub fn project(&self, scope: &ProjectScope) -> Result<&ProjectSession, WorkspaceError> {
        self.projects
            .iter()
            .find(|project| project.scope() == scope)
            .ok_or_else(|| WorkspaceError::UnknownProject(scope.clone()))
    }

    /// Borrow one session for lazy branch expansion.
    ///
    /// # Errors
    /// Returns an unknown scope.
    pub fn project_mut(
        &mut self,
        scope: &ProjectScope,
    ) -> Result<&mut ProjectSession, WorkspaceError> {
        self.projects
            .iter_mut()
            .find(|project| project.scope() == scope)
            .ok_or_else(|| WorkspaceError::UnknownProject(scope.clone()))
    }

    /// Release all session data; ordinary drop has the same effect and writes nothing.
    pub fn close(self) {}
}
