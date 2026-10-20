//! Native operations use the same bounded Designer identity reader as project onboarding.

use super::ArtifactError;
pub use eska_project_core::artifact::Identity;

/// Inspect XML identity while preserving native command error categories.
///
/// # Errors
/// Rejects malformed, ambiguous and unsafe roots using the shared structural reader.
pub fn inspect_identity(directory: &std::path::Path) -> Result<Option<Identity>, ArtifactError> {
    eska_project_core::artifact::inspect_identity(directory).map_err(super::structural_error)
}
