//! Read-only batches share input and diagnostics; only independent syntax can release each body.

mod profile;
mod query_text;
mod report;

#[cfg(test)]
mod tests;

use std::time::Duration;

use base_db::SourceDatabase;
use eska::{
    analysis_core::InputRequest,
    project_core::source_format::{SourcePath, SourceReader},
};

pub use profile::BatchProfile;
pub use query_text::{BatchQueryCoverage, BatchQueryFile, BatchQueryFinding};
pub use report::{
    BatchCoverage, BatchFile, BatchFinding, BatchReport, BatchStatus, FileState, MetadataIssue,
    StopReason,
};

use crate::{
    AnalysisSession, DiagnosticCode, DiagnosticsConfig, ModuleState, QueryError,
    RequestCancellation, SessionError, SessionOptions, SessionRequestError,
    deadline::{Deadline, DeadlineError},
};

/// Both profiles bound current source and time; syntax streams bodies, semantics retains them.
#[derive(Clone, Debug)]
pub struct BatchOptions {
    pub timeout: Duration,
    pub max_document_bytes: u64,
    pub max_retained_source_bytes: u64,
    pub cancellation: RequestCancellation,
}

impl Default for BatchOptions {
    /// Give the analysis phase five minutes and use the shared hard document bound.
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(300),
            max_document_bytes: 64 * 1024 * 1024,
            max_retained_source_bytes: SessionOptions::default().max_retained_source_bytes,
            cancellation: RequestCancellation::default(),
        }
    }
}

/// Fatal setup or invariant failures differ from ordinary findings and incomplete input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BatchError {
    Open(SessionError),
    Request(SessionRequestError),
    Query(QueryError),
    QuerySyntax(crate::QuerySyntaxRequestError),
    MetadataQuery {
        source: SourcePath,
        error: crate::MetadataDiagnosticsError,
    },
    TimerUnavailable,
    TimeoutOutOfRange,
    DiagnosticRangeInvalid {
        source: SourcePath,
        code: String,
    },
}

/// Check all declared saved input with the IDE's syntax profile, preserving explicit coverage.
///
/// Metadata models remain loaded; saved bodies are released after their independent syntax check.
/// The time bound is cooperative and applies after composition was opened by the caller.
///
/// # Errors
/// Returns opening, timer or query-invariant failures. Ordinary unread/binary/unsupported input
/// and cancellation produce an incomplete report rather than a successful empty result.
pub fn check_syntax(
    reader: SourceReader,
    config: &DiagnosticsConfig,
    options: &BatchOptions,
) -> Result<BatchReport, BatchError> {
    check(reader, config, options, BatchProfile::Syntax)
}

/// Run the configured native rules with all readable declared bodies retained before any query.
///
/// Missing, binary, unsupported or rejected inputs remain explicit incomplete coverage. This
/// profile does not substitute a syntax result for semantic analysis or validate the 1C runtime.
///
/// # Errors
/// Returns the same setup and query-invariant failures as the syntax batch.
pub fn check_semantic(
    reader: SourceReader,
    config: &DiagnosticsConfig,
    options: &BatchOptions,
) -> Result<BatchReport, BatchError> {
    check(reader, config, options, BatchProfile::Semantic)
}

/// Both profiles own the same deadline, metadata snapshot and report; only body residency differs.
fn check(
    reader: SourceReader,
    config: &DiagnosticsConfig,
    options: &BatchOptions,
    profile: BatchProfile,
) -> Result<BatchReport, BatchError> {
    let scope = profile.session_scope(&reader).map_err(BatchError::Open)?;
    let session = AnalysisSession::from_reader_with_scope(
        reader,
        profile.session_options(options.max_retained_source_bytes),
        scope,
    )
    .map_err(BatchError::Open)?;
    check_session(session, config, options, profile)
}

/// IDE and checker use the same resident current contributions for a configured composition.
///
/// # Errors
/// Returns the existing batch input, cancellation, deadline and native-query invariant failures.
pub fn check_session(
    mut session: AnalysisSession,
    config: &DiagnosticsConfig,
    options: &BatchOptions,
    profile: BatchProfile,
) -> Result<BatchReport, BatchError> {
    let deadline = Deadline::start(options.timeout, options.cancellation.clone()).map_err(
        |error| match error {
            DeadlineError::TimerUnavailable => BatchError::TimerUnavailable,
            DeadlineError::TimeoutOutOfRange => BatchError::TimeoutOutOfRange,
        },
    )?;
    let manifest_sha256 = session
        .selected_root
        .inputs
        .source_reader()
        .configuration()
        .manifest_sha256
        .clone();
    if profile == BatchProfile::Semantic {
        session
            .require_configuration_context(session.revision())
            .map_err(BatchError::Request)?;
    }
    let request = InputRequest {
        max_document_bytes: options.max_document_bytes,
        cancellation: options.cancellation.input.clone(),
        source_budget: None,
    };
    let query_config = config.clone();
    let config = match profile {
        BatchProfile::Syntax => syntax_profile(config).0,
        BatchProfile::Semantic => config.clone(),
    };
    let profile_enabled = profile.enabled(&config);
    if profile == BatchProfile::Semantic && profile_enabled {
        session
            .load_semantic_inputs(session.revision(), &request)
            .map_err(BatchError::Request)?;
    } else {
        session
            .load_pending_metadata_inputs(&request)
            .map_err(BatchError::Request)?;
    }
    let paths = session.module_paths().cloned().collect::<Vec<_>>();
    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        if !profile_enabled
            || deadline.stop_reason().is_some()
            || (profile == BatchProfile::Semantic && session.require_current_composition().is_err())
        {
            break;
        }
        let Some(file) = check_module(
            &mut session,
            &path,
            &config,
            &request,
            &options.cancellation,
            profile,
        )?
        else {
            break;
        };
        files.push(file);
        if files.len().is_multiple_of(32) {
            session.trim_batch_caches();
        }
    }
    session.trim_batch_caches();
    let (query_files, query_coverage) =
        query_text::check(&mut session, &query_config, &request, &deadline)?;
    let mut report = report::finish(
        &session,
        manifest_sha256,
        files,
        deadline.stop_reason().map(StopReason::from),
        &config,
        &options.cancellation.query,
        profile,
    )?;
    query_text::publish(&mut report, query_files, query_coverage);
    // Constructing coverage can itself cross the deadline; check again before returning success.
    if let Some(stop) = deadline.stop_reason() {
        report.stop = Some(StopReason::from(stop));
        report.status = BatchStatus::Incomplete;
        report.coverage.syntax_complete = false;
        report.query_coverage.syntax_complete = false;
        if let Some(complete) = &mut report.coverage.semantic_profile_complete {
            *complete = false;
        }
    }
    Ok(report)
}

/// IDE and batch syntax consumers preserve rule disabling, severity, locale and parameters.
#[must_use]
pub fn syntax_profile(config: &DiagnosticsConfig) -> (DiagnosticsConfig, bool) {
    let mut config = config.clone();
    let enabled = !config.is_disabled(DiagnosticCode::ParseError);
    config.only_enabled = Some(if enabled {
        vec![DiagnosticCode::ParseError]
    } else {
        vec![]
    });
    (config, enabled)
}

/// Only syntax reloads and releases one file; semantic queries retain the preloaded snapshot.
fn check_module(
    session: &mut AnalysisSession,
    path: &SourcePath,
    config: &DiagnosticsConfig,
    request: &InputRequest,
    cancellation: &RequestCancellation,
    profile: BatchProfile,
) -> Result<Option<BatchFile>, BatchError> {
    let initial = session
        .module(path)
        .ok_or_else(|| BatchError::Request(SessionRequestError::UnknownModule(path.clone())))?;
    if initial.state == ModuleState::Binary {
        return Ok(Some(BatchFile::unavailable(path, initial)));
    }
    if profile == BatchProfile::Syntax {
        session
            .load_module_saved(path, session.revision(), request)
            .map_err(BatchError::Request)?;
    }
    let input = session
        .module(path)
        .ok_or_else(|| BatchError::Request(SessionRequestError::UnknownModule(path.clone())))?;
    let mut file = BatchFile::unavailable(path, input);
    if matches!(input.state, ModuleState::Ready { .. }) {
        let diagnostics = match profile {
            BatchProfile::Syntax => session.module_syntax_diagnostics(
                path,
                session.revision(),
                config,
                &cancellation.query,
            ),
            BatchProfile::Semantic => {
                session.module_diagnostics(path, session.revision(), config, &cancellation.query)
            }
        };
        let result = match diagnostics {
            Ok(result) => result,
            Err(QueryError::Cancelled) => return Ok(None),
            Err(error) => return Err(BatchError::Query(error)),
        };
        let entry =
            session.selected_root.modules.get(path).ok_or_else(|| {
                BatchError::Request(SessionRequestError::UnknownModule(path.clone()))
            })?;
        let text = session
            .database
            .try_file_text_input(entry.file)
            .ok_or_else(|| {
                BatchError::Query(QueryError::ModuleUnavailable {
                    path: path.clone(),
                    state: ModuleState::Pending,
                })
            })?;
        file.state = FileState::Checked;
        for diagnostic in result.value {
            file.findings.push(report::finding(
                path,
                &diagnostic,
                text.text(&session.database),
            )?);
        }
    }
    if profile == BatchProfile::Syntax {
        session
            .release_module_saved(path, session.revision())
            .map_err(BatchError::Request)?;
    }
    Ok(Some(file))
}
