//! Portable diagnostic reports and read-only project source checks.

use crate::project::{Project, ProjectType, designer_xml};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};
const MAX_DESCRIPTOR_BYTES: u64 = 64 * 1024 * 1024;

/// Severity of one stable diagnostic check.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Warning,
    Fail,
    Skipped,
}

/// One locale-independent diagnostic result.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub id: String,
    pub status: CheckStatus,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub commands: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
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
            id: id.to_owned(),
            status,
            code: code.to_owned(),
            project: None,
            commands: commands.iter().map(|value| (*value).to_owned()).collect(),
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
    pub fn with_source(mut self, source: &'static str) -> Self {
        self.source = Some(source.to_owned());
        self
    }

    /// Attach a stable remediation identifier.
    #[must_use]
    pub fn with_remediation(mut self, remediation: &'static str) -> Self {
        self.remediation = Some(remediation.to_owned());
        self
    }
}

/// Counts for the complete diagnostic report.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
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
pub fn inspect_project(project: &Project, project_name: Option<&str>) -> Vec<Check> {
    let context = |check: Check| check.for_project(project_name);
    vec![
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
    ]
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
