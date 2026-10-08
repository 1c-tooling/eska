//! Project manifests share their schema; machine-local settings remain in the application.

mod global;

pub(crate) use crate::project_core::config::{FILE_NAME, parse_project_type};
pub use crate::project_core::config::{
    InvalidMemberPathReason, InvalidSourceReason, ManifestConfig, ManifestConfigError,
    ProjectConfig, ProjectConfigError, WorkspaceConfig, WorkspaceConfigError,
};
pub(crate) use global::{
    EditOutcome as GlobalConfigEditOutcome, GlobalConfigError,
    InitOutcome as GlobalConfigInitOutcome, RunnerKind, config_path, edit_at as edit_global_at,
    init_at as init_global_at, load as load_global,
};
