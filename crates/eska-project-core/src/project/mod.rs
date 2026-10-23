//! Project settings and Designer XML operations without Git, platform or CLI runtimes.

pub mod build;
pub mod configurator;
pub mod create;
pub mod designer_source;
pub mod designer_xml;
pub mod discovery;
pub mod init;
pub mod metadata;
mod metadata_disk_cache;
pub mod metadata_edit;
pub mod metadata_model;
pub mod metadata_parser;
pub mod metadata_rename;
pub mod metadata_workspace;
pub mod model;
pub mod object_model;
pub mod onboarding;
pub mod query;
pub mod selection;
pub mod support;
pub mod templates;
pub mod validation;
pub mod version;
mod workspace;

pub use model::{
    InvalidPathReason, Project, ProjectConfiguration, ProjectPath, ProjectPathError, ProjectType,
    SourceFormat,
};
pub use workspace::{ProjectName, ProjectNameError, Workspace, WorkspaceMember};
