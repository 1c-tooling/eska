//! Locale-independent model for a workspace of independent 1C projects.

use std::{
    fmt,
    path::{Path, PathBuf},
};

use crate::{
    project::{Project, build::BuildSettings},
    vcs::workflow::WorkflowSettings,
};

/// A portable, stable workspace member identifier.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProjectName(String);

impl ProjectName {
    /// Parses an ASCII identifier matching `[a-z0-9][a-z0-9_-]*`.
    ///
    /// # Errors
    /// Returns [`ProjectNameError`] when the value is empty or not portable.
    pub fn parse(value: String) -> Result<Self, ProjectNameError> {
        let mut bytes = value.bytes();
        let valid_first = bytes
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit());
        let valid_rest = bytes.all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        });
        if valid_first && valid_rest {
            Ok(Self(value))
        } else {
            Err(ProjectNameError { value })
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProjectName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A rejected workspace member name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectNameError {
    value: String,
}

impl ProjectNameError {
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// One validated and fully resolved workspace member.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceMember {
    name: ProjectName,
    project: Project,
}

impl WorkspaceMember {
    pub(crate) const fn new(name: ProjectName, project: Project) -> Self {
        Self { name, project }
    }

    #[must_use]
    pub const fn name(&self) -> &ProjectName {
        &self.name
    }

    #[must_use]
    pub const fn project(&self) -> &Project {
        &self.project
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        self.project.root()
    }
}

/// A validated workspace with resolved settings for every member.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Workspace {
    root: PathBuf,
    build: BuildSettings,
    workflow: Option<WorkflowSettings>,
    members: Vec<WorkspaceMember>,
}

impl Workspace {
    pub(crate) const fn new(
        root: PathBuf,
        build: BuildSettings,
        workflow: Option<WorkflowSettings>,
        members: Vec<WorkspaceMember>,
    ) -> Self {
        Self {
            root,
            build,
            workflow,
            members,
        }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub const fn build_settings(&self) -> &BuildSettings {
        &self.build
    }

    #[must_use]
    pub const fn workflow_settings(&self) -> Option<&WorkflowSettings> {
        self.workflow.as_ref()
    }

    #[must_use]
    pub fn members(&self) -> &[WorkspaceMember] {
        &self.members
    }

    #[must_use]
    pub fn member(&self, name: &ProjectName) -> Option<&WorkspaceMember> {
        self.members.iter().find(|member| member.name() == name)
    }
}

#[cfg(test)]
mod tests {
    use super::ProjectName;

    #[test]
    fn project_name_accepts_only_portable_identifiers() {
        for value in ["report", "sales-report", "sales_report", "1c-report"] {
            assert_eq!(
                ProjectName::parse(value.to_owned()).unwrap().as_str(),
                value
            );
        }
        for value in ["", "Sales", "-sales", "sales report", "отчет"] {
            assert!(ProjectName::parse(value.to_owned()).is_err(), "{value}");
        }
    }
}
