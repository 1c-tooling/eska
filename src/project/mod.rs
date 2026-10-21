//! Locale-independent project model and filesystem operations.

pub mod artifact;
pub mod build;
pub mod clone;
pub mod create;
pub mod diff;
pub mod doctor;
pub mod finish;
pub mod history;
pub mod init;
pub mod onboarding;
pub mod patch;
pub mod save;
pub mod semantic;
pub mod start;
pub mod status;
pub mod switch;

pub use eska_project_core::project::model::{
    InvalidPathReason, Project, ProjectConfiguration, ProjectPath, ProjectPathError, ProjectType,
    SourceFormat,
};
pub use eska_project_core::project::{ProjectName, ProjectNameError, Workspace, WorkspaceMember};

pub use eska_project_core::project::{
    configurator, designer_source, discovery, metadata_edit, metadata_model, metadata_parser,
    metadata_rename, metadata_workspace, model, object_model, selection, support, templates,
    version,
};
