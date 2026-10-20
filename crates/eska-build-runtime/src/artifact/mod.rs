//! Native 1C artifacts unpacked into isolated, validated Designer XML sources.

mod identity;
mod replace;
mod snapshot;
mod staging;
mod unpack;

use std::{io, path::PathBuf};

use crate::build::RunError;
use eska_project_core::project::ProjectType;

pub use identity::{Identity, inspect_identity};
pub use replace::{ImportPlan, ImportResult, SourceChanges};
pub use unpack::PreparedArtifact;

/// Import failures retain their operation and affected path without presentation text.
#[derive(Debug)]
pub enum ArtifactError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    UnsupportedFile(PathBuf),
    InvalidDescriptor(PathBuf),
    MissingDescriptor(PathBuf),
    AmbiguousDescriptor(PathBuf),
    UnsafePath(PathBuf),
    Config(eska_project_core::config::ProjectConfigError),
    ChangeInspection,
    ConfirmationRequired,
    SourceChanged(PathBuf),
    Rollback {
        backup: PathBuf,
        original: io::Error,
        restore: io::Error,
    },
    TypeMismatch {
        expected: ProjectType,
        actual: ProjectType,
    },
    Run(RunError),
    Output(io::Error),
    Platform {
        stage: &'static str,
        output: String,
    },
}

/// Preserve path context at every filesystem boundary.
fn io_error(path: &std::path::Path, source: io::Error) -> ArtifactError {
    ArtifactError::Io {
        path: path.to_owned(),
        source,
    }
}
