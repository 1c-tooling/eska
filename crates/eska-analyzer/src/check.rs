//! Complete, one-shot analysis uses the shared project context and the published JSONL contract.

mod files;
mod jsonl;

use std::{ffi::OsStr, io, path::PathBuf};

use eska_project_core::{
    application::CheckPolicy,
    project::{metadata_model::ProjectScope, validation},
};
use serde::{Deserialize, Serialize};

use crate::{
    configuration::{self, Plan},
    process::Analyzer,
};

/// Effective diagnostics retain upstream coordinates and messages without a second rule engine.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
    pub severity: Severity,
    pub start_line: u64,
    pub start_column: u64,
    pub end_line: u64,
    pub end_column: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// Public report categories follow upstream's LSP severity mapping, independent of the CLI locale.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Severity {
    /// Published JSONL also uses these Sonar levels; upstream presents all three as LSP errors.
    #[serde(alias = "Blocker", alias = "Critical", alias = "Major")]
    Error,
    Warning,
    Information,
    Hint,
}

/// Counts refer to the complete effective diagnostic set, including baseline policy if configured.
#[derive(Debug, Default, Eq, PartialEq, Serialize)]
pub struct Counts {
    pub errors: u64,
    pub warnings: u64,
    pub information: u64,
    pub hints: u64,
}

impl Counts {
    /// Keep threshold evaluation independent of upstream's process exit behavior.
    #[must_use]
    pub fn exceeds(&self, policy: &CheckPolicy) -> bool {
        self.errors > policy.max_errors
            || policy.max_warnings.is_some_and(|max| self.warnings > max)
            || policy
                .max_information
                .is_some_and(|max| self.information > max)
            || policy.max_hints.is_some_and(|max| self.hints > max)
    }

    /// Sum only accepted, validated severities.
    const fn add(&mut self, severity: Severity) {
        match severity {
            Severity::Error => self.errors += 1,
            Severity::Warning => self.warnings += 1,
            Severity::Information => self.information += 1,
            Severity::Hint => self.hints += 1,
        }
    }
}

/// Clean files remain present, proving that an empty diagnostics array is an analyzed result.
#[derive(Debug)]
pub struct FileResult {
    pub path: PathBuf,
    pub diagnostics: Vec<Diagnostic>,
}

/// A report exists only after successful exit, complete JSONL and unchanged input checks.
#[derive(Debug)]
pub struct Report {
    pub analyzer_version: String,
    pub root: PathBuf,
    pub application: Option<String>,
    pub limited_context: bool,
    pub files: Vec<FileResult>,
    pub counts: Counts,
    pub policy: CheckPolicy,
    pub declared_objects: usize,
    pub input_fingerprint: String,
}

/// Incomplete results are errors, never a partial report with a successful status.
#[derive(Debug)]
pub enum Failure {
    Configuration(crate::Error),
    Io {
        path: PathBuf,
        source: io::Error,
    },
    UnsupportedPath {
        path: PathBuf,
    },
    PartialPolicy {
        field: &'static str,
    },
    Structure {
        root: PathBuf,
        source: Box<validation::ValidationError>,
    },
    EmptyInput,
    Stream {
        reason: &'static str,
    },
    ProcessExit {
        exit_code: Option<i32>,
        stderr: String,
    },
    InputsChanged,
}

impl Failure {
    /// Stable categories distinguish threshold failure from cancellation or an incomplete tool run.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Configuration(error) => error.code(),
            Self::Io { .. } => "io",
            Self::UnsupportedPath { .. } => "unsupported_source_path",
            Self::PartialPolicy { .. } => "partial_analysis_policy",
            Self::Structure { .. } => "structure",
            Self::EmptyInput => "empty_input",
            Self::Stream { .. } => "invalid_stream",
            Self::ProcessExit { .. } => "analyzer_exit",
            Self::InputsChanged => "inputs_changed",
        }
    }
}

impl From<crate::Error> for Failure {
    /// Preserve timeout and cancellation from the shared process owner.
    fn from(error: crate::Error) -> Self {
        Self::Configuration(error)
    }
}

/// Check all declared XML and BSL files; callers prepare the derived config without migration.
///
/// No editor server, Git process or platform is used. The lease blocks other context writers;
/// byte fingerprints detect external source/config edits during this one-shot analysis.
///
/// # Errors
/// Rejects partial policy, structural failures, interrupted processes, invalid/incomplete streams
/// and changes to the selected inputs. No failure can be interpreted as a clean report.
pub fn analyze(plan: &Plan, analyzer: &Analyzer<'_>) -> Result<Report, Failure> {
    let snapshot = files::Snapshot::capture(plan, analyzer)?;
    let mut declared_objects = 0;
    for source in plan.context.sources() {
        let scope = source.member().map_or(ProjectScope::Standalone, |name| {
            ProjectScope::Member(name.clone())
        });
        match validation::validate(source.project().clone(), scope, &|| analyzer.is_cancelled()) {
            Ok(count) => declared_objects += count,
            Err(validation::ValidationError::Cancelled) => {
                return Err(crate::Error::Cancelled.into());
            }
            Err(source_error) => {
                return Err(Failure::Structure {
                    root: source.project().root().into(),
                    source: Box::new(source_error),
                });
            }
        }
    }
    let _lease = configuration::lease(plan)?;
    let version = analyzer.validate(plan.context.root())?;
    let expected_version = version.clone();
    analyzer.check_config(plan.context.root(), &plan.path())?;
    let expected = snapshot.modules.clone();
    let config = plan.path();
    let output = analyzer.run_with_stdout(
        plan.context.root(),
        [
            OsStr::new("analyze"),
            OsStr::new("--source-dir"),
            plan.context.root().as_os_str(),
            OsStr::new("--config"),
            config.as_os_str(),
            OsStr::new("--format"),
            OsStr::new("jsonl"),
        ],
        move |stdout| Ok(jsonl::read(stdout, &expected, &expected_version)),
    )?;
    if !output.status.success() {
        return Err(Failure::ProcessExit {
            exit_code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    let files = output.stdout?;
    configuration::ensure_snapshot(plan)?;
    if snapshot != files::Snapshot::capture(plan, analyzer)? {
        return Err(Failure::InputsChanged);
    }
    let mut counts = Counts::default();
    for file in &files {
        for diagnostic in &file.diagnostics {
            counts.add(diagnostic.severity);
        }
    }
    Ok(Report {
        analyzer_version: version,
        root: plan.context.root().into(),
        application: plan.context.name().map(|name| name.as_str().to_owned()),
        limited_context: plan.context.limited_context(),
        files,
        counts,
        policy: plan.context.check_policy().clone(),
        declared_objects,
        input_fingerprint: snapshot.fingerprint(plan),
    })
}
