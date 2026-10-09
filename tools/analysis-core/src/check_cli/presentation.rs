//! Human diagnostics are localized; schema 1 machine output contains codes and precise source ranges.

use std::{
    io::{self, Write},
    path::Path,
    process::ExitCode,
};

use eska::{
    project_core::metadata_model::ProjectScope,
    tool_cli::{
        Failure, OutputFormat,
        localization::{LocalizationValue, Localizer},
    },
};

use super::settings::Settings;
use crate::{
    SessionError,
    batch::{BatchError, BatchReport, BatchStatus},
};

/// Keep findings (1), incomplete input (2) and fatal setup (3) distinct in CI.
pub fn emit(
    root: &Path,
    scope: &ProjectScope,
    settings: &Settings,
    report: &BatchReport,
    format: OutputFormat,
    localizer: &Localizer,
) -> Result<ExitCode, Failure> {
    let scope = match scope {
        ProjectScope::Standalone => serde_json::json!({"kind":"standalone"}),
        ProjectScope::Member(name) => serde_json::json!({"kind":"member", "name":name.as_str()}),
    };
    let bytes = match format {
        OutputFormat::Json => {
            let mut bytes = serde_json::to_vec(&serde_json::json!({
                "schema_version":1, "tool":"eska-check", "command":"check", "status":report.status,
                "data":{"project_root":root, "scope":scope, "settings_path":settings.path,
                    "settings_sha256":settings.sha256, "position_encoding":"unicode-scalar",
                    "report":report},
            }))
            .map_err(|_| Failure::new("tool_json"))?;
            bytes.push(b'\n');
            bytes
        }
        OutputFormat::Human => human(report, localizer).into_bytes(),
    };
    io::stdout()
        .lock()
        .write_all(&bytes)
        .map_err(|_| Failure::new("tool_io"))?;
    Ok(match report.status {
        BatchStatus::Passed => ExitCode::SUCCESS,
        BatchStatus::Findings => ExitCode::from(1),
        BatchStatus::Incomplete => ExitCode::from(2),
    })
}

/// Summarize coverage before listing ordinary localized syntax findings.
fn human(report: &BatchReport, localizer: &Localizer) -> String {
    use std::fmt::Write;
    let status = match report.status {
        BatchStatus::Passed => "tool-check-status-passed",
        BatchStatus::Findings => "tool-check-status-findings",
        BatchStatus::Incomplete => "tool-check-status-incomplete",
    };
    let status = localizer.text(status);
    let checked = report.coverage.checked_modules.to_string();
    let total = report.coverage.modules.to_string();
    let metadata = report.metadata_issues.len().to_string();
    let findings = report
        .files
        .iter()
        .map(|file| file.findings.len())
        .sum::<usize>()
        .to_string();
    let mut text = localizer.format(
        "tool-check-result",
        &[
            ("status", LocalizationValue::Text(&status)),
            ("checked", LocalizationValue::Text(&checked)),
            ("total", LocalizationValue::Text(&total)),
            ("metadata", LocalizationValue::Text(&metadata)),
            ("findings", LocalizationValue::Text(&findings)),
        ],
    );
    text.push('\n');
    if let Some(stop) = report.stop {
        let key = match stop {
            crate::batch::StopReason::DeadlineExceeded => "tool-check-deadline",
            crate::batch::StopReason::Cancelled => "tool-check-cancelled",
        };
        text.push_str(&localizer.text(key));
        text.push('\n');
    }
    for file in &report.files {
        for finding in &file.findings {
            let _ = writeln!(
                text,
                "{}:{}:{}: {} ({})",
                file.source.as_str(),
                finding.start_line + 1,
                finding.start_column + 1,
                finding.message,
                finding.code
            );
        }
    }
    text
}

/// Setup failures retain stable coordinates without serializing a Debug representation.
pub fn batch_failure(error: BatchError) -> Failure {
    match error {
        BatchError::Open(SessionError::Source(error)) => error.into(),
        BatchError::Open(SessionError::NonUtf8Root) => Failure::new("path_encoding_unsupported"),
        BatchError::Open(SessionError::TooManyFiles) => Failure::new("analysis_file_id_exhausted"),
        BatchError::Open(SessionError::MissingListing) => {
            Failure::new("analysis_kernel_structure_missing")
        }
        BatchError::TimerUnavailable => Failure::new("analysis_timer_unavailable"),
        BatchError::TimeoutOutOfRange => Failure::new("analysis_timeout_invalid"),
        BatchError::DiagnosticRangeInvalid { source, code } => {
            Failure::new("analysis_diagnostic_range_invalid")
                .at(source.as_str())
                .at(code)
        }
        BatchError::Request(_) | BatchError::Query(_) => Failure::new("analysis_internal"),
    }
}
