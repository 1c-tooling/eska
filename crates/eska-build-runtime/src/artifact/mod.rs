//! Native 1C artifacts unpacked into isolated, validated Designer XML sources.

mod identity;
mod replace;
mod snapshot;
mod unpack;

use std::{io, path::PathBuf};

use crate::build::RunError;
use eska_project_core::project::ProjectType;

pub use identity::{Identity, inspect_identity};
pub use replace::{ImportPlan, ImportResult, SourceChanges};
pub use unpack::{PreparedArtifact, new_destination};

/// Publication keeps cancellation distinct from native filesystem errors after rollback completes.
#[derive(Debug)]
pub enum PublicationError {
    Cancelled,
    Io(io::Error),
}

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
    Cancelled,
    Publication {
        path: PathBuf,
        source: Box<eska_project_core::project::create::CreationError<PublicationError>>,
    },
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

/// Preserve existing native error codes around the shared structural implementation.
fn structural_error(error: eska_project_core::artifact::Error) -> ArtifactError {
    use eska_project_core::artifact::Error;
    match error {
        Error::Io { path, source } => ArtifactError::Io { path, source },
        Error::InvalidDescriptor(path) => ArtifactError::InvalidDescriptor(path),
        Error::AmbiguousDescriptor(path) => ArtifactError::AmbiguousDescriptor(path),
        Error::UnsafePath(path) => ArtifactError::UnsafePath(path),
    }
}
