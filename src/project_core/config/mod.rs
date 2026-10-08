//! Shared strict project/workspace manifests without machine-local settings or Git operations.

mod manifest;
mod project;
mod schema;
mod workflow;
mod workspace;

pub use manifest::{ManifestConfig, ManifestConfigError};
pub use project::{InvalidSourceReason, ProjectConfig, ProjectConfigError};
pub use schema::parse_project_type;
pub use workspace::{InvalidMemberPathReason, WorkspaceConfig, WorkspaceConfigError};

/// The authoritative project or workspace manifest filename.
pub const FILE_NAME: &str = "eska.toml";
