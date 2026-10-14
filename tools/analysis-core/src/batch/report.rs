//! Stable machine fields never serialize a translated diagnostic message.

use eska::{analysis_core::InputFailure, project_core::source_format::SourcePath};

use crate::{
    AnalysisScope, AnalysisSession, Diagnostic, DiagnosticCode, DiagnosticsConfig,
    MetadataDiagnostic, MetadataDiagnosticsError, ModelState, ModuleFailure, ModuleInput,
    ModuleState, QueryCancellation, ResourceSummary, SessionRevision,
};

use super::{BatchError, BatchProfile};

/// Findings and incompleteness are separate from a fatal setup error.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchStatus {
    Passed,
    Findings,
    Incomplete,
}

/// Stopping is explicit; unvisited declarations are not counted as checked.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    DeadlineExceeded,
    Cancelled,
}

impl From<crate::cancellation::RequestStop> for StopReason {
    /// The batch schema preserves its existing names while the request control is shared.
    fn from(reason: crate::cancellation::RequestStop) -> Self {
        match reason {
            crate::cancellation::RequestStop::Cancelled => Self::Cancelled,
            crate::cancellation::RequestStop::DeadlineExceeded => Self::DeadlineExceeded,
        }
    }
}

/// The module report retains the checked bytes' identity after resident text was released.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FileState {
    Checked,
    Binary,
    Unavailable { code: String, location: Vec<String> },
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct BatchFile {
    pub source: SourcePath,
    pub revision: SessionRevision,
    pub source_sha256: Option<String>,
    pub state: FileState,
    pub findings: Vec<BatchFinding>,
}

/// Lines and columns are zero-based Unicode scalar positions; byte ranges retain exact UTF-8.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct BatchFinding {
    pub code: String,
    pub severity: String,
    pub start_byte: u32,
    pub end_byte: u32,
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
    #[serde(skip)]
    pub message: String,
}

/// A YAML issue has the current fingerprint and range, never coordinates in exported XML.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct MetadataIssue {
    pub source: SourcePath,
    pub source_sha256: Option<String>,
    pub code: String,
    pub location: Vec<String>,
    pub range: Option<std::ops::Range<usize>>,
    /// Nested failures retain full descriptor identities and never borrow the main object's range.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub forms: Vec<FormIssue>,
    /// `None` means no current diagnostic walk completed; an empty vector is a checked local result.
    #[serde(skip)]
    pub diagnostics: Option<Vec<MetadataDiagnostic>>,
}

/// A form dependency failure is visible even when its enclosing metadata object is ready.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct FormIssue {
    pub native_path: SourcePath,
    pub code: String,
    pub location: Vec<String>,
    pub range: Option<std::ops::Range<usize>>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize)]
pub struct BatchCoverage {
    pub metadata_owners: usize,
    pub metadata_inputs_complete: bool,
    pub metadata_models_complete: bool,
    pub modules: usize,
    pub checked_modules: usize,
    pub binary_modules: usize,
    pub unavailable_modules: usize,
    pub unchecked_modules: usize,
    pub syntax_complete: bool,
    /// All configured rules ran over complete supported inputs; absent for the syntax profile.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_profile_complete: Option<bool>,
}

/// Profile coverage never claims platform or runtime validation.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct BatchReport {
    pub analysis_scope: AnalysisScope,
    pub profile: BatchProfile,
    pub profile_enabled: bool,
    pub status: BatchStatus,
    pub manifest_sha256: String,
    pub coverage: BatchCoverage,
    pub resources: ResourceSummary,
    pub stop: Option<StopReason>,
    pub metadata_issues: Vec<MetadataIssue>,
    pub files: Vec<BatchFile>,
}

impl BatchFile {
    /// Project an unread/binary state before querying; successful syntax later changes it to checked.
    #[must_use]
    pub fn unavailable(path: &SourcePath, input: &ModuleInput) -> Self {
        let state = match &input.state {
            ModuleState::Binary => FileState::Binary,
            ModuleState::Unavailable(ModuleFailure::Input(failure)) => {
                let (code, location) = input_failure(failure);
                FileState::Unavailable { code, location }
            }
            ModuleState::Unavailable(ModuleFailure::LoadedTextBudgetExceeded { .. }) => {
                FileState::Unavailable {
                    code: "module_text_budget_exceeded".into(),
                    location: vec![],
                }
            }
            ModuleState::Pending | ModuleState::Ready { .. } => FileState::Unavailable {
                code: "module_pending".into(),
                location: vec![],
            },
        };
        Self {
            source: path.clone(),
            revision: input.revision,
            source_sha256: input.source_sha256.clone(),
            state,
            findings: vec![],
        }
    }
}

/// Validate kernel coordinates before calling its conversion routine, which panics on bad ranges.
pub fn finding(
    path: &SourcePath,
    diagnostic: &Diagnostic,
    text: &str,
) -> Result<BatchFinding, BatchError> {
    let start_byte = u32::from(diagnostic.range.start());
    let end_byte = u32::from(diagnostic.range.end());
    if !text.is_char_boundary(start_byte as usize) || !text.is_char_boundary(end_byte as usize) {
        return Err(BatchError::DiagnosticRangeInvalid {
            source: path.clone(),
            code: diagnostic.code.as_str().into(),
        });
    }
    let output = diagnostic.to_output(text);
    Ok(BatchFinding {
        code: output.code,
        severity: output.severity,
        start_byte,
        end_byte,
        start_line: output.start_line,
        start_column: output.start_column,
        end_line: output.end_line,
        end_column: output.end_column,
        message: output.message,
    })
}

/// The batch counts completed checks, rather than pretending its released bodies remain resident.
pub fn finish(
    session: &AnalysisSession,
    manifest_sha256: String,
    files: Vec<BatchFile>,
    stop: Option<StopReason>,
    config: &DiagnosticsConfig,
    cancellation: &QueryCancellation,
    profile: BatchProfile,
) -> Result<BatchReport, BatchError> {
    let summary = session.summary();
    let mut coverage = BatchCoverage {
        metadata_owners: summary.inputs.owners,
        metadata_inputs_complete: summary.inputs.metadata_input_complete,
        metadata_models_complete: summary.descriptor_models_complete,
        modules: session.module_paths().count(),
        ..BatchCoverage::default()
    };
    for file in &files {
        match file.state {
            FileState::Checked => coverage.checked_modules += 1,
            FileState::Binary => coverage.binary_modules += 1,
            FileState::Unavailable { .. } => coverage.unavailable_modules += 1,
        }
    }
    coverage.unchecked_modules = coverage.modules - files.len();
    let all_checked = stop.is_none() && coverage.checked_modules == coverage.modules;
    coverage.syntax_complete = !config.is_disabled(DiagnosticCode::ParseError) && all_checked;
    coverage.semantic_profile_complete = (profile == BatchProfile::Semantic).then(|| {
        profile.enabled(config)
            && all_checked
            && coverage.metadata_inputs_complete
            && coverage.metadata_models_complete
            && session.module_summary().module_input_complete
    });
    let complete = coverage
        .semantic_profile_complete
        .unwrap_or(coverage.syntax_complete);
    let status = if !coverage.metadata_models_complete || !complete {
        BatchStatus::Incomplete
    } else if files.iter().any(|file| !file.findings.is_empty()) {
        BatchStatus::Findings
    } else {
        BatchStatus::Passed
    };
    let metadata_issues = session
        .owner_paths()
        .filter_map(|path| metadata_issue(session, path, cancellation).transpose())
        .collect::<Result<Vec<_>, _>>()?;
    Ok(BatchReport {
        analysis_scope: session.scope(),
        profile,
        profile_enabled: profile.enabled(config),
        status,
        manifest_sha256,
        coverage,
        resources: session.resources(),
        stop,
        metadata_issues,
        files,
    })
}

/// Preserve unsupported/invalid/pending YAML separately from successful syntax diagnostics.
fn metadata_issue(
    session: &AnalysisSession,
    path: &SourcePath,
    cancellation: &QueryCancellation,
) -> Result<Option<MetadataIssue>, BatchError> {
    let Some(model) = session.owner(path) else {
        return Ok(None);
    };
    let (code, location, range) = match &model.state {
        ModelState::Pending => ("metadata_pending".into(), vec![], None),
        ModelState::Unsupported(_) => ("metadata_projection_unsupported".into(), vec![], None),
        ModelState::SourceOnly => ("analysis_context_required".into(), vec![], None),
        ModelState::Unavailable(failure) => {
            let (code, location) = input_failure(failure);
            (code, location, None)
        }
        ModelState::Invalid { error, .. } => (
            "metadata_projection_invalid".into(),
            vec![],
            Some(error.range.clone()),
        ),
        ModelState::Ready(_) | ModelState::StructureOnly if !model.descriptors_complete() => {
            ("metadata_descriptors_unmodeled".into(), vec![], None)
        }
        ModelState::Ready(_) | ModelState::StructureOnly => return Ok(None),
    };
    let diagnostics = match session.metadata_diagnostics(path, session.revision(), cancellation) {
        Ok(report) => Some(report.findings),
        Err(
            MetadataDiagnosticsError::Pending
            | MetadataDiagnosticsError::Unavailable(_)
            | MetadataDiagnosticsError::Cancelled,
        ) => None,
        Err(error) => {
            return Err(BatchError::MetadataQuery {
                source: path.clone(),
                error,
            });
        }
    };
    Ok(Some(MetadataIssue {
        source: path.clone(),
        source_sha256: model.source_sha256.clone(),
        code,
        location,
        range,
        forms: model
            .forms
            .iter()
            .filter_map(|form| {
                let (code, location, range) = match &form.state {
                    ModelState::Pending => ("metadata_pending".into(), vec![], None),
                    ModelState::Unavailable(failure) => {
                        let (code, location) = input_failure(failure);
                        (code, location, None)
                    }
                    ModelState::Invalid { error, .. } => (
                        "metadata_projection_invalid".into(),
                        vec![],
                        Some(error.range.clone()),
                    ),
                    ModelState::Unsupported(_) => {
                        ("metadata_projection_unsupported".into(), vec![], None)
                    }
                    ModelState::SourceOnly => ("analysis_context_required".into(), vec![], None),
                    ModelState::Ready(_) | ModelState::StructureOnly => return None,
                };
                Some(FormIssue {
                    native_path: form.native_path.clone(),
                    code,
                    location,
                    range,
                })
            })
            .collect(),
        diagnostics,
    }))
}

/// Domain input errors already have stable codes; protocol fields never contain Debug or messages.
fn input_failure(failure: &InputFailure) -> (String, Vec<String>) {
    match failure {
        InputFailure::Source(error) => (error.code.into(), error.location.clone()),
        InputFailure::Cancelled => ("analysis_cancelled".into(), vec![]),
        InputFailure::DocumentTooLarge { .. } => ("source_document_too_large".into(), vec![]),
        InputFailure::SourceBudgetExceeded { .. } => {
            ("analysis_source_budget_exceeded".into(), vec![])
        }
    }
}
