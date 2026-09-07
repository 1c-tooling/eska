//! Strict root workspace configuration and member path validation.

use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    project::build::{BuildSettings, BuildSettingsError},
    vcs::workflow::WorkflowSettings,
};

use super::{
    ProjectConfigError,
    schema::{RawBuild, RawVcs, SerializedBuild, SerializedVcs},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWorkspaceDocument {
    workspace: RawWorkspace,
    build: Option<RawBuild>,
    vcs: Option<RawVcs>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWorkspace {
    members: Vec<PathBuf>,
}

#[derive(Serialize)]
struct SerializedWorkspaceDocument<'a> {
    workspace: SerializedWorkspace<'a>,
    build: SerializedBuild<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vcs: Option<SerializedVcs<'a>>,
}

#[derive(Serialize)]
struct SerializedWorkspace<'a> {
    members: &'a [PathBuf],
}

/// The validated contents of a root workspace `eska.toml` file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceConfig {
    members: Vec<PathBuf>,
    build: BuildSettings,
    workflow: Option<WorkflowSettings>,
}

impl WorkspaceConfig {
    /// Loads and validates a root workspace manifest.
    ///
    /// # Errors
    /// Returns a structured I/O, TOML, build, workflow, or member path error.
    pub fn load(path: &Path) -> Result<Self, WorkspaceConfigError> {
        let input = fs::read_to_string(path).map_err(|source| WorkspaceConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_toml(&input)
    }

    /// Parses and validates a root workspace manifest.
    ///
    /// # Errors
    /// Returns an error for malformed fields or unsafe member paths.
    pub fn from_toml(input: &str) -> Result<Self, WorkspaceConfigError> {
        let document: RawWorkspaceDocument =
            toml::from_str(input).map_err(WorkspaceConfigError::Toml)?;
        for member in &document.workspace.members {
            validate_member_path(member)?;
        }

        let defaults = BuildSettings::default();
        let raw_build = document.build.unwrap_or_default();
        let build = BuildSettings::new(
            raw_build.platform_version.as_deref().unwrap_or_default(),
            raw_build
                .artifacts_directory
                .unwrap_or_else(|| defaults.artifacts_directory().to_owned()),
        )
        .map_err(WorkspaceConfigError::InvalidBuild)?;
        let workflow = document
            .vcs
            .map(|vcs| super::workflow::parse(vcs.workflow))
            .transpose()
            .map_err(WorkspaceConfigError::InvalidWorkflow)?;

        Ok(Self {
            members: document.workspace.members,
            build,
            workflow,
        })
    }

    /// Serializes a canonical root workspace manifest.
    ///
    /// # Errors
    /// Returns an error when TOML serialization fails.
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        let defaults = BuildSettings::default();
        let document = SerializedWorkspaceDocument {
            workspace: SerializedWorkspace {
                members: &self.members,
            },
            build: SerializedBuild {
                platform_version: self
                    .build
                    .platform_version()
                    .map_or("", crate::project::build::PlatformVersion::as_str),
                artifacts_directory: (self.build.artifacts_directory()
                    != defaults.artifacts_directory())
                .then(|| self.build.artifacts_directory()),
            },
            vcs: self.workflow.as_ref().map(|settings| SerializedVcs {
                workflow: super::workflow::serialize(settings),
            }),
        };
        toml::to_string_pretty(&document)
    }

    #[must_use]
    pub fn members(&self) -> &[PathBuf] {
        &self.members
    }

    #[must_use]
    pub const fn build_settings(&self) -> &BuildSettings {
        &self.build
    }

    #[must_use]
    pub const fn workflow_settings(&self) -> Option<&WorkflowSettings> {
        self.workflow.as_ref()
    }
}

/// The reason a workspace member path was rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidMemberPathReason {
    Empty,
    Absolute,
    ContainsParentTraversal,
}

/// A structured root workspace configuration error.
#[derive(Debug)]
pub enum WorkspaceConfigError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Toml(toml::de::Error),
    InvalidBuild(BuildSettingsError),
    InvalidWorkflow(ProjectConfigError),
    InvalidMemberPath {
        path: PathBuf,
        reason: InvalidMemberPathReason,
    },
}

fn validate_member_path(path: &Path) -> Result<(), WorkspaceConfigError> {
    let reason = if path.as_os_str().is_empty() {
        Some(InvalidMemberPathReason::Empty)
    } else if path.is_absolute() {
        Some(InvalidMemberPathReason::Absolute)
    } else if path
        .components()
        .any(|component| component == Component::ParentDir)
    {
        Some(InvalidMemberPathReason::ContainsParentTraversal)
    } else {
        None
    };
    reason.map_or(Ok(()), |reason| {
        Err(WorkspaceConfigError::InvalidMemberPath {
            path: path.to_path_buf(),
            reason,
        })
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{WorkspaceConfig, WorkspaceConfigError};

    #[test]
    fn workspace_settings_and_members_round_trip() {
        let input = r#"[workspace]
members = ["src/report", "src/processing"]

[build]
platform_version = "8.3.27.2325"
artifacts_directory = "dist"

[vcs.workflow]
preset = "trunk"
"#;
        let config = WorkspaceConfig::from_toml(input).unwrap();
        assert_eq!(config.members().len(), 2);
        assert_eq!(
            config.build_settings().artifacts_directory(),
            Path::new("dist")
        );
        assert_eq!(
            WorkspaceConfig::from_toml(&config.to_toml().unwrap()).unwrap(),
            config
        );
    }

    #[test]
    fn workspace_rejects_unsafe_member_paths_and_project_sections() {
        for member in ["", "/absolute", "src/../outside"] {
            let result =
                WorkspaceConfig::from_toml(&format!("[workspace]\nmembers = ['{member}']\n"));
            assert!(matches!(
                result,
                Err(WorkspaceConfigError::InvalidMemberPath { .. })
            ));
        }
        assert!(matches!(
            WorkspaceConfig::from_toml("[workspace]\nmembers = []\n[project]\ntype = 'report'\n"),
            Err(WorkspaceConfigError::Toml(_))
        ));
    }
}
