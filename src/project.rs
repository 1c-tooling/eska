use std::path::{Component, Path, PathBuf};

/// A validated eska project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Project {
    root: PathBuf,
    source: PathBuf,
    configuration: ProjectConfiguration,
}

impl Project {
    /// Creates a project from absolute paths without parent traversal.
    ///
    /// This constructor validates path relationships without accessing the file
    /// system. Project discovery is responsible for checking that the paths exist.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectPathError`] when either path is relative, contains parent
    /// traversal, or when the source is outside the project root.
    pub fn new(
        root: PathBuf,
        source: PathBuf,
        configuration: ProjectConfiguration,
    ) -> Result<Self, ProjectPathError> {
        validate_path(&root, ProjectPath::Root)?;
        validate_path(&source, ProjectPath::Source)?;

        if !source.starts_with(&root) {
            return Err(ProjectPathError::SourceOutsideRoot { root, source });
        }

        Ok(Self {
            root,
            source,
            configuration,
        })
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
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

/// Project settings that come from the project configuration.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProjectConfiguration {
    project_type: ProjectType,
    source_format: SourceFormat,
}

impl ProjectConfiguration {
    #[must_use]
    pub const fn new(project_type: ProjectType, source_format: SourceFormat) -> Self {
        Self {
            project_type,
            source_format,
        }
    }

    #[must_use]
    pub const fn project_type(self) -> ProjectType {
        self.project_type
    }

    #[must_use]
    pub const fn source_format(self) -> SourceFormat {
        self.source_format
    }
}

/// The kind of 1C artifact represented by a project.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProjectType {
    Configuration,
    Extension,
    Processing,
    Report,
}

/// The on-disk representation of project sources.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SourceFormat {
    DesignerXml,
}

/// Identifies a path field in a project.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectPath {
    Root,
    Source,
}

/// The reason a project path was rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidPathReason {
    NotAbsolute,
    ContainsParentTraversal,
}

/// A structured violation of project path invariants.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectPathError {
    InvalidPath {
        field: ProjectPath,
        path: PathBuf,
        reason: InvalidPathReason,
    },
    SourceOutsideRoot {
        root: PathBuf,
        source: PathBuf,
    },
}

fn validate_path(path: &Path, field: ProjectPath) -> Result<(), ProjectPathError> {
    if !path.is_absolute() {
        return Err(ProjectPathError::InvalidPath {
            field,
            path: path.to_path_buf(),
            reason: InvalidPathReason::NotAbsolute,
        });
    }

    if path
        .components()
        .any(|component| component == Component::ParentDir)
    {
        return Err(ProjectPathError::InvalidPath {
            field,
            path: path.to_path_buf(),
            reason: InvalidPathReason::ContainsParentTraversal,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        InvalidPathReason, Project, ProjectConfiguration, ProjectPath, ProjectPathError,
        ProjectType, SourceFormat,
    };
    use std::path::{Path, PathBuf};

    fn configuration() -> ProjectConfiguration {
        ProjectConfiguration::new(ProjectType::Configuration, SourceFormat::DesignerXml)
    }

    #[test]
    fn creates_project_with_source_inside_root() {
        let project = Project::new(
            PathBuf::from("/work/example"),
            PathBuf::from("/work/example/src"),
            configuration(),
        )
        .expect("valid project paths");

        assert_eq!(project.root(), Path::new("/work/example"));
        assert_eq!(project.source(), Path::new("/work/example/src"));
        assert_eq!(
            project.configuration().project_type(),
            ProjectType::Configuration
        );
        assert_eq!(
            project.configuration().source_format(),
            SourceFormat::DesignerXml
        );
    }

    #[test]
    fn allows_project_root_as_source_directory() {
        let project = Project::new(
            PathBuf::from("/work/example"),
            PathBuf::from("/work/example"),
            configuration(),
        );

        assert!(project.is_ok());
    }

    #[test]
    fn rejects_relative_root() {
        let error = Project::new(
            PathBuf::from("example"),
            PathBuf::from("/work/example/src"),
            configuration(),
        )
        .expect_err("relative root must be rejected");

        assert_eq!(
            error,
            ProjectPathError::InvalidPath {
                field: ProjectPath::Root,
                path: PathBuf::from("example"),
                reason: InvalidPathReason::NotAbsolute,
            }
        );
    }

    #[test]
    fn rejects_relative_source() {
        let error = Project::new(
            PathBuf::from("/work/example"),
            PathBuf::from("src"),
            configuration(),
        )
        .expect_err("relative source must be rejected");

        assert_eq!(
            error,
            ProjectPathError::InvalidPath {
                field: ProjectPath::Source,
                path: PathBuf::from("src"),
                reason: InvalidPathReason::NotAbsolute,
            }
        );
    }

    #[test]
    fn rejects_parent_traversal() {
        for (field, root, source, path) in [
            (
                ProjectPath::Root,
                "/work/other/../example",
                "/work/example/src",
                "/work/other/../example",
            ),
            (
                ProjectPath::Source,
                "/work/example",
                "/work/example/src/../xml",
                "/work/example/src/../xml",
            ),
        ] {
            let error = Project::new(root.into(), source.into(), configuration())
                .expect_err("parent traversal must be rejected");

            assert_eq!(
                error,
                ProjectPathError::InvalidPath {
                    field,
                    path: path.into(),
                    reason: InvalidPathReason::ContainsParentTraversal,
                }
            );
        }
    }

    #[test]
    fn rejects_source_outside_root() {
        let error = Project::new(
            PathBuf::from("/work/example"),
            PathBuf::from("/work/another/src"),
            configuration(),
        )
        .expect_err("source outside root must be rejected");

        assert_eq!(
            error,
            ProjectPathError::SourceOutsideRoot {
                root: PathBuf::from("/work/example"),
                source: PathBuf::from("/work/another/src"),
            }
        );
    }

    #[test]
    fn exposes_all_supported_project_types() {
        let variants = [
            ProjectType::Configuration,
            ProjectType::Extension,
            ProjectType::Processing,
            ProjectType::Report,
        ];

        assert_eq!(variants.len(), 4);
    }
}
