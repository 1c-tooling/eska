//! Read-only syntax batches use the resident session's exact input and diagnostic paths.

mod deadline;
mod report;

#[cfg(test)]
mod tests;

use std::time::Duration;

use base_db::SourceDatabase;
use eska::{
    analysis_core::{Cancellation, InputRequest},
    project_core::source_format::{SourcePath, SourceReader},
};

pub use report::{
    BatchCoverage, BatchFile, BatchFinding, BatchReport, BatchStatus, FileState, MetadataIssue,
    StopReason,
};

use crate::{
    AnalysisSession, DiagnosticCode, DiagnosticsConfig, ModuleState, QueryCancellation, QueryError,
    SessionError, SessionOptions, SessionRequestError,
};

/// A caller can cancel reads and the current kernel query using the same request control.
#[derive(Clone, Debug, Default)]
pub struct BatchCancellation {
    input: Cancellation,
    query: QueryCancellation,
}

impl BatchCancellation {
    /// Stop this whole batch cooperatively; cancellation does not change project source files.
    pub fn cancel(&self) {
        self.input.cancel();
        self.query.cancel();
    }
}

/// A syntax sweep keeps one body resident and trims heavy memos between bounded chunks.
#[derive(Clone, Debug)]
pub struct BatchOptions {
    pub timeout: Duration,
    pub max_document_bytes: u64,
    pub cancellation: BatchCancellation,
}

impl Default for BatchOptions {
    /// Give the analysis phase five minutes and use the shared hard document bound.
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(300),
            max_document_bytes: 64 * 1024 * 1024,
            cancellation: BatchCancellation::default(),
        }
    }
}

/// Fatal setup or invariant failures differ from ordinary findings and incomplete input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BatchError {
    Open(SessionError),
    Request(SessionRequestError),
    Query(QueryError),
    TimerUnavailable,
    TimeoutOutOfRange,
    DiagnosticRangeInvalid { source: SourcePath, code: String },
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
    let deadline = deadline::Deadline::start(options.timeout, options.cancellation.clone())?;
    let manifest_sha256 = reader.configuration().manifest_sha256.clone();
    let mut session = AnalysisSession::from_reader(reader, SessionOptions::default())
        .map_err(BatchError::Open)?;
    let request = InputRequest {
        max_document_bytes: options.max_document_bytes,
        cancellation: options.cancellation.input.clone(),
    };
    let owners = session.owner_paths().cloned().collect::<Vec<_>>();
    for path in owners {
        if deadline.stop_reason().is_some() {
            break;
        }
        session
            .load_saved(&path, session.revision(), &request)
            .map_err(BatchError::Request)?;
    }
    let mut config = config.clone();
    let syntax_enabled = !config.is_disabled(DiagnosticCode::ParseError);
    config.only_enabled = Some(if syntax_enabled {
        vec![DiagnosticCode::ParseError]
    } else {
        vec![]
    });
    let paths = session.module_paths().cloned().collect::<Vec<_>>();
    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        if !syntax_enabled || deadline.stop_reason().is_some() {
            break;
        }
        let Some(file) = check_module(
            &mut session,
            &path,
            &config,
            &request,
            &options.cancellation,
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
    let mut report = report::finish(
        &session,
        manifest_sha256,
        files,
        deadline.stop_reason(),
        syntax_enabled,
    );
    // Constructing coverage can itself cross the deadline; check again before returning success.
    if let Some(stop) = deadline.stop_reason() {
        report.stop = Some(stop);
        report.status = BatchStatus::Incomplete;
        report.coverage.syntax_complete = false;
    }
    Ok(report)
}

/// One syntax file is independent of unread neighbors; never use this streaming rule for semantics.
fn check_module(
    session: &mut AnalysisSession,
    path: &SourcePath,
    config: &DiagnosticsConfig,
    request: &InputRequest,
    cancellation: &BatchCancellation,
) -> Result<Option<BatchFile>, BatchError> {
    let initial = session
        .module(path)
        .ok_or_else(|| BatchError::Request(SessionRequestError::UnknownModule(path.clone())))?;
    if initial.state == ModuleState::Binary {
        return Ok(Some(BatchFile::unavailable(path, initial)));
    }
    session
        .load_module_saved(path, session.revision(), request)
        .map_err(BatchError::Request)?;
    let input = session
        .module(path)
        .ok_or_else(|| BatchError::Request(SessionRequestError::UnknownModule(path.clone())))?;
    let mut file = BatchFile::unavailable(path, input);
    if matches!(input.state, ModuleState::Ready { .. }) {
        let result =
            match session.module_diagnostics(path, session.revision(), config, &cancellation.query)
            {
                Ok(result) => result,
                Err(QueryError::Cancelled) => return Ok(None),
                Err(error) => return Err(BatchError::Query(error)),
            };
        let entry = session
            .modules
            .get(path)
            .ok_or_else(|| BatchError::Request(SessionRequestError::UnknownModule(path.clone())))?;
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
    session
        .release_module_saved(path, session.revision())
        .map_err(BatchError::Request)?;
    Ok(Some(file))
}
