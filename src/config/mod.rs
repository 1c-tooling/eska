//! Project manifests shared with the standalone project tools.

pub(crate) use eska_project_core::config::parse_project_type;
pub use eska_project_core::config::{
    InvalidMemberPathReason, InvalidSourceReason, ManifestConfig, ManifestConfigError,
    ProjectConfig, ProjectConfigError, WorkspaceConfig, WorkspaceConfigError,
};
