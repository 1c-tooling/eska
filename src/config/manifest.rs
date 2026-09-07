//! Classification of strict project and workspace manifests.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use super::{ProjectConfig, ProjectConfigError, WorkspaceConfig, WorkspaceConfigError};

#[derive(Deserialize)]
struct ManifestKind {
    project: Option<toml::Value>,
    workspace: Option<toml::Value>,
}

/// A strict `eska.toml` manifest kind.
#[derive(Debug)]
pub enum ManifestConfig {
    Project(ProjectConfig),
    Workspace(WorkspaceConfig),
}

impl ManifestConfig {
    /// Loads, classifies, and validates one `eska.toml` file.
    ///
    /// # Errors
    /// Returns an I/O, classification, or strict schema error.
    pub fn load(path: &Path) -> Result<Self, ManifestConfigError> {
        let input = fs::read_to_string(path).map_err(|source| ManifestConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_toml(&input)
    }

    /// Classifies a manifest before parsing its strict schema.
    ///
    /// # Errors
    /// Returns an error when exactly one of `[project]` and `[workspace]` is not present.
    pub fn from_toml(input: &str) -> Result<Self, ManifestConfigError> {
        let kind: ManifestKind = toml::from_str(input).map_err(ManifestConfigError::Toml)?;
        match (kind.project.is_some(), kind.workspace.is_some()) {
            (true, false) => ProjectConfig::from_toml(input)
                .map(Self::Project)
                .map_err(ManifestConfigError::Project),
            (false, true) => WorkspaceConfig::from_toml(input)
                .map(Self::Workspace)
                .map_err(ManifestConfigError::Workspace),
            (false, false) => Err(ManifestConfigError::KindMissing),
            (true, true) => Err(ManifestConfigError::KindAmbiguous),
        }
    }
}

/// A structured manifest loading or classification error.
#[derive(Debug)]
pub enum ManifestConfigError {
    Io { path: PathBuf, source: io::Error },
    Toml(toml::de::Error),
    KindMissing,
    KindAmbiguous,
    Project(ProjectConfigError),
    Workspace(WorkspaceConfigError),
}

#[cfg(test)]
mod tests {
    use super::{ManifestConfig, ManifestConfigError};

    #[test]
    fn manifest_requires_exactly_one_root_kind() {
        assert!(matches!(
            ManifestConfig::from_toml("[build]\nplatform_version = ''\n"),
            Err(ManifestConfigError::KindMissing)
        ));
        assert!(matches!(
            ManifestConfig::from_toml("[project]\ntype = 'report'\n[workspace]\nmembers = []\n"),
            Err(ManifestConfigError::KindAmbiguous)
        ));
    }
}
