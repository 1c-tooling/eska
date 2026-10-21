//! Native prerequisites belong to the platform owner, independently from Git and project servers.

use crate::build::{Ibcmd, ToolError, ToolOptions};
use eska_project_core::{
    doctor::{Check, CheckStatus},
    project::{Project, ProjectType},
};

/// Inspect platform requirements shared by build and patch commands.
#[must_use]
pub fn inspect(
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
