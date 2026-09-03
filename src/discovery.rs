//! Filesystem-backed project discovery, independent of CLI presentation.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::{config::ProjectConfig, config::ProjectConfigError, project::Project};

const CONFIG_FILE: &str = "eska.toml";

/// Finds and validates the nearest project starting from a directory.
///
/// Relative paths are resolved against the current working directory. Symlinks
/// are resolved before walking ancestors and before checking source containment.
/// The first configuration encountered is authoritative, even when invalid.
///
/// # Errors
///
/// Returns a structured error for an inaccessible or invalid directory, a missing
/// project, invalid configuration, or sources outside the project root.
pub fn discover(start: &Path) -> Result<Project, DiscoveryError> {
    let start = canonicalize(start)?;
    if !metadata(&start)?.is_dir() {
        return Err(DiscoveryError::StartNotDirectory { path: start });
    }

    for root in start.ancestors() {
        let path = root.join(CONFIG_FILE);
        // A dangling link is a broken nearest config, not permission to fall back
        // to another project higher in the directory tree.
        match fs::symlink_metadata(&path) {
            Ok(_) => return load_project(root, &path),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(source) => return Err(DiscoveryError::Io { path, source }),
        }
    }

    Err(DiscoveryError::NotFound { start })
}

fn load_project(root: &Path, path: &Path) -> Result<Project, DiscoveryError> {
    // Reject directories and special files before reading them (e.g. FIFOs).
    if !metadata(path)?.is_file() {
        return Err(DiscoveryError::ConfigNotFile {
            path: path.to_path_buf(),
        });
    }

    let config_error = |source| DiscoveryError::Config {
        path: path.to_path_buf(),
        source,
    };
    let project = ProjectConfig::load(path)
        .and_then(|config| config.into_project(root.to_path_buf()))
        .map_err(config_error)?;

    if !metadata(project.source())?.is_dir() {
        return Err(DiscoveryError::SourceNotDirectory {
            path: project.source().to_path_buf(),
        });
    }
    let source = canonicalize(project.source())?;
    Project::new(root.to_path_buf(), source, *project.configuration())
        .map_err(ProjectConfigError::ProjectPath)
        .map_err(config_error)
}

fn canonicalize(path: &Path) -> Result<PathBuf, DiscoveryError> {
    fs::canonicalize(path).map_err(|source| DiscoveryError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn metadata(path: &Path) -> Result<fs::Metadata, DiscoveryError> {
    fs::metadata(path).map_err(|source| DiscoveryError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Structured discovery failures; localized text belongs to the CLI layer.
#[derive(Debug)]
pub enum DiscoveryError {
    NotFound {
        start: PathBuf,
    },
    Io {
        path: PathBuf,
        source: io::Error,
    },
    StartNotDirectory {
        path: PathBuf,
    },
    ConfigNotFile {
        path: PathBuf,
    },
    SourceNotDirectory {
        path: PathBuf,
    },
    Config {
        path: PathBuf,
        source: ProjectConfigError,
    },
}
