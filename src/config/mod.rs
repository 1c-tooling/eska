//! Project manifests and machine-local runner configuration.

pub(crate) use eska_build_runtime::config::{
    EditOutcome as GlobalConfigEditOutcome, GlobalConfigError,
    InitOutcome as GlobalConfigInitOutcome, RunnerKind, config_path, edit_at as edit_global_at,
    init_at as init_global_at, load as load_global,
};
pub(crate) use eska_project_core::config::parse_project_type;
pub use eska_project_core::config::{
    InvalidMemberPathReason, InvalidSourceReason, ManifestConfig, ManifestConfigError,
    ProjectConfig, ProjectConfigError, WorkspaceConfig, WorkspaceConfigError,
};
