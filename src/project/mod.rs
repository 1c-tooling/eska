//! Locale-independent project model and filesystem operations.

pub mod create;
pub mod discovery;
pub mod init;
pub mod model;
pub mod templates;

pub use model::{
    InvalidPathReason, Project, ProjectConfiguration, ProjectPath, ProjectPathError, ProjectType,
    SourceFormat,
};
