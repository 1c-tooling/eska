use std::path::{Component, Path, PathBuf};

pub const DEFAULT_PLATFORM_VERSION: &str = "8.3.27.2325";
pub const DEFAULT_ARTIFACTS_DIRECTORY: &str = "build";

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PlatformVersion(String);

impl PlatformVersion {
    /// Parse an exact four-component numeric 1C platform version.
    ///
    /// # Errors
    /// Returns the original value when it is not in `major.minor.patch.build` form.
    pub fn parse(value: &str) -> Result<Self, BuildSettingsError> {
        let components: Vec<_> = value.split('.').collect();
        if components.len() != 4
            || components.iter().any(|component| {
                component.is_empty()
                    || !component.bytes().all(|byte| byte.is_ascii_digit())
                    || component.parse::<u32>().is_err()
            })
        {
            return Err(BuildSettingsError::InvalidPlatformVersion {
                value: value.to_owned(),
            });
        }
        Ok(Self(value.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BuildSettings {
    platform_version: PlatformVersion,
    artifacts_directory: PathBuf,
}

impl Default for BuildSettings {
    fn default() -> Self {
        Self {
            platform_version: PlatformVersion(DEFAULT_PLATFORM_VERSION.to_owned()),
            artifacts_directory: PathBuf::from(DEFAULT_ARTIFACTS_DIRECTORY),
        }
    }
}

impl BuildSettings {
    /// Validate portable project build settings.
    ///
    /// # Errors
    /// Returns a structured error for an invalid platform version or unsafe artifacts path.
    pub fn new(
        platform_version: &str,
        artifacts_directory: PathBuf,
    ) -> Result<Self, BuildSettingsError> {
        let platform_version = PlatformVersion::parse(platform_version)?;
        validate_artifacts_directory(&artifacts_directory)?;
        Ok(Self {
            platform_version,
            artifacts_directory,
        })
    }

    #[must_use]
    pub const fn platform_version(&self) -> &PlatformVersion {
        &self.platform_version
    }

    #[must_use]
    pub fn artifacts_directory(&self) -> &Path {
        &self.artifacts_directory
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidArtifactsDirectoryReason {
    Empty,
    Absolute,
    ContainsParentTraversal,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildSettingsError {
    InvalidPlatformVersion {
        value: String,
    },
    InvalidArtifactsDirectory {
        path: PathBuf,
        reason: InvalidArtifactsDirectoryReason,
    },
}

/// Enforce a project-relative artifacts directory without upward traversal.
fn validate_artifacts_directory(path: &Path) -> Result<(), BuildSettingsError> {
    let reason = if path.as_os_str().is_empty() {
        Some(InvalidArtifactsDirectoryReason::Empty)
    } else if path.is_absolute() {
        Some(InvalidArtifactsDirectoryReason::Absolute)
    } else if path
        .components()
        .any(|component| component == Component::ParentDir)
    {
        Some(InvalidArtifactsDirectoryReason::ContainsParentTraversal)
    } else {
        None
    };
    reason.map_or(Ok(()), |reason| {
        Err(BuildSettingsError::InvalidArtifactsDirectory {
            path: path.to_owned(),
            reason,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Accept only the exact numeric version shape used by 1C distributions.
    fn platform_version_requires_four_numeric_components() {
        assert_eq!(
            PlatformVersion::parse("8.3.27.2325")
                .expect("valid version")
                .as_str(),
            "8.3.27.2325"
        );
        for value in ["", "8.3", "8.3.27.x", "8.3.27.2325.1", " 8.3.27.2325"] {
            assert!(matches!(
                PlatformVersion::parse(value),
                Err(BuildSettingsError::InvalidPlatformVersion { .. })
            ));
        }
    }

    #[test]
    /// Keep configured artifacts inside the project-relative namespace.
    fn artifacts_directory_is_project_relative_without_parent_traversal() {
        assert!(BuildSettings::new("8.3.27.2325", PathBuf::from("artifacts/onec")).is_ok());
        for path in [
            PathBuf::new(),
            PathBuf::from("/build"),
            PathBuf::from("build/../out"),
        ] {
            assert!(matches!(
                BuildSettings::new("8.3.27.2325", path),
                Err(BuildSettingsError::InvalidArtifactsDirectory { .. })
            ));
        }
    }
}
