//! Shared project manifests and machine-local settings retain the application's existing API.

pub(crate) use crate::build_core::machine::{
    EditOutcome as GlobalConfigEditOutcome, GlobalConfigError,
    InitOutcome as GlobalConfigInitOutcome, config_path, edit_at as edit_global_at,
    init_at as init_global_at,
};
pub(crate) use crate::project_core::config::{FILE_NAME, parse_project_type};
pub use crate::project_core::config::{
    InvalidMemberPathReason, InvalidSourceReason, ManifestConfig, ManifestConfigError,
    ProjectConfig, ProjectConfigError, WorkspaceConfig, WorkspaceConfigError,
};
