//! Shared process-boundary selection, without a second discovery or workspace policy.

use std::path::Path;

use super::Failure;
use crate::project_core::{
    Project,
    discovery::{DiscoveryContext, discover_context},
    metadata_model::ProjectScope,
    selection::{SelectionIntent, select_projects},
};

/// Keep member identity and artifact scope alongside its inherited project settings.
pub struct Selected {
    pub project: Project,
    pub scope: ProjectScope,
    #[cfg(feature = "build-cli")]
    pub member: Option<crate::project_core::ProjectName>,
    #[cfg(feature = "build-cli")]
    pub workspace_root: Option<std::path::PathBuf>,
}

/// Resolve one member through the same shared selector used by the application and offline CLI.
///
/// # Errors
/// Returns failed discovery, ambiguous/unknown selection or a non-UTF-8 output coordinate.
pub fn selected(start: &Path, requested: Option<&str>) -> Result<Selected, Failure> {
    let context = discover_context(start)
        .map_err(|_| Failure::new("project_discovery_failed").at(start.to_string_lossy()))?;
    let names: Vec<_> = requested.into_iter().map(str::to_owned).collect();
    let selection = select_projects(&context, &names, false, SelectionIntent::SingleMutation)
        .map_err(|_| Failure::new("project_selection_failed"))?;
    let item = selection
        .projects()
        .first()
        .ok_or_else(|| Failure::new("project_selection_failed"))?;
    path_text(item.project().root())?;
    let scope = item.name().map_or(ProjectScope::Standalone, |name| {
        ProjectScope::Member(name.clone())
    });
    let workspace_root = match &context {
        DiscoveryContext::Standalone(_) => None,
        DiscoveryContext::Workspace { workspace, .. } => Some(workspace.root().to_owned()),
    };
    if let Some(root) = &workspace_root {
        path_text(root)?;
    }
    Ok(Selected {
        project: item.project().clone(),
        scope,
        #[cfg(feature = "build-cli")]
        member: item.name().cloned(),
        #[cfg(feature = "build-cli")]
        workspace_root,
    })
}

/// Machine paths must retain all bytes; unsupported encodings fail before mutation.
///
/// # Errors
/// Returns `path_encoding_unsupported` when the selected path is not valid UTF-8.
pub fn path_text(path: &Path) -> Result<&str, Failure> {
    path.to_str()
        .ok_or_else(|| Failure::new("path_encoding_unsupported"))
}
