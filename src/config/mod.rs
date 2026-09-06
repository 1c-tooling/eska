//! Reading and writing the strict `eska.toml` configuration format.

mod global;
mod project;
mod schema;
mod workflow;

pub(crate) use global::{
    EditOutcome as GlobalConfigEditOutcome, GlobalConfigError,
    InitOutcome as GlobalConfigInitOutcome, RunnerKind, config_path, edit_at as edit_global_at,
    init_at as init_global_at, load as load_global,
};
pub use project::{InvalidSourceReason, ProjectConfig, ProjectConfigError};
pub(crate) use schema::parse_project_type;

pub(crate) const FILE_NAME: &str = "eska.toml";
