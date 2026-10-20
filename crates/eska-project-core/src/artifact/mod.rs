//! Designer XML identity and exclusively owned ignored staging, without native tools or Git.

mod identity;
mod staging;

pub use identity::{Identity, inspect_identity};
pub use staging::Staging;
use std::{io, path::PathBuf};

/// Structural failures carry paths; each command owner supplies presentation and native context.
#[derive(Debug)]
pub enum Error {
    Io { path: PathBuf, source: io::Error },
    InvalidDescriptor(PathBuf),
    AmbiguousDescriptor(PathBuf),
    UnsafePath(PathBuf),
}

/// Retain filesystem location without introducing user-facing messages into the core.
fn io_error(path: &std::path::Path, source: io::Error) -> Error {
    Error::Io {
        path: path.to_owned(),
        source,
    }
}
