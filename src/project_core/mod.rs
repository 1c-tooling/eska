//! Shared project values and source operations; no Git, CLI, IDE or platform runner.

mod metadata_kind;
pub mod native;
mod project_type;
pub mod publish;
pub mod source_format;
pub mod support;

pub use metadata_kind::{MetadataKind, UnknownMetadataKind};
pub use project_type::ProjectType;
