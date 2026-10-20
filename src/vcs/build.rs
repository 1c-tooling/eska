//! Import preflight queries the VCS owner without interpreting repository state locally.

use crate::project::{Project, artifact::ArtifactError};
use eska_tool_runtime::{Component, query, resolve};
use std::ffi::OsString;

/// Preserve the import callback category and re-query the selected source before publication.
pub fn source_changes(
    project: &Project,
    empty_scaffold: bool,
    files: usize,
) -> Result<usize, ArtifactError> {
    let binary = resolve(Component::Vcs).map_err(|_| ArtifactError::ChangeInspection)?;
    let mut options = vec![OsString::from("--files"), OsString::from(files.to_string())];
    if empty_scaffold {
        options.push(OsString::from("--empty-scaffold"));
    }
    let data = query(
        Component::Vcs,
        &binary,
        "source_changes",
        project.root(),
        &options,
    )
    .map_err(|error| match error {
        eska_tool_runtime::Error::Cancelled => ArtifactError::Cancelled,
        _ => ArtifactError::ChangeInspection,
    })?;
    data.get("local_changes")
        .and_then(serde_json::Value::as_u64)
        .and_then(|count| usize::try_from(count).ok())
        .ok_or(ArtifactError::ChangeInspection)
}
