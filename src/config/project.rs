//! Validated project configuration: loading, serialization and source-path rules.

use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

use crate::project::{
    Project, ProjectConfiguration, ProjectName, ProjectNameError, ProjectPathError, ProjectType,
    SourceFormat,
    build::{BuildSettings, BuildSettingsError},
};

use crate::vcs::workflow::WorkflowPreset;

use super::schema::{
    DEFAULT_SOURCE, RawDocument, SerializedBuild, SerializedDocument, SerializedProject,
    SerializedVcs, default_source, parse_project_type, parse_source_format, project_type_name,
    source_format_name,
};

/// The validated contents of an `eska.toml` file.
#[derive(Clone, Debug)]
pub struct ProjectConfig {
    name: Option<ProjectName>,
    source: PathBuf,
    configuration: ProjectConfiguration,
    build_overrides: ProjectBuildOverrides,
}

impl PartialEq for ProjectConfig {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.source == other.source
            && self.configuration == other.configuration
    }
}

impl Eq for ProjectConfig {}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ProjectBuildOverrides {
    platform_version: Option<String>,
    artifacts_directory: Option<PathBuf>,
}

impl ProjectConfig {
    /// Creates a configuration with the default source directory and format.
    #[must_use]
    pub fn new(project_type: ProjectType) -> Self {
        Self {
            name: None,
            source: default_source(),
            configuration: ProjectConfiguration::new(project_type, SourceFormat::DesignerXml),
            build_overrides: ProjectBuildOverrides::default(),
        }
    }

    /// Adds a workflow selection without configuring its future execution policy.
    #[must_use]
    pub fn with_workflow(mut self, workflow: WorkflowPreset) -> Self {
        self.configuration = self.configuration.with_workflow(workflow);
        self
    }

    #[must_use]
    pub fn with_build_settings(mut self, build: BuildSettings) -> Self {
        self.build_overrides = ProjectBuildOverrides {
            platform_version: Some(
                build
                    .platform_version()
                    .map_or_else(String::new, |version| version.as_str().to_owned()),
            ),
            artifacts_directory: Some(build.artifacts_directory().to_owned()),
        };
        self.configuration = self.configuration.with_build_settings(build);
        self
    }

    /// Sets the portable name required when this project is a workspace member.
    #[must_use]
    pub fn with_name(mut self, name: ProjectName) -> Self {
        self.name = Some(name);
        self
    }

    /// Sets a validated relative source directory.
    ///
    /// # Errors
    /// Returns [`ProjectConfigError::InvalidSource`] for an unsafe path.
    pub fn with_source(mut self, source: PathBuf) -> Result<Self, ProjectConfigError> {
        validate_source_path(&source)?;
        self.source = source;
        Ok(self)
    }

    /// Loads and validates an `eska.toml` file.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectConfigError::Io`] when the file cannot be read, or the
    /// same validation errors as [`Self::from_toml`] for invalid contents.
    pub fn load(path: &Path) -> Result<Self, ProjectConfigError> {
        let input = fs::read_to_string(path).map_err(|source| ProjectConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_toml(&input)
    }

    /// Parses and validates an `eska.toml` document.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectConfigError`] for malformed TOML, unknown machine-facing
    /// values, unknown fields, or an invalid source path.
    pub fn from_toml(input: &str) -> Result<Self, ProjectConfigError> {
        let document: RawDocument = toml::from_str(input).map_err(ProjectConfigError::Toml)?;
        let name = document
            .project
            .name
            .map(ProjectName::parse)
            .transpose()
            .map_err(ProjectConfigError::InvalidName)?;
        let project_type = parse_project_type(document.project.project_type)?;
        let source_format = parse_source_format(document.project.source_format)?;

        validate_source_path(&document.project.source)?;

        let default_build = BuildSettings::default();
        let build = document.build.unwrap_or_default();
        let build_overrides = ProjectBuildOverrides {
            platform_version: build.platform_version.clone(),
            artifacts_directory: build.artifacts_directory.clone(),
        };
        let build = BuildSettings::new(
            build.platform_version.as_deref().unwrap_or_default(),
            build
                .artifacts_directory
                .unwrap_or_else(|| default_build.artifacts_directory().to_owned()),
        )?;
        let mut configuration =
            ProjectConfiguration::new(project_type, source_format).with_build_settings(build);
        if let Some(vcs) = document.vcs {
            configuration =
                configuration.with_workflow_settings(super::workflow::parse(vcs.workflow)?);
        }
        Ok(Self {
            name,
            source: document.project.source,
            configuration,
            build_overrides,
        })
    }

    /// Serializes the configuration using the compact canonical representation.
    ///
    /// # Errors
    ///
    /// Returns an error if a path cannot be represented by the TOML serializer.
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        self.serialize(false)
    }

    /// Serializes a member manifest without workspace-owned workflow and defaults.
    ///
    /// Callers construct member configs without workflow or artifact overrides;
    /// only an explicit platform override is retained.
    pub(crate) fn to_workspace_member_toml(&self) -> Result<String, toml::ser::Error> {
        self.serialize(true)
    }

    fn serialize(&self, workspace_member: bool) -> Result<String, toml::ser::Error> {
        let source = (self.source != Path::new(DEFAULT_SOURCE)).then_some(self.source.as_path());
        let source_format = (self.configuration.source_format() != SourceFormat::DesignerXml)
            .then_some(source_format_name(self.configuration.source_format()));
        let default_build = BuildSettings::default();
        let build = self.configuration.build_settings();
        let build =
            (!workspace_member || self.build_overrides.platform_version.is_some()).then(|| {
                SerializedBuild {
                    platform_version: build
                        .platform_version()
                        .map_or("", crate::project::build::PlatformVersion::as_str),
                    artifacts_directory: (!workspace_member
                        && build.artifacts_directory() != default_build.artifacts_directory())
                    .then(|| build.artifacts_directory()),
                }
            });
        let document = SerializedDocument {
            project: SerializedProject {
                name: self.name.as_ref().map(ProjectName::as_str),
                project_type: project_type_name(self.configuration.project_type()),
                source,
                source_format,
            },
            build,
            vcs: (!workspace_member)
                .then(|| self.configuration.workflow_settings())
                .flatten()
                .map(|settings| SerializedVcs {
                    workflow: super::workflow::serialize(settings),
                }),
        };

        toml::to_string_pretty(&document)
    }

    /// Resolves the configured source against a project root and builds the
    /// locale-independent domain model.
    ///
    /// This does not access the file system. Directory existence is validated by
    /// project discovery.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectConfigError::ProjectPath`] when the root violates domain
    /// path invariants.
    pub fn into_project(self, root: PathBuf) -> Result<Project, ProjectConfigError> {
        let source = root.join(self.source);
        Project::new(root, source, self.configuration).map_err(ProjectConfigError::ProjectPath)
    }

    #[must_use]
    pub fn source(&self) -> &Path {
        &self.source
    }

    #[must_use]
    pub const fn configuration(&self) -> &ProjectConfiguration {
        &self.configuration
    }

    #[must_use]
    pub const fn name(&self) -> Option<&ProjectName> {
        self.name.as_ref()
    }

    pub(crate) fn platform_version_override(&self) -> Option<&str> {
        self.build_overrides.platform_version.as_deref()
    }

    pub(crate) const fn overrides_artifacts_directory(&self) -> bool {
        self.build_overrides.artifacts_directory.is_some()
    }
}

/// The reason a configured source path was rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidSourceReason {
    Empty,
    Absolute,
    ContainsParentTraversal,
}

/// A structured project configuration error.
#[derive(Debug)]
pub enum ProjectConfigError {
    InvalidName(ProjectNameError),
    InvalidBuild(BuildSettingsError),
    InvalidWorkflow(crate::vcs::workflow::PolicyError),
    UnknownWorkflow {
        value: String,
    },
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Toml(toml::de::Error),
    UnknownProjectType {
        value: String,
    },
    UnknownSourceFormat {
        value: String,
    },
    InvalidSource {
        path: PathBuf,
        reason: InvalidSourceReason,
    },
    ProjectPath(ProjectPathError),
}

impl From<BuildSettingsError> for ProjectConfigError {
    fn from(value: BuildSettingsError) -> Self {
        Self::InvalidBuild(value)
    }
}

fn validate_source_path(path: &Path) -> Result<(), ProjectConfigError> {
    let reason = if path.as_os_str().is_empty() {
        Some(InvalidSourceReason::Empty)
    } else if path.is_absolute() {
        Some(InvalidSourceReason::Absolute)
    } else if path
        .components()
        .any(|component| component == Component::ParentDir)
    {
        Some(InvalidSourceReason::ContainsParentTraversal)
    } else {
        None
    };

    reason.map_or(Ok(()), |reason| {
        Err(ProjectConfigError::InvalidSource {
            path: path.to_path_buf(),
            reason,
        })
    })
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{InvalidSourceReason, ProjectConfig, ProjectConfigError};
    use crate::{
        project::{ProjectType, SourceFormat},
        vcs::workflow::WorkflowPreset,
    };

    #[test]
    fn workflow_selection_round_trips_without_policy_or_locale() {
        for preset in [
            WorkflowPreset::Trunk,
            WorkflowPreset::GitFlow,
            WorkflowPreset::GithubFlow,
            WorkflowPreset::Custom,
        ] {
            let config = ProjectConfig::new(ProjectType::Report).with_workflow(preset);
            let text = config.to_toml().expect("serialize workflow");
            assert_eq!(
                text,
                format!(
                    "[project]\ntype = \"report\"\n\n[build]\nplatform_version = \"\"\n\n[vcs.workflow]\npreset = \"{}\"\n",
                    preset.as_str()
                )
            );
            let parsed = ProjectConfig::from_toml(&text).expect("parse workflow");
            assert_eq!(parsed, config);
            assert_eq!(
                parsed
                    .into_project(PathBuf::from("/work/demo"))
                    .expect("project")
                    .configuration()
                    .workflow(),
                Some(preset)
            );
        }
        assert_eq!(
            ProjectConfig::new(ProjectType::Report)
                .configuration()
                .workflow(),
            None
        );
    }

    #[test]
    fn workspace_member_serialization_omits_root_owned_settings() {
        let config = ProjectConfig::new(ProjectType::Processing)
            .with_name(crate::project::ProjectName::parse("my-orders".to_owned()).unwrap());
        assert_eq!(
            config.to_workspace_member_toml().unwrap(),
            "[project]\nname = \"my-orders\"\ntype = \"processing\"\n"
        );
    }

    #[test]
    /// Treat a missing legacy version as explicitly unconfigured on serialization.
    fn missing_build_version_is_unconfigured_and_materialized() {
        let config = ProjectConfig::from_toml("[project]\ntype = 'configuration'\n")
            .expect("legacy config remains valid");

        assert_eq!(
            config.configuration().build_settings().platform_version(),
            None
        );
        assert_eq!(
            config
                .configuration()
                .build_settings()
                .artifacts_directory(),
            Path::new("build")
        );
        assert_eq!(
            config.to_toml().expect("serialize defaults"),
            "[project]\ntype = \"configuration\"\n\n[build]\nplatform_version = \"\"\n"
        );
    }

    #[test]
    /// Persist only explicit non-default build values in canonical TOML.
    fn explicit_build_settings_round_trip_without_materializing_defaults() {
        let config = ProjectConfig::from_toml(
            "[project]\ntype = 'report'\n[build]\nplatform_version = '8.5.4.1000'\nartifacts_directory = 'dist/onec'\n",
        )
        .expect("valid build settings");

        let text = config.to_toml().expect("serialize build settings");
        assert!(text.contains("platform_version = \"8.5.4.1000\""));
        assert!(text.contains("artifacts_directory = \"dist/onec\""));
        assert_eq!(ProjectConfig::from_toml(&text).expect("round trip"), config);
    }

    #[test]
    /// Report invalid build values through the existing structured config boundary.
    fn invalid_build_settings_are_structured_config_errors() {
        for suffix in [
            "[build]\nplatform_version = '8.3'\n",
            "[build]\nartifacts_directory = '../outside'\n",
            "[build]\nunknown = true\n",
        ] {
            let error =
                ProjectConfig::from_toml(&format!("[project]\ntype = 'configuration'\n{suffix}"))
                    .expect_err("invalid build settings must fail");
            assert!(matches!(
                error,
                ProjectConfigError::InvalidBuild(_) | ProjectConfigError::Toml(_)
            ));
        }
    }

    #[test]
    fn rejects_invalid_or_incomplete_workflow_configuration() {
        let prefix = "[project]\ntype = \"report\"\n";
        let error =
            ProjectConfig::from_toml(&format!("{prefix}[vcs.workflow]\npreset = \"unknown\"\n"))
                .expect_err("unknown preset");
        assert!(
            matches!(error, ProjectConfigError::UnknownWorkflow { value } if value == "unknown")
        );
        for suffix in [
            "[vcs]\n",
            "[vcs.workflow]\n",
            "[vcs]\nenabled = true\n",
            "[vcs.workflow]\npreset = \"trunk\"\nbranch = \"main\"\n",
        ] {
            assert!(matches!(
                ProjectConfig::from_toml(&format!("{prefix}{suffix}")),
                Err(ProjectConfigError::Toml(_))
            ));
        }
    }

    struct TempConfig {
        directory: PathBuf,
        path: PathBuf,
    }

    impl TempConfig {
        fn new(contents: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time after Unix epoch")
                .as_nanos();
            let directory = std::env::temp_dir()
                .join(format!("eska-config-test-{}-{unique}", std::process::id()));
            fs::create_dir(&directory).expect("create temporary config directory");
            let path = directory.join("eska.toml");
            fs::write(&path, contents).expect("write temporary config");

            Self { directory, path }
        }
    }

    impl Drop for TempConfig {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.directory).expect("remove temporary config directory");
        }
    }

    #[test]
    fn loads_config_from_file() {
        let file = TempConfig::new("[project]\ntype = \"extension\"\n");

        let config = ProjectConfig::load(&file.path).expect("load valid eska.toml");

        assert_eq!(
            config.configuration().project_type(),
            ProjectType::Extension
        );
    }

    #[test]
    fn reports_config_read_error_with_path() {
        let file = TempConfig::new("[project]\ntype = \"report\"\n");
        let path = file.directory.join("missing.toml");

        let error = ProjectConfig::load(&path).expect_err("missing file must be reported");

        assert!(matches!(
            error,
            ProjectConfigError::Io { path: error_path, .. } if error_path == path
        ));
    }

    #[test]
    fn loads_minimal_config_into_project_with_defaults() {
        let config = ProjectConfig::from_toml(
            r#"
                [project]
                type = "configuration"
            "#,
        )
        .expect("valid minimal config");

        assert_eq!(config.source(), Path::new("src"));
        assert_eq!(
            config.configuration().source_format(),
            SourceFormat::DesignerXml
        );

        let project = config
            .into_project(PathBuf::from("/work/example"))
            .expect("valid project model");
        assert_eq!(project.root(), Path::new("/work/example"));
        assert_eq!(project.source(), Path::new("/work/example/src"));
        assert_eq!(
            project.configuration().project_type(),
            ProjectType::Configuration
        );
    }

    #[test]
    fn parses_all_project_types_and_explicit_source_format() {
        for (name, expected) in [
            ("configuration", ProjectType::Configuration),
            ("extension", ProjectType::Extension),
            ("processing", ProjectType::Processing),
            ("report", ProjectType::Report),
        ] {
            let input = format!(
                "[project]\ntype = \"{name}\"\nsource = \"xml\"\nsource_format = \"designer-xml\"\n"
            );
            let config = ProjectConfig::from_toml(&input).expect("valid project type");

            assert_eq!(config.configuration().project_type(), expected);
            assert_eq!(config.source(), Path::new("xml"));
        }
    }

    #[test]
    fn reports_malformed_toml() {
        let error = ProjectConfig::from_toml("[project\ntype = 42")
            .expect_err("malformed TOML must be rejected");

        assert!(matches!(error, ProjectConfigError::Toml(_)));
    }

    #[test]
    fn reports_unknown_enum_values() {
        let project_type = ProjectConfig::from_toml("[project]\ntype = \"database\"\n")
            .expect_err("unknown project type must be rejected");
        assert!(matches!(
            project_type,
            ProjectConfigError::UnknownProjectType { value } if value == "database"
        ));

        let source_format =
            ProjectConfig::from_toml("[project]\ntype = \"report\"\nsource_format = \"edt\"\n")
                .expect_err("unknown source format must be rejected");
        assert!(matches!(
            source_format,
            ProjectConfigError::UnknownSourceFormat { value } if value == "edt"
        ));
    }

    #[test]
    fn reports_invalid_source_paths() {
        for (source, expected_reason) in [
            ("", InvalidSourceReason::Empty),
            ("/outside", InvalidSourceReason::Absolute),
            ("../outside", InvalidSourceReason::ContainsParentTraversal),
        ] {
            let input = format!("[project]\ntype = \"report\"\nsource = \"{source}\"\n");
            let error =
                ProjectConfig::from_toml(&input).expect_err("invalid source path must be rejected");

            assert!(matches!(
                error,
                ProjectConfigError::InvalidSource { path, reason }
                    if path == Path::new(source) && reason == expected_reason
            ));
        }
    }

    #[test]
    fn reports_invalid_project_root() {
        let error = ProjectConfig::from_toml("[project]\ntype = \"extension\"\n")
            .expect("valid config")
            .into_project(PathBuf::from("relative-root"))
            .expect_err("invalid root must be rejected");

        assert!(matches!(error, ProjectConfigError::ProjectPath(_)));
    }

    #[test]
    fn rejects_unknown_fields_including_locale() {
        for input in [
            "[project]\ntype = \"configuration\"\nlocale = \"ru-RU\"\n",
            "[project]\ntype = \"configuration\"\n[format]\nline_width = 120\n",
        ] {
            let error =
                ProjectConfig::from_toml(input).expect_err("unknown fields must be rejected");
            assert!(matches!(error, ProjectConfigError::Toml(_)));
        }
    }

    #[test]
    fn serializes_defaults_compactly_and_round_trips() {
        let config = ProjectConfig::from_toml(
            "[project]\ntype = \"processing\"\nsource_format = \"designer-xml\"\n",
        )
        .expect("valid config");

        let serialized = config.to_toml().expect("serializable config");
        assert_eq!(
            serialized,
            "[project]\ntype = \"processing\"\n\n[build]\nplatform_version = \"\"\n"
        );
        assert_eq!(
            ProjectConfig::from_toml(&serialized).expect("round-trip config"),
            config
        );
    }

    #[test]
    fn serializes_non_default_source() {
        let config =
            ProjectConfig::from_toml("[project]\ntype = \"report\"\nsource = \"designer\"\n")
                .expect("valid config");

        assert_eq!(
            config.to_toml().expect("serializable config"),
            "[project]\ntype = \"report\"\nsource = \"designer\"\n\n[build]\nplatform_version = \"\"\n"
        );
    }

    #[test]
    fn project_name_round_trips_without_changing_unnamed_configs() {
        let named = ProjectConfig::from_toml(
            "[project]\nname = 'sales-report'\ntype = 'report'\nsource = '.'\n",
        )
        .expect("named project config");
        assert_eq!(named.name().unwrap().as_str(), "sales-report");
        assert!(named.to_toml().unwrap().contains("name = \"sales-report\""));

        let unnamed = ProjectConfig::new(ProjectType::Report);
        assert!(!unnamed.to_toml().unwrap().contains("name"));
    }
}
