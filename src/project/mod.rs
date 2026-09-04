//! Locale-independent project model and filesystem operations.

pub mod clone;
pub mod create;
mod designer_xml;
pub mod diff;
pub mod discovery;
pub mod init;
pub(crate) mod metadata;
pub mod model;
pub mod save;
pub mod start;
pub mod status;
pub mod templates;

pub use model::{
    InvalidPathReason, Project, ProjectConfiguration, ProjectPath, ProjectPathError, ProjectType,
    SourceFormat,
};
