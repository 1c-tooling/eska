//! Reverse source navigation resolves declared ownership once, independently of IDE tree state.

use std::path::{Component, Path};

use super::{ProjectSession, WorkspaceError};
use crate::project::{
    configurator::TreeOptions,
    designer_source::{SourceLocation, SourceRole},
    metadata_model::{MetadataKind, NodeId, ObjectId},
    object_model::logical_path_for_source,
};

/// The exact source and inclusive presentation ancestry share the current session generation.
#[derive(Debug)]
pub struct SourceNavigation {
    pub source: SourceLocation,
    pub ancestry: Vec<NodeId>,
}

impl ProjectSession {
    /// Resolve an existing source by declared owners, without scanning siblings or building a search index.
    ///
    /// # Errors
    /// Rejects escaping paths, invalid declared descriptors and source containment failures.
    /// Unknown, undeclared and missing sources return `None`.
    pub fn reveal_source(
        &mut self,
        path: &Path,
    ) -> Result<Option<SourceNavigation>, WorkspaceError> {
        if path.as_os_str().is_empty()
            || path
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(WorkspaceError::InvalidChangedPath(path.to_path_buf()));
        }
        let Some(owner) = self.source_owner(path) else {
            return Ok(None);
        };
        if self
            .source
            .checked_file(path)
            .map_err(WorkspaceError::Source)?
            .is_none()
        {
            return Ok(None);
        }
        match self.reveal_declared_object(&owner) {
            Ok(_) => (),
            Err(WorkspaceError::UnknownObject(_) | WorkspaceError::UnknownNode(_)) => {
                return Ok(None);
            }
            Err(error) => return Err(error),
        }
        let object = NodeId::Object(owner.clone());
        let Some(source) = self
            .source(&object)?
            .into_iter()
            .find(|source| source.path == path)
        else {
            return Ok(None);
        };
        let target = if let SourceRole::Module(role) = source.role {
            self.children(&object, TreeOptions::default())?;
            NodeId::Module { owner, role }
        } else {
            object
        };
        Ok(Some(SourceNavigation {
            source,
            ancestry: self.ancestry(&target)?,
        }))
    }

    /// Share the Designer ownership vocabulary with support checks; declarations are verified by each caller.
    pub(super) fn source_owner(&self, path: &Path) -> Option<ObjectId> {
        let logical = logical_path_for_source(self.project().configuration().project_type(), path)?;
        let mut owner = None;
        for part in logical.parts {
            let Some(kind) = MetadataKind::ALL
                .iter()
                .find(|kind| kind.as_str() == part.kind)
            else {
                break;
            };
            let name = part
                .name
                .as_deref()
                .unwrap_or_else(|| self.source.root().name());
            owner = Some(ObjectId::from_parts(owner.as_ref(), kind.as_str(), name));
        }
        owner
    }
}
