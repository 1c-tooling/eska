//! Owned build staging and empty-directory cleanup never claim existing user directories.

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use super::BuildError;

static WORKSPACE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Produce a collision-resistant sibling name without inspecting user content.
pub(super) fn unique_sibling(path: &Path, label: &str) -> PathBuf {
    let sequence = WORKSPACE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".eska-{label}-{}-{sequence}", std::process::id()));
    path.with_file_name(name)
}

pub(super) struct CreatedDirectories {
    paths: Vec<PathBuf>,
    keep: bool,
}

impl CreatedDirectories {
    /// Create the output hierarchy and remember only directories owned by this run.
    pub(super) fn create(parent: &Path) -> Result<Self, BuildError> {
        let mut paths = Vec::new();
        let mut current = parent;
        while !current.exists() {
            paths.push(current.to_owned());
            current = current
                .parent()
                .ok_or_else(|| BuildError::OutputParentMissing(parent.to_owned()))?;
        }
        fs::create_dir_all(parent).map_err(|source| BuildError::CreateDirectory {
            path: parent.to_owned(),
            source,
        })?;
        Ok(Self { paths, keep: false })
    }

    /// Preserve the output directories after successful artifact publication.
    pub(super) const fn keep(&mut self) {
        self.keep = true;
    }
}

impl Drop for CreatedDirectories {
    /// Roll back only empty directories created by the active build.
    fn drop(&mut self) {
        if !self.keep {
            for directory in &self.paths {
                let _ = fs::remove_dir(directory);
            }
        }
    }
}

pub(super) struct Workspace {
    pub(super) path: PathBuf,
}

impl Workspace {
    /// Create one private workspace next to the destination for same-filesystem publication.
    pub(super) fn create(parent: &Path) -> Result<Self, BuildError> {
        for _ in 0..32 {
            let path = unique_sibling(&parent.join("build"), "work");
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(source) => return Err(BuildError::CreateWorkspace { path, source }),
            }
        }
        let path = parent.join(".eska-build-work");
        Err(BuildError::CreateWorkspace {
            path,
            source: io::Error::new(io::ErrorKind::AlreadyExists, "workspace name exhausted"),
        })
    }
}

impl Drop for Workspace {
    /// Remove snapshots and unpublished artifacts while retaining the managed infobase.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
