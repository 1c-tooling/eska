//! Related schemas are read only by their declared metadata identity.

use super::super::{ProjectSession, QuerySessionError, WorkspaceError, workspace_error};
use crate::project::metadata_model::{MetadataKind, NodeId};

/// An absent relation yields no invented fields; a declared unreadable source remains a real error.
pub(super) fn descriptor(
    project: &ProjectSession,
    kind: MetadataKind,
    reference: &str,
) -> Result<Option<String>, QuerySessionError> {
    let name = reference.rsplit('.').next().unwrap_or(reference);
    let Some(object) = project
        .objects
        .values()
        .find(|object| object.kind == kind && object.name == name)
    else {
        return Ok(None);
    };
    let missing = || {
        workspace_error(WorkspaceError::MissingSource(NodeId::Object(
            object.id.clone(),
        )))
    };
    let location = project
        .source
        .object_descriptor(&object.id)
        .map_err(|error| workspace_error(WorkspaceError::Source(error)))?
        .ok_or_else(missing)?;
    project
        .source
        .read_xml(&location.path)
        .map_err(|error| workspace_error(WorkspaceError::Source(error)))?
        .ok_or_else(missing)
        .map(Some)
}
