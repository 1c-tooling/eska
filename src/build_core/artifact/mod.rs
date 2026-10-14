//! Operation-owned native unpacking; project selection and source replacement belong to callers.

mod export;
mod publication;
pub(crate) mod snapshot;
mod source_import;
pub(crate) mod staging;
mod unpack;

use std::{
    io,
    path::{Path, PathBuf},
};

use super::RunError;
use crate::project_core::native::{Identity, IdentityError};

pub use publication::unpack_destination;
pub use source_import::{ArtifactSourceImportError, PreparedArtifactSourceImport};
pub use unpack::PreparedArtifact;

/// Unpacking never needs project settings, Git state or an overwrite decision.
#[derive(Debug)]
pub enum UnpackError {
    Io { path: PathBuf, source: io::Error },
    UnsupportedFile(PathBuf),
    InvalidDescriptor(PathBuf),
    MissingDescriptor(PathBuf),
    AmbiguousDescriptor(PathBuf),
    UnsafePath(PathBuf),
    DestinationExists(PathBuf),
    SourceChanged(PathBuf),
    Run(RunError),
    Platform { stage: &'static str, output: String },
}

impl From<IdentityError> for UnpackError {
    /// Preserve descriptor failure categories across the shared native identity boundary.
    fn from(error: IdentityError) -> Self {
        match error {
            IdentityError::Io { path, source } => Self::Io { path, source },
            IdentityError::InvalidDescriptor(path) => Self::InvalidDescriptor(path),
            IdentityError::AmbiguousDescriptor(path) => Self::AmbiguousDescriptor(path),
            IdentityError::UnsafePath(path) => Self::UnsafePath(path),
        }
    }
}

/// Preserve path context at every filesystem boundary.
fn io_error(path: &Path, source: io::Error) -> UnpackError {
    UnpackError::Io {
        path: path.to_owned(),
        source,
    }
}
