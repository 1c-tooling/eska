//! Standalone query batches stream the same host inputs with explicit syntax or metadata scope.

use eska::{
    analysis_core::InputRequest,
    project_core::source_format::{SourcePath, TextCoordinates, TextRange},
};

use crate::{
    AnalysisSession, DiagnosticCode, DiagnosticsConfig, QuerySyntaxError, QuerySyntaxRequestError,
    QueryTextSnapshot, QueryTextState, SessionRequestError, SessionRevision, deadline::Deadline,
};

use super::{BatchError, BatchFinding, BatchProfile, BatchReport, BatchStatus, FileState, report};

mod analysis;
mod consumers;
#[cfg(test)]
mod tests;

/// The existing syntax serialization is retained while contextual queries identify their admitted scope.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchQueryScope {
    SyntaxOnly,
    CurrentCanonicalMetadata,
}

/// Semantic query results bind the current input and native root without claiming consumer coverage.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct BatchQueryContext {
    pub root: crate::composition::RootId,
    pub input_revision: SessionRevision,
    pub composition_sha256: Option<String>,
    pub metadata_complete: bool,
}

/// Exact byte offsets and zero-based UTF-16 ranges refer to the same current plain query text.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct BatchQueryFinding {
    #[serde(flatten)]
    pub finding: BatchFinding,
    pub range: TextRange,
}

/// Parse coverage is independent of metadata/consumer completeness, even for a clean query.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct BatchQueryFile {
    pub source: SourcePath,
    pub revision: SessionRevision,
    pub source_sha256: Option<String>,
    pub scope: BatchQueryScope,
    pub position_encoding: &'static str,
    pub state: FileState,
    pub findings: Vec<BatchQueryFinding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<BatchQueryContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumers: Option<crate::QueryConsumerReportStatus>,
}

/// Missing/disabled/stopped checks remain separate from a checked empty result.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize)]
pub struct BatchQueryCoverage {
    pub queries: usize,
    pub checked: usize,
    pub unavailable: usize,
    pub unchecked: usize,
    pub syntax_complete: bool,
    pub metadata_complete: bool,
    pub consumer_complete: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters_complete: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_complete: Option<bool>,
}

/// Disabled rules and cancellation stop before reading; each accepted saved text is released after use.
pub(super) fn check(
    session: &mut AnalysisSession,
    config: &DiagnosticsConfig,
    request: &InputRequest,
    deadline: &Deadline,
    profile: BatchProfile,
    cancellation: &crate::QueryCancellation,
) -> Result<(Vec<BatchQueryFile>, BatchQueryCoverage), BatchError> {
    let paths = session.query_paths().cloned().collect::<Vec<_>>();
    let enabled = match profile {
        BatchProfile::Syntax => !config.is_disabled(DiagnosticCode::QueryParseError),
        BatchProfile::Semantic => crate::query_analysis_enabled(config),
    };
    let mut files = Vec::with_capacity(paths.len());
    for path in &paths {
        if !enabled || deadline.stop_reason().is_some() {
            break;
        }
        let file = match profile {
            BatchProfile::Syntax => check_one(session, path, config, request)?,
            BatchProfile::Semantic => {
                analysis::check_one(session, path, config, request, cancellation)?
            }
        };
        let Some(file) = file else {
            break;
        };
        files.push(file);
    }
    let checked = files
        .iter()
        .filter(|file| file.state == FileState::Checked)
        .count();
    let coverage = BatchQueryCoverage {
        queries: paths.len(),
        checked,
        unavailable: files.len() - checked,
        unchecked: paths.len() - files.len(),
        syntax_complete: paths.is_empty()
            || (enabled
                && !config.is_disabled(DiagnosticCode::QueryParseError)
                && checked == paths.len()
                && deadline.stop_reason().is_none()),
        metadata_complete: profile == BatchProfile::Semantic
            && enabled
            && checked == paths.len()
            && deadline.stop_reason().is_none(),
        consumer_complete: false,
        parameters_complete: (profile == BatchProfile::Semantic).then_some(false),
        semantic_complete: (profile == BatchProfile::Semantic).then_some(false),
    };
    Ok((files, coverage))
}

/// Global BSL coverage stays intact; declared queries require their own real syntax/context coverage.
pub(super) fn publish(
    report: &mut BatchReport,
    files: Vec<BatchQueryFile>,
    coverage: BatchQueryCoverage,
) {
    if coverage.queries > 0
        && (!coverage.syntax_complete || report.profile == BatchProfile::Semantic)
    {
        report.status = BatchStatus::Incomplete;
    } else if report.status == BatchStatus::Passed
        && files.iter().any(|file| !file.findings.is_empty())
    {
        report.status = BatchStatus::Findings;
    }
    report.query_files = files;
    report.query_coverage = coverage;
}

/// Unavailable current input is recorded before a native query; a cancelled result cannot become checked.
fn check_one(
    session: &mut AnalysisSession,
    path: &SourcePath,
    config: &DiagnosticsConfig,
    request: &InputRequest,
) -> Result<Option<BatchQueryFile>, BatchError> {
    session
        .load_query_saved(path, session.revision(), request)
        .map_err(BatchError::Request)?;
    let input = session
        .query_input(path)
        .ok_or_else(|| BatchError::Request(SessionRequestError::UnknownQuery(path.clone())))?;
    let mut file = unavailable(path, input);
    if let QueryTextState::Ready(text) = &input.state {
        match session.query_syntax_diagnostics(path, session.revision(), config, request) {
            Ok(result) => {
                file.revision = result.revision;
                file.state = FileState::Checked;
                file.findings = findings(path, text, &result.result.diagnostics)?;
            }
            Err(QuerySyntaxRequestError::Syntax(QuerySyntaxError::Cancelled)) => {
                session
                    .release_query_saved(path, session.revision())
                    .map_err(BatchError::Request)?;
                return Ok(None);
            }
            Err(error) => return Err(BatchError::QuerySyntax(error)),
        }
    }
    session
        .release_query_saved(path, session.revision())
        .map_err(BatchError::Request)?;
    Ok(Some(file))
}

/// Source failure codes remain locale-independent and never acquire a successful empty fingerprint.
fn unavailable(path: &SourcePath, input: &QueryTextSnapshot) -> BatchQueryFile {
    let state = if let QueryTextState::Unavailable(failure) = &input.state {
        let (code, location) = report::input_failure(failure);
        FileState::Unavailable { code, location }
    } else {
        FileState::Unavailable {
            code: "query_pending".into(),
            location: vec![],
        }
    };
    BatchQueryFile {
        source: path.clone(),
        revision: input.revision,
        source_sha256: input.source_sha256.clone(),
        scope: BatchQueryScope::SyntaxOnly,
        position_encoding: "utf-16",
        state,
        findings: vec![],
        context: None,
        consumers: None,
    }
}

/// Convert all native UTF-8 endpoints together, retaining existing checker scalar locations for human text.
fn findings(
    path: &SourcePath,
    text: &str,
    diagnostics: &[crate::Diagnostic],
) -> Result<Vec<BatchQueryFinding>, BatchError> {
    let offsets = diagnostics
        .iter()
        .flat_map(|diagnostic| {
            [
                u32::from(diagnostic.range.start()) as usize,
                u32::from(diagnostic.range.end()) as usize,
            ]
        })
        .collect::<Vec<_>>();
    let invalid = || BatchError::DiagnosticRangeInvalid {
        source: path.clone(),
        code: DiagnosticCode::QueryParseError.as_str().into(),
    };
    let coordinates = TextCoordinates::new(text, &offsets).map_err(|_| invalid())?;
    diagnostics
        .iter()
        .map(|diagnostic| {
            Ok(BatchQueryFinding {
                finding: report::finding(path, diagnostic, text)?,
                range: coordinates
                    .range(
                        u32::from(diagnostic.range.start()) as usize
                            ..u32::from(diagnostic.range.end()) as usize,
                    )
                    .map_err(|_| invalid())?,
            })
        })
        .collect()
}
