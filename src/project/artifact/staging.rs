//! Exclusively owned temporary directories beside their eventual destination.

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use super::{ArtifactError, io_error};

pub(super) struct Staging {
    pub path: PathBuf,
}

impl Staging {
    /// Claim a unique directory without reusing any pre-existing user data.
    pub fn create(parent: &Path) -> Result<Self, ArtifactError> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for attempt in 0..1024 {
            let path = parent.join(format!(
                ".eska-import-{}-{stamp}-{attempt}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(io_error(&path, error)),
            }
        }
        Err(io_error(
            parent,
            io::Error::from(io::ErrorKind::AlreadyExists),
        ))
    }
}

impl Drop for Staging {
    /// Remove only the directory exclusively claimed by this operation.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
