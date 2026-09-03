//! Reading and writing the strict `eska.toml` configuration format.

mod project;
mod schema;

pub use project::{InvalidSourceReason, ProjectConfig, ProjectConfigError};
pub(crate) use schema::parse_project_type;

pub(crate) const FILE_NAME: &str = "eska.toml";
