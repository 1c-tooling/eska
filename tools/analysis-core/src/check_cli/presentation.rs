//! Human summaries are localized; schema 1 machine output contains codes and precise source ranges.

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
    MetadataDiagnosticsError, SessionError,
    batch::{BatchError, BatchReport, BatchStatus},
    tool_metadata,
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
                    "metadata_position_encoding":"utf-16", "report":machine_report(report)?},
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

/// JSON presentation adds shared findings while the domain report stays independent of tool features.
fn machine_report(report: &BatchReport) -> Result<serde_json::Value, Failure> {
    let mut value = serde_json::to_value(report).map_err(|_| Failure::new("tool_json"))?;
    let issues = value
        .get_mut("metadata_issues")
        .and_then(serde_json::Value::as_array_mut)
        .filter(|issues| issues.len() == report.metadata_issues.len())
        .ok_or_else(|| Failure::new("tool_json"))?;
    for (value, issue) in issues.iter_mut().zip(&report.metadata_issues) {
        let fields = value
            .as_object_mut()
            .ok_or_else(|| Failure::new("tool_json"))?;
        let findings = issue.diagnostics.as_ref().map(|findings| {
            findings
                .iter()
                .map(tool_metadata::finding)
                .collect::<Vec<_>>()
        });
        fields.insert("findings".into(), serde_json::json!(findings));
    }
    Ok(value)
}

/// Summarize profile coverage before listing findings in their supported presentation.
fn human(report: &BatchReport, localizer: &Localizer) -> String {
    use std::fmt::Write;
    let status = match (report.profile, report.status) {
        (_, BatchStatus::Incomplete) => "tool-check-status-incomplete",
        (crate::batch::BatchProfile::Syntax, BatchStatus::Passed) => "tool-check-status-passed",
        (crate::batch::BatchProfile::Syntax, BatchStatus::Findings) => "tool-check-status-findings",
        (crate::batch::BatchProfile::Semantic, BatchStatus::Passed) => "tool-check-semantic-passed",
        (crate::batch::BatchProfile::Semantic, BatchStatus::Findings) => {
            "tool-check-semantic-findings"
        }
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
        match report.profile {
            crate::batch::BatchProfile::Syntax => "tool-check-result",
            crate::batch::BatchProfile::Semantic => "tool-check-semantic-result",
        },
        &[
            ("status", LocalizationValue::Text(&status)),
            ("checked", LocalizationValue::Text(&checked)),
            ("total", LocalizationValue::Text(&total)),
            ("metadata", LocalizationValue::Text(&metadata)),
            ("findings", LocalizationValue::Text(&findings)),
        ],
    );
    text.push('\n');
    let quota_exceeded = report.metadata_issues.iter().any(|issue| {
        issue.code == "analysis_source_budget_exceeded"
            || issue
                .forms
                .iter()
                .any(|form| form.code == "analysis_source_budget_exceeded")
    }) || report.files.iter().any(|file| {
        matches!(&file.state, crate::batch::FileState::Unavailable { code, .. }
                if code == "analysis_source_budget_exceeded")
    });
    if quota_exceeded {
        let used = report.resources.total_source_bytes.to_string();
        let limit = report.resources.max_source_bytes.to_string();
        text.push_str(&localizer.format(
            "tool-check-source-budget",
            &[
                ("used", LocalizationValue::Text(&used)),
                ("limit", LocalizationValue::Text(&limit)),
            ],
        ));
        text.push('\n');
    }
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
            // Semantic kernel prose is opaque and not consistently localized; present stable
            // rule/severity data in both locales until the kernel provides structured messages.
            let (label, detail) = match report.profile {
                crate::batch::BatchProfile::Syntax => (&finding.message, &finding.code),
                crate::batch::BatchProfile::Semantic => (&finding.code, &finding.severity),
            };
            let _ = writeln!(
                text,
                "{}:{}:{}: {} ({})",
                file.source.as_str(),
                finding.start_line + 1,
                finding.start_column + 1,
                label,
                detail
            );
        }
    }
    metadata_human(report, localizer, &mut text);
    text
}

/// Unavailable input remains explicit; captured findings use the same reasons as the IDE.
fn metadata_human(report: &BatchReport, localizer: &Localizer, text: &mut String) {
    use std::fmt::Write;
    for issue in &report.metadata_issues {
        if let Some(findings) = &issue.diagnostics
            && !findings.is_empty()
        {
            for finding in findings {
                let _ = writeln!(
                    text,
                    "{}:{}:{}: {} ({})",
                    issue.source.as_str(),
                    finding.range.start.line + 1,
                    finding.range.start.character + 1,
                    tool_metadata::message(&finding.issue, localizer),
                    tool_metadata::code(&finding.issue),
                );
            }
        } else {
            text.push_str(&localizer.format(
                "tool-check-metadata-incomplete",
                &[
                    ("source", LocalizationValue::Text(issue.source.as_str())),
                    ("code", LocalizationValue::Text(&issue.code)),
                ],
            ));
            text.push('\n');
        }
        for form in &issue.forms {
            text.push_str(
                &localizer.format(
                    "tool-check-metadata-incomplete",
                    &[
                        (
                            "source",
                            LocalizationValue::Text(
                                form.location
                                    .first()
                                    .map_or(form.native_path.as_str(), String::as_str),
                            ),
                        ),
                        ("code", LocalizationValue::Text(&form.code)),
                    ],
                ),
            );
            text.push('\n');
        }
    }
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
        BatchError::MetadataQuery {
            source,
            error: MetadataDiagnosticsError::InconsistentCoordinates(_),
        } => Failure::new("analysis_diagnostic_range_invalid")
            .at(source.as_str())
            .at("metadata"),
        BatchError::MetadataQuery { source, .. } => {
            Failure::new("analysis_internal").at(source.as_str())
        }
        BatchError::Request(_) | BatchError::Query(_) => Failure::new("analysis_internal"),
    }
}
