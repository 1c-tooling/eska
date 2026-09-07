//! Reading and writing the strict `eska.toml` configuration format.

mod global;
mod manifest;
mod project;
mod schema;
mod workflow;
mod workspace;

pub(crate) use global::{
    EditOutcome as GlobalConfigEditOutcome, GlobalConfigError,
    InitOutcome as GlobalConfigInitOutcome, RunnerKind, config_path, edit_at as edit_global_at,
    init_at as init_global_at, load as load_global,
};
pub use manifest::{ManifestConfig, ManifestConfigError};
pub use project::{InvalidSourceReason, ProjectConfig, ProjectConfigError};
pub(crate) use schema::parse_project_type;
pub use workspace::{InvalidMemberPathReason, WorkspaceConfig, WorkspaceConfigError};

pub(crate) const FILE_NAME: &str = "eska.toml";
