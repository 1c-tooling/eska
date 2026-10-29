//! Strict project and workspace manifests; machine-local runner configuration is separate.

pub(crate) mod composition;
mod manifest;
mod paths;
mod project;
mod schema;
mod workflow;
mod workspace;

pub use manifest::{ManifestConfig, ManifestConfigError};
pub use project::{InvalidSourceReason, ProjectConfig, ProjectConfigError};
pub use schema::parse_project_type;
pub use workspace::{InvalidMemberPathReason, WorkspaceConfig, WorkspaceConfigError};

/// Name shared by project discovery and manifest writers.
pub const FILE_NAME: &str = "eska.toml";
