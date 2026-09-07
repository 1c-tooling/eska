use std::{
    ffi::{OsStr, OsString},
    path::{Component, Path, PathBuf},
};

use super::PlatformVersion;
use crate::project::{Project, ProjectType};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactType {
    Configuration,
    Extension,
    Processing,
    Report,
}

impl ArtifactType {
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Configuration => "cf",
            Self::Extension => "cfe",
            Self::Processing => "epf",
            Self::Report => "erf",
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Configuration => "configuration",
            Self::Extension => "extension",
            Self::Processing => "processing",
            Self::Report => "report",
        }
    }
}

impl From<ProjectType> for ArtifactType {
    /// Map each supported project type to its native artifact kind.
    fn from(value: ProjectType) -> Self {
        match value {
            ProjectType::Configuration => Self::Configuration,
            ProjectType::Extension => Self::Extension,
            ProjectType::Processing => Self::Processing,
            ProjectType::Report => Self::Report,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildPlan {
    project_root: PathBuf,
    output_scope_root: PathBuf,
    artifact_type: ArtifactType,
    platform_version: PlatformVersion,
    source: PathBuf,
    artifacts_directory: PathBuf,
    output: PathBuf,
    explicit_output: bool,
}

impl BuildPlan {
    /// Resolve a deterministic artifact path without creating files or invoking tools.
    ///
    /// Relative explicit outputs are resolved from the project root. Their extension must match
    /// the project type so a successful build cannot be presented as a different artifact.
    ///
    /// # Errors
    /// Returns a structured error for a nameless project root or unsafe/mismatched output path.
    pub fn new(project: &Project, output: Option<&Path>) -> Result<Self, PlanError> {
        Self::with_platform_version(project, output, None)
    }

    /// Resolve an artifact plan with an optional one-run platform override.
    ///
    /// # Errors
    /// Returns a structured error for a nameless project root or unsafe/mismatched output path.
    pub fn with_platform_version(
        project: &Project,
        output: Option<&Path>,
        platform_version: Option<PlatformVersion>,
    ) -> Result<Self, PlanError> {
        let explicit_output = output.is_some();
        let artifact_type = ArtifactType::from(project.configuration().project_type());
        let artifacts_directory = project.root().join(
            project
                .configuration()
                .build_settings()
                .artifacts_directory(),
        );
        let output = match output {
            Some(output) => resolve_explicit_output(project.root(), output, artifact_type)?,
            None => artifacts_directory.join(default_filename(project.root(), artifact_type)?),
        };
        Ok(Self {
            project_root: project.root().to_owned(),
            output_scope_root: project.root().to_owned(),
            artifact_type,
            platform_version: platform_version
                .or_else(|| {
                    project
                        .configuration()
                        .build_settings()
                        .platform_version()
                        .cloned()
                })
                .ok_or(PlanError::PlatformVersionMissing)?,
            source: project.source().to_owned(),
            artifacts_directory,
            output,
            explicit_output,
        })
    }

    /// Resolve an artifact plan for one workspace member.
    ///
    /// Default and relative explicit outputs belong to the workspace root, while the source
    /// remains scoped to the member project. The stable member name defines the artifact name.
    ///
    /// # Errors
    /// Returns a structured error for an unsafe/mismatched output path or missing platform.
    pub fn for_workspace_member(
        project: &Project,
        workspace_root: &Path,
        member_name: &str,
        output: Option<&Path>,
        platform_version: Option<PlatformVersion>,
    ) -> Result<Self, PlanError> {
        let explicit_output = output.is_some();
        let artifact_type = ArtifactType::from(project.configuration().project_type());
        let artifacts_directory = workspace_root.join(
            project
                .configuration()
                .build_settings()
                .artifacts_directory(),
        );
        let output = match output {
            Some(output) => resolve_explicit_output(workspace_root, output, artifact_type)?,
            None => artifacts_directory.join(named_filename(member_name, artifact_type)?),
        };
        Ok(Self {
            project_root: project.root().to_owned(),
            output_scope_root: workspace_root.to_owned(),
            artifact_type,
            platform_version: platform_version
                .or_else(|| {
                    project
                        .configuration()
                        .build_settings()
                        .platform_version()
                        .cloned()
                })
                .ok_or(PlanError::PlatformVersionMissing)?,
            source: project.source().to_owned(),
            artifacts_directory,
            output,
            explicit_output,
        })
    }

    #[must_use]
    pub const fn artifact_type(&self) -> ArtifactType {
        self.artifact_type
    }

    #[must_use]
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    #[must_use]
    pub fn output_scope_root(&self) -> &Path {
        &self.output_scope_root
    }

    #[must_use]
    pub const fn platform_version(&self) -> &PlatformVersion {
        &self.platform_version
    }

    #[must_use]
    pub fn source(&self) -> &Path {
        &self.source
    }

    #[must_use]
    pub fn artifacts_directory(&self) -> &Path {
        &self.artifacts_directory
    }

    #[must_use]
    pub fn output(&self) -> &Path {
        &self.output
    }

    #[must_use]
    pub const fn has_explicit_output(&self) -> bool {
        self.explicit_output
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanError {
    ProjectNameMissing,
    PlatformVersionMissing,
    InvalidOutput {
        path: PathBuf,
    },
    UnexpectedExtension {
        path: PathBuf,
        expected: &'static str,
    },
    OutputCollision {
        path: PathBuf,
    },
}

/// Reject a group in which multiple plans would publish the same artifact.
///
/// # Errors
/// Returns the first duplicate output path in plan order.
pub fn validate_unique_outputs(plans: &[&BuildPlan]) -> Result<(), PlanError> {
    for (index, plan) in plans.iter().enumerate() {
        if plans[..index]
            .iter()
            .any(|existing| existing.output() == plan.output())
        {
            return Err(PlanError::OutputCollision {
                path: plan.output().to_owned(),
            });
        }
    }
    Ok(())
}

/// Derive the native artifact filename from the project directory name.
fn default_filename(root: &Path, artifact_type: ArtifactType) -> Result<OsString, PlanError> {
    let name = root
        .file_name()
        .filter(|name| !name.is_empty())
        .ok_or(PlanError::ProjectNameMissing)?;
    named_filename(name, artifact_type)
}

fn named_filename(
    name: impl AsRef<OsStr>,
    artifact_type: ArtifactType,
) -> Result<OsString, PlanError> {
    let name = name.as_ref();
    if name.is_empty() {
        return Err(PlanError::ProjectNameMissing);
    }
    let mut filename = name.to_os_string();
    filename.push(".");
    filename.push(artifact_type.extension());
    Ok(filename)
}

/// Resolve a safe explicit output and enforce its native extension.
fn resolve_explicit_output(
    root: &Path,
    output: &Path,
    artifact_type: ArtifactType,
) -> Result<PathBuf, PlanError> {
    if output.as_os_str().is_empty()
        || output
            .components()
            .any(|component| component == Component::ParentDir)
    {
        return Err(PlanError::InvalidOutput {
            path: output.to_owned(),
        });
    }
    if output.extension().and_then(|value| value.to_str()) != Some(artifact_type.extension()) {
        return Err(PlanError::UnexpectedExtension {
            path: output.to_owned(),
            expected: artifact_type.extension(),
        });
    }
    Ok(if output.is_absolute() {
        output.to_owned()
    } else {
        root.join(output)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{ProjectConfiguration, SourceFormat, build::BuildSettings};

    /// Construct a filesystem-independent project for plan tests.
    fn project(project_type: ProjectType, root: &str) -> Project {
        Project::new(
            PathBuf::from(root),
            PathBuf::from(root).join("src"),
            ProjectConfiguration::new(project_type, SourceFormat::DesignerXml).with_build_settings(
                BuildSettings::new("8.3.27.2325", PathBuf::from("artifacts"))
                    .expect("valid settings"),
            ),
        )
        .expect("valid project")
    }

    #[test]
    /// Derive the correct native extension for every project type.
    fn default_artifact_matches_every_project_type() {
        for (project_type, extension) in [
            (ProjectType::Configuration, "cf"),
            (ProjectType::Extension, "cfe"),
            (ProjectType::Processing, "epf"),
            (ProjectType::Report, "erf"),
        ] {
            let plan =
                BuildPlan::new(&project(project_type, "/work/demo"), None).expect("valid plan");
            assert_eq!(
                plan.output(),
                Path::new(&format!("/work/demo/artifacts/demo.{extension}"))
            );
        }
    }

    #[test]
    /// Resolve relative output without allowing traversal or type confusion.
    fn explicit_output_is_project_relative_and_type_safe() {
        let project = project(ProjectType::Configuration, "/work/demo");
        assert_eq!(
            BuildPlan::new(&project, Some(Path::new("dist/result.cf")))
                .expect("valid output")
                .output(),
            Path::new("/work/demo/dist/result.cf")
        );
        assert!(matches!(
            BuildPlan::new(&project, Some(Path::new("../result.cf"))),
            Err(PlanError::InvalidOutput { .. })
        ));
        assert!(matches!(
            BuildPlan::new(&project, Some(Path::new("result.cfe"))),
            Err(PlanError::UnexpectedExtension { expected: "cf", .. })
        ));
    }

    #[test]
    /// Keep a platform override inside the plan without changing project settings.
    fn platform_override_is_plan_local() {
        let project = project(ProjectType::Configuration, "/work/demo");
        let override_version = PlatformVersion::parse("8.5.4.1234").expect("version");
        let plan = BuildPlan::with_platform_version(&project, None, Some(override_version))
            .expect("valid plan");
        assert_eq!(plan.platform_version().as_str(), "8.5.4.1234");
        assert_eq!(
            project
                .configuration()
                .build_settings()
                .platform_version()
                .expect("configured version")
                .as_str(),
            "8.3.27.2325"
        );
    }

    #[test]
    /// Place workspace artifacts under the shared build directory using stable member names.
    fn workspace_member_uses_workspace_output_scope() {
        let project = project(ProjectType::Report, "/work/src/report-directory");
        let plan = BuildPlan::for_workspace_member(
            &project,
            Path::new("/work"),
            "sales-report",
            None,
            None,
        )
        .expect("valid workspace plan");
        assert_eq!(plan.project_root(), Path::new("/work/src/report-directory"));
        assert_eq!(plan.output_scope_root(), Path::new("/work"));
        assert_eq!(plan.output(), Path::new("/work/artifacts/sales-report.erf"));
    }

    #[test]
    /// Reject duplicate publication targets before any build can start.
    fn grouped_plans_reject_output_collisions() {
        let project = project(ProjectType::Report, "/work/src/report");
        let plan =
            BuildPlan::for_workspace_member(&project, Path::new("/work"), "report", None, None)
                .expect("valid workspace plan");
        assert!(matches!(
            validate_unique_outputs(&[&plan, &plan]),
            Err(PlanError::OutputCollision { .. })
        ));
    }
}
