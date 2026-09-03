//! Built-in project scaffolds, represented as data without filesystem side effects.

use std::path::{Path, PathBuf};

use crate::config::{FILE_NAME, ProjectConfig};

use super::ProjectType;

/// A project-relative file and directory plan for the project creation layer.
///
/// Template selection is separate from filesystem writing. Future local template
/// sources can produce the same plan without changing the creation workflow.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Template {
    directories: Vec<PathBuf>,
    files: Vec<TemplateFile>,
}

impl Template {
    /// Renders the minimal built-in scaffold for a supported project type.
    ///
    /// This creates no files, initializes no VCS, and generates no 1C XML. All
    /// paths in the returned plan are relative to the destination project root.
    ///
    /// # Errors
    ///
    /// Returns the configuration serializer's error if rendering `eska.toml`
    /// fails. The caller can render before making any filesystem changes.
    pub fn built_in(project_type: ProjectType) -> Result<Self, toml::ser::Error> {
        let config = ProjectConfig::new(project_type);
        Self::from_config(&config)
    }

    /// Renders a scaffold using a validated config, including its saved workflow.
    ///
    /// # Errors
    ///
    /// Returns the configuration serializer's error if rendering fails.
    pub fn from_config(config: &ProjectConfig) -> Result<Self, toml::ser::Error> {
        let source = config.source().to_path_buf();
        let contents = config.to_toml()?;

        Ok(Self {
            files: vec![
                TemplateFile {
                    path: PathBuf::from(FILE_NAME),
                    contents,
                },
                // Git does not retain empty directories. Keep the source root
                // discoverable after a checkout even before the first XML export.
                TemplateFile {
                    path: source.join(".gitkeep"),
                    contents: String::new(),
                },
            ],
            directories: vec![source],
        })
    }

    /// Returns project-relative directories to create before writing files.
    #[must_use]
    pub fn directories(&self) -> &[PathBuf] {
        &self.directories
    }

    /// Returns files to create, with deterministic ordering and UTF-8 contents.
    #[must_use]
    pub fn files(&self) -> &[TemplateFile] {
        &self.files
    }
}

/// A rendered template file, independent of its eventual destination root.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TemplateFile {
    path: PathBuf,
    contents: String,
}

impl TemplateFile {
    /// Returns the path relative to the destination project root.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn contents(&self) -> &str {
        &self.contents
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Component, Path, PathBuf};

    use super::Template;
    use crate::{
        config::ProjectConfig,
        project::{ProjectType, SourceFormat},
    };

    #[test]
    fn all_built_ins_use_the_config_schema_and_compact_defaults() {
        for (project_type, name) in [
            (ProjectType::Configuration, "configuration"),
            (ProjectType::Extension, "extension"),
            (ProjectType::Processing, "processing"),
            (ProjectType::Report, "report"),
        ] {
            let template = Template::built_in(project_type).expect("render built-in");
            assert_eq!(template.directories(), &[PathBuf::from("src")]);
            assert_eq!(template.files().len(), 2);
            let config_file = &template.files()[0];
            assert_eq!(config_file.path(), Path::new("eska.toml"));
            assert_eq!(
                config_file.contents(),
                format!("[project]\ntype = \"{name}\"\n")
            );

            let config = ProjectConfig::from_toml(config_file.contents()).expect("valid config");
            assert_eq!(config.configuration().project_type(), project_type);
            assert_eq!(
                config.configuration().source_format(),
                SourceFormat::DesignerXml
            );
            assert_eq!(config.source(), template.directories()[0]);
            assert_eq!(config, ProjectConfig::new(project_type));
            assert_eq!(
                config.to_toml().expect("serialize config"),
                config_file.contents()
            );

            let placeholder = &template.files()[1];
            assert_eq!(placeholder.path(), Path::new("src/.gitkeep"));
            assert!(placeholder.contents().is_empty());
            assert_eq!(
                Template::built_in(project_type).expect("repeat render"),
                template
            );
        }
    }

    #[test]
    fn built_in_paths_are_safe_and_unique_project_relative_paths() {
        for project_type in [
            ProjectType::Configuration,
            ProjectType::Extension,
            ProjectType::Processing,
            ProjectType::Report,
        ] {
            let template = Template::built_in(project_type).expect("render built-in");
            let mut seen = std::collections::HashSet::new();
            for path in template
                .directories()
                .iter()
                .map(PathBuf::as_path)
                .chain(template.files().iter().map(super::TemplateFile::path))
            {
                assert!(!path.as_os_str().is_empty());
                assert!(
                    path.components()
                        .all(|part| matches!(part, Component::Normal(_)))
                );
                assert!(seen.insert(path), "duplicate path: {path:?}");
            }
        }
    }
}
