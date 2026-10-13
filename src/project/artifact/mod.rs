//! Native 1C artifacts unpacked into isolated, validated Designer XML sources.

mod replace;

use std::{io, path::PathBuf};

use super::{ProjectType, build::RunError};

pub use crate::build_core::artifact::PreparedArtifact;
pub use crate::project_core::native::{Identity, inspect_identity};
pub use replace::{ImportPlan, ImportResult};

/// Import failures retain their operation and affected path without presentation text.
#[derive(Debug)]
pub enum ArtifactError {
    SourceFormatUnsupported,
    Io {
        path: PathBuf,
        source: io::Error,
    },
    UnsupportedFile(PathBuf),
    InvalidDescriptor(PathBuf),
    MissingDescriptor(PathBuf),
    AmbiguousDescriptor(PathBuf),
    UnsafePath(PathBuf),
    Config(crate::config::ProjectConfigError),
    Repository(crate::vcs::repository::Error),
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

impl From<crate::build_core::artifact::UnpackError> for ArtifactError {
    /// Keep the installed CLI's error contract while native execution lives in build core.
    fn from(error: crate::build_core::artifact::UnpackError) -> Self {
        use crate::build_core::artifact::UnpackError;
        match error {
            UnpackError::Io { path, source } => Self::Io { path, source },
            UnpackError::UnsupportedFile(path) => Self::UnsupportedFile(path),
            UnpackError::InvalidDescriptor(path) => Self::InvalidDescriptor(path),
            UnpackError::MissingDescriptor(path) => Self::MissingDescriptor(path),
            UnpackError::AmbiguousDescriptor(path) => Self::AmbiguousDescriptor(path),
            UnpackError::UnsafePath(path) | UnpackError::DestinationExists(path) => {
                Self::UnsafePath(path)
            }
            UnpackError::SourceChanged(path) => Self::SourceChanged(path),
            UnpackError::Run(source) => Self::Run(source),
            UnpackError::Platform { stage, output } => Self::Platform { stage, output },
        }
    }
}

impl From<crate::project_core::native::IdentityError> for ArtifactError {
    /// Identity checks remain available independently of the platform runner.
    fn from(error: crate::project_core::native::IdentityError) -> Self {
        crate::build_core::artifact::UnpackError::from(error).into()
    }
}
