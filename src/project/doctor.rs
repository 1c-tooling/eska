//! Read-only diagnostic checks for the commands supported by a project.

use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
};

use serde::Serialize;

use super::{
    Project, ProjectType,
    build::{Ibcmd, ToolError, ToolOptions},
    designer_xml,
};
use crate::vcs::{command::Executor, repository::Repository, workflow::WorkflowPolicy};

const MAX_DESCRIPTOR_BYTES: u64 = 64 * 1024 * 1024;

/// Severity of one stable diagnostic check.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Warning,
    Fail,
    Skipped,
}

/// One locale-independent diagnostic result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Check {
    pub id: &'static str,
    pub status: CheckStatus,
    pub code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub commands: Vec<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<&'static str>,
}

impl Check {
    /// Construct a check with no optional context.
    #[must_use]
    pub fn new(
        id: &'static str,
        status: CheckStatus,
        code: &'static str,
        commands: &[&'static str],
    ) -> Self {
        Self {
            id,
            status,
            code,
            project: None,
            commands: commands.to_vec(),
            expected: None,
            actual: None,
            source: None,
            remediation: None,
        }
    }

    /// Attach the selected workspace project name.
    #[must_use]
    pub fn for_project(mut self, project: Option<&str>) -> Self {
        self.project = project.map(str::to_owned);
        self
    }

    /// Attach stable expected and actual values.
    #[must_use]
    pub fn with_values(mut self, expected: Option<String>, actual: Option<String>) -> Self {
        self.expected = expected;
        self.actual = actual;
        self
    }

    /// Attach the effective platform runner.
    #[must_use]
    pub const fn with_source(mut self, source: &'static str) -> Self {
        self.source = Some(source);
        self
    }

    /// Attach a stable remediation identifier.
    #[must_use]
    pub const fn with_remediation(mut self, remediation: &'static str) -> Self {
        self.remediation = Some(remediation);
        self
    }
}

/// Counts for the complete diagnostic report.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Summary {
    pub passed: usize,
    pub warnings: usize,
    pub failed: usize,
    pub skipped: usize,
}

/// An ordered collection of independent diagnostic checks.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Report {
    checks: Vec<Check>,
}

impl Report {
    /// Add one completed check in presentation order.
    pub fn push(&mut self, check: Check) {
        self.checks.push(check);
    }

    /// Add several checks while preserving their order.
    pub fn extend(&mut self, checks: impl IntoIterator<Item = Check>) {
        self.checks.extend(checks);
    }

    #[must_use]
    pub fn checks(&self) -> &[Check] {
        &self.checks
    }

    #[must_use]
    pub fn summary(&self) -> Summary {
        let mut summary = Summary::default();
        for check in &self.checks {
            match check.status {
                CheckStatus::Pass => summary.passed += 1,
                CheckStatus::Warning => summary.warnings += 1,
                CheckStatus::Fail => summary.failed += 1,
                CheckStatus::Skipped => summary.skipped += 1,
            }
        }
        summary
    }

    #[must_use]
    pub fn has_failures(&self) -> bool {
        self.checks
            .iter()
            .any(|check| check.status == CheckStatus::Fail)
    }
}

/// Inspect one validated project and its exact build tools without changing files.
#[must_use]
pub fn inspect_project(
    project: &Project,
    project_name: Option<&str>,
    tools: Option<&ToolOptions>,
) -> Vec<Check> {
    let context = |check: Check| check.for_project(project_name);
    let mut checks = vec![
        context(Check::new(
            "project.config",
            CheckStatus::Pass,
            "valid",
            &["status", "diff", "save", "build"],
        )),
        context(Check::new(
            "project.source",
            CheckStatus::Pass,
            "available",
            &["diff", "save", "build"],
        )),
        context(inspect_descriptor(project)),
    ];

    checks.extend(inspect_build(project, project_name, tools));
    checks
}

/// Inspect platform requirements shared by build and patch commands.
fn inspect_build(
    project: &Project,
    project_name: Option<&str>,
    tools: Option<&ToolOptions>,
) -> Vec<Check> {
    let context = |check: Check| check.for_project(project_name);
    let mut checks = Vec::new();
    let Some(version) = project.configuration().build_settings().platform_version() else {
        checks.push(context(
            Check::new(
                "build.platform-version",
                CheckStatus::Fail,
                "not-configured",
                &["build"],
            )
            .with_remediation("configure-platform-version"),
        ));
        checks.push(context(Check::new(
            "build.ibcmd",
            CheckStatus::Skipped,
            "platform-version-required",
            &["build"],
        )));
        append_skipped_designer(project, project_name, &mut checks);
        return checks;
    };
    checks.push(context(
        Check::new(
            "build.platform-version",
            CheckStatus::Pass,
            "version-configured",
            &["build"],
        )
        .with_values(Some(version.as_str().to_owned()), None),
    ));

    let Some(tools) = tools else {
        checks.push(context(Check::new(
            "build.ibcmd",
            CheckStatus::Skipped,
            "machine-config-required",
            &["build"],
        )));
        append_skipped_designer(project, project_name, &mut checks);
        return checks;
    };

    match Ibcmd::discover(version, tools) {
        Ok(ibcmd) => {
            checks.push(context(
                Check::new("build.ibcmd", CheckStatus::Pass, "compatible", &["build"])
                    .with_values(
                        Some(version.as_str().to_owned()),
                        Some(ibcmd.version().as_str().to_owned()),
                    )
                    .with_source(ibcmd.runner_kind()),
            ));
            if project.configuration().project_type() == ProjectType::Configuration {
                let check = match ibcmd.designer_available() {
                    Ok(true) => {
                        Check::new("patch.designer", CheckStatus::Pass, "available", &["patch"])
                    }
                    Ok(false) => {
                        Check::new("patch.designer", CheckStatus::Fail, "not-found", &["patch"])
                            .with_remediation("install-matching-designer")
                    }
                    Err(_) => Check::new(
                        "patch.designer",
                        CheckStatus::Fail,
                        "probe-failed",
                        &["patch"],
                    )
                    .with_remediation("check-platform-runner"),
                };
                checks.push(context(check.with_source(ibcmd.runner_kind())));
            }
        }
        Err(error) => {
            let (code, actual, source) = tool_failure(&error);
            let mut check = Check::new("build.ibcmd", CheckStatus::Fail, code, &["build"])
                .with_values(Some(version.as_str().to_owned()), actual)
                .with_remediation("configure-ibcmd");
            if let Some(source) = source {
                check = check.with_source(source);
            }
            checks.push(context(check));
            append_skipped_designer(project, project_name, &mut checks);
        }
    }
    checks
}

/// Add the patch prerequisite that applies only to full configurations.
fn append_skipped_designer(project: &Project, project_name: Option<&str>, checks: &mut Vec<Check>) {
    if project.configuration().project_type() == ProjectType::Configuration {
        checks.push(
            Check::new(
                "patch.designer",
                CheckStatus::Skipped,
                "ibcmd-required",
                &["patch"],
            )
            .for_project(project_name),
        );
    }
}

/// Inspect repository, workflow and system Git capabilities without network access.
#[must_use]
pub fn inspect_vcs(
    scope_root: &Path,
    project: &Project,
    attribute_roots: &[PathBuf],
) -> Vec<Check> {
    let mut checks = Vec::new();
    let repository = Repository::discover(scope_root);
    checks.push(match &repository {
        Ok(repository) if project.root().starts_with(repository.work_dir()) => Check::new(
            "vcs.repository",
            CheckStatus::Pass,
            "available",
            &["status", "start", "save", "switch", "finish"],
        ),
        Ok(_) => Check::new(
            "vcs.repository",
            CheckStatus::Fail,
            "project-outside-repository",
            &["status", "start", "save", "switch", "finish"],
        )
        .with_remediation("move-project-into-repository"),
        Err(_) => Check::new(
            "vcs.repository",
            CheckStatus::Fail,
            "not-found",
            &["status", "start", "save", "switch", "finish"],
        )
        .with_remediation("initialize-git-repository"),
    });

    let policy = project
        .configuration()
        .workflow_settings()
        .map(|settings| settings.resolve(None));
    checks.push(match &policy {
        Some(Ok(_)) => Check::new(
            "vcs.workflow",
            CheckStatus::Pass,
            "configured",
            &["start", "switch", "finish", "patch"],
        ),
        Some(Err(_)) => Check::new(
            "vcs.workflow",
            CheckStatus::Fail,
            "invalid",
            &["start", "switch", "finish", "patch"],
        )
        .with_remediation("correct-workflow-policy"),
        None => Check::new(
            "vcs.workflow",
            CheckStatus::Warning,
            "not-configured",
            &["start", "switch", "finish", "patch"],
        )
        .with_remediation("configure-workflow"),
    });
    checks.extend(inspect_repository_state(
        repository.as_ref().ok(),
        policy.as_ref().and_then(|policy| policy.as_ref().ok()),
    ));
    checks.extend(inspect_system_git(scope_root, attribute_roots));
    checks
}

/// Inspect repository state and the configured remote without contacting it.
fn inspect_repository_state(
    repository: Option<&Repository>,
    policy: Option<&WorkflowPolicy>,
) -> Vec<Check> {
    let Some(repository) = repository else {
        return vec![
            Check::new(
                "vcs.operation",
                CheckStatus::Skipped,
                "repository-required",
                &["start", "switch", "finish", "patch"],
            ),
            Check::new(
                "vcs.remote",
                CheckStatus::Skipped,
                "repository-required",
                &["start", "finish"],
            ),
        ];
    };
    let operation = if repository.has_in_progress_operation() {
        Check::new(
            "vcs.operation",
            CheckStatus::Fail,
            "in-progress",
            &["start", "switch", "finish", "patch"],
        )
        .with_remediation("finish-git-operation")
    } else {
        Check::new(
            "vcs.operation",
            CheckStatus::Pass,
            "clean",
            &["start", "switch", "finish", "patch"],
        )
    };
    let remote = policy.map_or_else(
        || {
            Check::new(
                "vcs.remote",
                CheckStatus::Skipped,
                "workflow-required",
                &["start", "finish"],
            )
        },
        |policy| match repository.remote(policy.remote()) {
            Ok(Some(_)) => Check::new(
                "vcs.remote",
                CheckStatus::Pass,
                "configured",
                &["start", "finish"],
            ),
            Ok(None) => Check::new(
                "vcs.remote",
                CheckStatus::Pass,
                "not-configured",
                &["start", "finish"],
            ),
            Err(_) => Check::new(
                "vcs.remote",
                CheckStatus::Fail,
                "invalid",
                &["start", "finish"],
            )
            .with_remediation("correct-remote"),
        },
    );
    vec![operation, remote]
}

/// Inspect system Git, author identity and conditional Git LFS availability.
#[must_use]
pub fn inspect_system_git(scope_root: &Path, attribute_roots: &[PathBuf]) -> Vec<Check> {
    let mut checks = Vec::new();
    let mut attribute_roots = attribute_roots.to_vec();
    if let Ok(repository) = Repository::discover(scope_root) {
        attribute_roots.push(repository.work_dir().to_path_buf());
    }

    let git = Executor::new(scope_root);
    let git_available = git.git_available();
    checks.push(if git_available {
        Check::new(
            "vcs.git",
            CheckStatus::Pass,
            "available",
            &["save", "switch", "finish"],
        )
    } else {
        Check::new(
            "vcs.git",
            CheckStatus::Fail,
            "not-found",
            &["save", "switch", "finish"],
        )
        .with_remediation("install-git")
    });
    checks.push(if !git_available {
        Check::new(
            "vcs.author",
            CheckStatus::Skipped,
            "git-required",
            &["save"],
        )
    } else if git.author_configured() {
        Check::new("vcs.author", CheckStatus::Pass, "configured", &["save"])
    } else {
        Check::new("vcs.author", CheckStatus::Fail, "not-configured", &["save"])
            .with_remediation("configure-git-author")
    });

    match lfs_required(&attribute_roots) {
        Ok(false) => checks.push(Check::new(
            "vcs.git-lfs",
            CheckStatus::Skipped,
            "not-required",
            &["save"],
        )),
        Ok(true) if !git_available => checks.push(Check::new(
            "vcs.git-lfs",
            CheckStatus::Skipped,
            "git-required",
            &["save"],
        )),
        Ok(true) if git.git_lfs_available() => checks.push(Check::new(
            "vcs.git-lfs",
            CheckStatus::Pass,
            "available",
            &["save"],
        )),
        Ok(true) => checks.push(
            Check::new("vcs.git-lfs", CheckStatus::Fail, "not-found", &["save"])
                .with_remediation("install-git-lfs"),
        ),
        Err(_) => checks.push(
            Check::new(
                "vcs.git-lfs",
                CheckStatus::Fail,
                "attributes-unreadable",
                &["save"],
            )
            .with_remediation("check-git-attributes"),
        ),
    }
    checks
}

/// Check whether any applicable attributes file selects the Git LFS filter.
fn lfs_required(roots: &[PathBuf]) -> Result<bool, io::Error> {
    let mut visited = BTreeSet::new();
    for root in roots {
        let path = root.join(".gitattributes");
        if !visited.insert(path.clone()) {
            continue;
        }
        match fs::read(&path) {
            Ok(contents)
                if contents
                    .windows(b"filter=lfs".len())
                    .any(|part| part == b"filter=lfs") =>
            {
                return Ok(true);
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(false)
}

/// Validate the root descriptor enough to know whether current build inputs exist.
fn inspect_descriptor(project: &Project) -> Check {
    match root_descriptor(project) {
        Ok(path) => Check::new(
            "project.descriptor",
            CheckStatus::Pass,
            "valid",
            &["build", "version"],
        )
        .with_values(
            None,
            path.file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned),
        ),
        Err(DescriptorError::Missing) if source_is_scaffold(project.source()) => Check::new(
            "project.descriptor",
            CheckStatus::Warning,
            "scaffold",
            &["build", "version"],
        )
        .with_remediation("export-designer-xml"),
        Err(DescriptorError::Missing) => Check::new(
            "project.descriptor",
            CheckStatus::Fail,
            "missing",
            &["build", "version"],
        )
        .with_remediation("export-designer-xml"),
        Err(DescriptorError::Ambiguous) => Check::new(
            "project.descriptor",
            CheckStatus::Fail,
            "ambiguous",
            &["build", "version"],
        )
        .with_remediation("keep-single-root-descriptor"),
        Err(DescriptorError::Invalid) => Check::new(
            "project.descriptor",
            CheckStatus::Fail,
            "invalid",
            &["build", "version"],
        )
        .with_remediation("correct-root-descriptor"),
        Err(DescriptorError::TooLarge) => Check::new(
            "project.descriptor",
            CheckStatus::Fail,
            "too-large",
            &["build", "version"],
        )
        .with_remediation("correct-root-descriptor"),
        Err(DescriptorError::Io) => Check::new(
            "project.descriptor",
            CheckStatus::Fail,
            "unreadable",
            &["build", "version"],
        )
        .with_remediation("check-source-permissions"),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DescriptorError {
    Missing,
    Ambiguous,
    Invalid,
    TooLarge,
    Io,
}

/// Locate the single safe root descriptor matching the configured project type.
fn root_descriptor(project: &Project) -> Result<PathBuf, DescriptorError> {
    let project_type = project.configuration().project_type();
    if matches!(
        project_type,
        ProjectType::Configuration | ProjectType::Extension
    ) {
        let path = project.source().join("Configuration.xml");
        return descriptor_type(&path).and_then(|actual| {
            (actual == project_type)
                .then_some(path)
                .ok_or(DescriptorError::Invalid)
        });
    }
    let entries = fs::read_dir(project.source()).map_err(|_| DescriptorError::Io)?;
    let mut found = None;
    for entry in entries {
        let entry = entry.map_err(|_| DescriptorError::Io)?;
        let path = entry.path();
        if entry.file_name() == "ConfigDumpInfo.xml"
            || !entry
                .file_type()
                .map_err(|_| DescriptorError::Io)?
                .is_file()
            || !path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
        {
            continue;
        }
        if descriptor_type(&path)? == project_type {
            if found.is_some() {
                return Err(DescriptorError::Ambiguous);
            }
            found = Some(path);
        }
    }
    found.ok_or(DescriptorError::Missing)
}

/// Read one bounded XML file and return its detected Designer project type.
fn descriptor_type(path: &Path) -> Result<ProjectType, DescriptorError> {
    let metadata = fs::metadata(path).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            DescriptorError::Missing
        } else {
            DescriptorError::Io
        }
    })?;
    if metadata.len() > MAX_DESCRIPTOR_BYTES {
        return Err(DescriptorError::TooLarge);
    }
    let input = fs::read_to_string(path).map_err(|_| DescriptorError::Io)?;
    designer_xml::project_type(&input)
        .map_err(|_| DescriptorError::Invalid)?
        .ok_or(DescriptorError::Invalid)
}

/// Recognize the untouched empty source directory created by built-in onboarding.
fn source_is_scaffold(source: &Path) -> bool {
    fs::read_dir(source).is_ok_and(|entries| {
        entries
            .filter_map(Result::ok)
            .all(|entry| entry.file_name() == ".gitkeep")
    })
}

/// Convert detailed tool failures into stable diagnostic codes and values.
fn tool_failure(error: &ToolError) -> (&'static str, Option<String>, Option<&'static str>) {
    match error {
        ToolError::InvalidArchitecture(_) => ("invalid-architecture", None, None),
        ToolError::InvalidContainer(_) => ("invalid-container", None, Some("distrobox")),
        ToolError::InvalidExecutable(_) => ("invalid-executable", None, Some("host")),
        ToolError::DistroboxContainerRequired => ("container-required", None, Some("distrobox")),
        ToolError::Scan { .. } => ("scan-failed", None, Some("host")),
        ToolError::ScanCommandFailed { .. } => ("scan-failed", None, Some("distrobox")),
        ToolError::NotFound { .. } => ("not-found", None, None),
        ToolError::Run(_) => ("run-failed", None, None),
        ToolError::VersionCommandFailed { source, .. } => {
            ("version-command-failed", None, Some(source.runner_kind()))
        }
        ToolError::VersionUnreadable(source) => {
            ("version-unreadable", None, Some(source.runner_kind()))
        }
        ToolError::VersionMismatch { actual, source, .. } => (
            "version-mismatch",
            Some(actual.as_str().to_owned()),
            Some(source.runner_kind()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{Check, CheckStatus, Report};

    #[test]
    /// Count all statuses and treat only failed checks as a failing report.
    fn report_summary_counts_each_status_and_detects_failures() {
        let mut report = Report::default();
        for status in [
            CheckStatus::Pass,
            CheckStatus::Warning,
            CheckStatus::Fail,
            CheckStatus::Skipped,
        ] {
            report.push(Check::new("test", status, "test", &[]));
        }
        assert_eq!(report.summary().passed, 1);
        assert_eq!(report.summary().warnings, 1);
        assert_eq!(report.summary().failed, 1);
        assert_eq!(report.summary().skipped, 1);
        assert!(report.has_failures());
    }
}
