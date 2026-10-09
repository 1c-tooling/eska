//! Shared project values and source operations; no Git, CLI, IDE or platform runner.

mod metadata_kind;
pub mod metadata_model;
pub mod metadata_parser;
pub mod native;
pub mod native_version;
mod project_type;
pub mod publish;
pub mod source_format;
pub mod support;
pub mod transport;

pub use metadata_kind::{MetadataKind, UnknownMetadataKind};
pub use project_type::ProjectType;

pub mod config;
pub mod discovery;
pub mod eska_source;
mod model;
pub mod platform_settings;
pub mod selection;
pub mod workflow;
mod workspace;

pub use model::{
    InvalidPathReason, Project, ProjectConfiguration, ProjectPath, ProjectPathError, SourceFormat,
};
pub use workspace::{ProjectName, ProjectNameError, Workspace, WorkspaceMember};
