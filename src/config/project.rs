//! Validated project configuration: loading, serialization and source-path rules.

use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

use crate::project::{Project, ProjectConfiguration, ProjectPathError, ProjectType, SourceFormat};

use crate::vcs::workflow::WorkflowPreset;

use super::schema::{
    DEFAULT_SOURCE, RawDocument, SerializedDocument, SerializedProject, SerializedVcs,
    SerializedWorkflow, default_source, parse_project_type, parse_source_format, project_type_name,
    source_format_name,
};

/// The validated contents of an `eska.toml` file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectConfig {
    source: PathBuf,
    configuration: ProjectConfiguration,
}

impl ProjectConfig {
    /// Creates a configuration with the default source directory and format.
    #[must_use]
    pub fn new(project_type: ProjectType) -> Self {
        Self {
            source: default_source(),
            configuration: ProjectConfiguration::new(project_type, SourceFormat::DesignerXml),
        }
    }

    /// Adds a workflow selection without configuring its future execution policy.
    #[must_use]
    pub const fn with_workflow(mut self, workflow: WorkflowPreset) -> Self {
        self.configuration = self.configuration.with_workflow(workflow);
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
        let project_type = parse_project_type(document.project.project_type)?;
        let source_format = parse_source_format(document.project.source_format)?;

        validate_source_path(&document.project.source)?;

        let mut configuration = ProjectConfiguration::new(project_type, source_format);
        if let Some(vcs) = document.vcs {
            let value = vcs.workflow.preset;
            let preset = WorkflowPreset::from_name(&value)
                .ok_or(ProjectConfigError::UnknownWorkflow { value })?;
            configuration = configuration.with_workflow(preset);
        }
        Ok(Self {
            source: document.project.source,
            configuration,
        })
    }

    /// Serializes the configuration using the compact canonical representation.
    ///
    /// # Errors
    ///
    /// Returns an error if a path cannot be represented by the TOML serializer.
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        let source = (self.source != Path::new(DEFAULT_SOURCE)).then_some(self.source.as_path());
        let source_format = (self.configuration.source_format() != SourceFormat::DesignerXml)
            .then_some(source_format_name(self.configuration.source_format()));
        let document = SerializedDocument {
            project: SerializedProject {
                project_type: project_type_name(self.configuration.project_type()),
                source,
                source_format,
            },
            vcs: self.configuration.workflow().map(|preset| SerializedVcs {
                workflow: SerializedWorkflow {
                    preset: preset.as_str(),
                },
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
                    "[project]\ntype = \"report\"\n\n[vcs.workflow]\npreset = \"{}\"\n",
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
        assert_eq!(serialized, "[project]\ntype = \"processing\"\n");
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
            "[project]\ntype = \"report\"\nsource = \"designer\"\n"
        );
    }
}
