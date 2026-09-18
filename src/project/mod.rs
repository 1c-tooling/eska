//! Locale-independent project model and filesystem operations.

pub mod build;
pub mod clone;
pub mod create;
pub mod designer_source;
mod designer_xml;
pub mod diff;
pub mod discovery;
pub mod doctor;
pub mod finish;
pub mod history;
pub mod init;
pub(crate) mod metadata;
pub mod metadata_model;
pub mod metadata_parser;
pub mod metadata_workspace;
pub mod model;
pub mod object_model;
pub mod onboarding;
pub mod patch;
pub mod save;
pub mod selection;
pub mod semantic;
pub mod start;
pub mod status;
pub mod switch;
pub mod templates;
pub mod version;
mod workspace;

pub use model::{
    InvalidPathReason, Project, ProjectConfiguration, ProjectPath, ProjectPathError, ProjectType,
    SourceFormat,
};
pub use workspace::{ProjectName, ProjectNameError, Workspace, WorkspaceMember};

pub mod configurator;
