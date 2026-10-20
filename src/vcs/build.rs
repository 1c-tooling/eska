//! VCS orchestration supplies provenance and source-change checks to the independent build runtime.

use std::path::Path;

use crate::project::{Project, artifact::ArtifactError, build::GitProvenance};

/// Capture repository metadata through the same owner used by the standalone VCS CLI.
pub fn provenance(root: &Path) -> GitProvenance {
    eska_vcs::bridge::provenance(root)
}

/// Preserve the import callback category while the process client is being extracted.
pub fn source_changes(
    project: &Project,
    empty_scaffold: bool,
    files: usize,
) -> Result<usize, ArtifactError> {
    eska_vcs::bridge::source_changes(project, empty_scaffold, files)
        .map_err(|_| ArtifactError::ChangeInspection)
}
