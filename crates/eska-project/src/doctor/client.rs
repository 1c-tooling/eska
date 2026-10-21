//! Independent diagnostic owners report through a strict bounded process boundary.

use eska_presentation::localization::Localizer;
use eska_project_core::{
    doctor::{Check, CheckStatus, Report},
    project::{Project, ProjectType},
};
use eska_tool_runtime::{Component, Error};
use serde::Deserialize;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

#[derive(Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Packet {
    checks: Vec<Check>,
    details: Vec<Option<String>>,
}

impl Packet {
    /// Preserve the original machine-first ordering without hiding independent project checks.
    pub fn append_machine(&self, report: &mut Report, details: &mut Vec<Option<String>>) {
        if let (Some(check), Some(detail)) = (self.checks.first(), self.details.first()) {
            report.push(check.clone());
            details.push(detail.clone());
        }
    }

    /// Reject missing or duplicate native prerequisites rather than reporting a partial success.
    pub fn for_project(
        &self,
        project: &Project,
        name: Option<&str>,
        localizer: &Localizer,
    ) -> Self {
        if self
            .checks
            .first()
            .is_some_and(|check| check.id == "component.build")
        {
            return Self::default();
        }
        let mut packet = Self::default();
        for (check, detail) in self.checks.iter().zip(&self.details).skip(1) {
            if check.project.as_deref() == name {
                packet.checks.push(check.clone());
                packet.details.push(detail.clone());
            }
        }
        let expected = if project.configuration().project_type() == ProjectType::Configuration {
            &["build.platform-version", "build.ibcmd", "patch.designer"][..]
        } else {
            &["build.platform-version", "build.ibcmd"][..]
        };
        if packet
            .checks
            .iter()
            .map(|check| check.id.as_str())
            .ne(expected.iter().copied())
        {
            return failure(Component::Build, &Error::InvalidResponse, localizer);
        }
        packet
    }

    /// Add a complete owner report and keep localized human-only details aligned.
    pub fn append(self, report: &mut Report, details: &mut Vec<Option<String>>) {
        report.extend(self.checks);
        details.extend(self.details);
    }
}

/// A missing or failed native owner becomes one explicit failure while XML and Git continue.
pub(super) fn native(root: &Path, options: &[OsString], localizer: &Localizer) -> Packet {
    let result = query(Component::Build, root, options).and_then(|packet| {
        if packet
            .checks
            .first()
            .is_none_or(|check| check.id != "machine.config" || check.project.is_some())
        {
            return Err(Error::InvalidResponse);
        }
        Ok(packet)
    });
    result.unwrap_or_else(|error| failure(Component::Build, &error, localizer))
}

/// VCS receives only the chosen project and applicable attribute roots, preserving native paths.
pub(super) fn vcs(
    root: &Path,
    project: Option<&Path>,
    attributes: &[PathBuf],
    localizer: &Localizer,
) -> Packet {
    let mut options = Vec::new();
    if let Some(project) = project {
        options.extend(["--project".into(), project.as_os_str().to_owned()]);
    }
    for root in attributes {
        options.extend(["--attribute-root".into(), root.as_os_str().to_owned()]);
    }
    let expected = if project.is_some() {
        &[
            "vcs.repository",
            "vcs.workflow",
            "vcs.operation",
            "vcs.remote",
            "vcs.git",
            "vcs.author",
            "vcs.git-lfs",
        ][..]
    } else {
        &["vcs.git", "vcs.author", "vcs.git-lfs"][..]
    };
    let result = query(Component::Vcs, root, &options).and_then(|packet| {
        if packet
            .checks
            .iter()
            .map(|check| check.id.as_str())
            .ne(expected.iter().copied())
        {
            return Err(Error::InvalidResponse);
        }
        Ok(packet)
    });
    result.unwrap_or_else(|error| failure(Component::Vcs, &error, localizer))
}

/// Require every check to have a corresponding detail before accepting a successful child exit.
fn query(component: Component, root: &Path, options: &[OsString]) -> Result<Packet, Error> {
    let binary = eska_tool_runtime::resolve(component)?;
    let policy = eska_tool_runtime::QueryPolicy {
        cancellation_grace: if matches!(component, Component::Build) {
            std::time::Duration::from_secs(30)
        } else {
            std::time::Duration::ZERO
        },
        ..eska_tool_runtime::QueryPolicy::default()
    };
    let value =
        eska_tool_runtime::query_with_policy(component, &binary, "doctor", root, options, policy)?;
    let packet: Packet = serde_json::from_value(value).map_err(|_| Error::InvalidResponse)?;
    if packet.checks.is_empty() || packet.checks.len() != packet.details.len() {
        return Err(Error::InvalidResponse);
    }
    Ok(packet)
}

/// Keep protocol failure classification locale-independent and presentation owned by the CLI.
fn failure(component: Component, error: &Error, localizer: &Localizer) -> Packet {
    let id = match component {
        Component::Build => "component.build",
        _ => "component.vcs",
    };
    Packet {
        checks: vec![Check::new(
            id,
            CheckStatus::Fail,
            "component-failed",
            &["doctor"],
        )],
        details: vec![Some(eska_presentation::components::present(
            error, localizer,
        ))],
    }
}
