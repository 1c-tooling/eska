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
    let findings = finding_count(report).to_string();
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
    if report.analysis_scope == crate::AnalysisScope::LocalSyntax {
        text.push_str(&localizer.text("tool-check-local-syntax-scope"));
        text.push('\n');
    }
    query_human(report, localizer, &mut text);
    let quota_exceeded = report.metadata_issues.iter().any(|issue| {
        issue.code == "analysis_source_budget_exceeded"
            || issue
                .forms
                .iter()
                .any(|form| form.code == "analysis_source_budget_exceeded")
    }) || report.files.iter().any(|file| {
        matches!(&file.state, crate::batch::FileState::Unavailable { code, .. }
                if code == "analysis_source_budget_exceeded")
    }) || report.query_files.iter().any(|file| {
        matches!(&file.state, crate::batch::FileState::Unavailable { code, .. } if code == "analysis_source_budget_exceeded")
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

/// Native findings and current consumer bindings retain their separate coverage and presentation.
fn query_human(report: &BatchReport, localizer: &Localizer, text: &mut String) {
    use std::fmt::Write;
    if report.query_coverage.queries == 0 {
        return;
    }
    let checked = report.query_coverage.checked.to_string();
    let total = report.query_coverage.queries.to_string();
    text.push_str(&localizer.format(
        if report.profile == crate::batch::BatchProfile::Syntax {
            "tool-check-query-result"
        } else {
            "tool-check-query-semantic-result"
        },
        &[
            ("checked", LocalizationValue::Text(&checked)),
            ("total", LocalizationValue::Text(&total)),
        ],
    ));
    text.push('\n');
    for file in &report.query_files {
        consumer_human(file, localizer, text);
        for query in &file.findings {
            let finding = &query.finding;
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
        if let crate::batch::FileState::Unavailable { code, .. } = &file.state {
            text.push_str(&localizer.format(
                "tool-check-query-unavailable",
                &[
                    ("source", LocalizationValue::Text(file.source.as_str())),
                    ("code", LocalizationValue::Text(code)),
                ],
            ));
            text.push('\n');
        }
    }
}

/// Native and consumer findings contribute to the same localized summary.
fn finding_count(report: &BatchReport) -> usize {
    report
        .files
        .iter()
        .map(|file| file.findings.len())
        .sum::<usize>()
        .saturating_add(
            report
                .query_files
                .iter()
                .map(|file| {
                    file.findings
                        .len()
                        .saturating_add(consumer_finding_count(file))
                })
                .sum::<usize>(),
        )
}

/// Count proven bindings without treating an unavailable current resource as a checked empty result.
fn consumer_finding_count(file: &crate::batch::BatchQueryFile) -> usize {
    match &file.consumers {
        Some(crate::QueryConsumerReportStatus::Checked { report }) => report.findings.len(),
        Some(crate::QueryConsumerReportStatus::Unavailable { .. }) | None => 0,
    }
}

/// A missing authoring map has no invented line/column; machine precision stays available separately.
fn consumer_human(file: &crate::batch::BatchQueryFile, localizer: &Localizer, text: &mut String) {
    use std::fmt::Write;
    match &file.consumers {
        Some(crate::QueryConsumerReportStatus::Checked { report }) => {
            let checked = report.coverage.checked_bindings.to_string();
            let incomplete = report.coverage.incomplete.len().to_string();
            text.push_str(&localizer.format(
                "tool-query-consumer-incomplete",
                &[
                    ("checked", LocalizationValue::Text(&checked)),
                    ("incomplete", LocalizationValue::Text(&incomplete)),
                ],
            ));
            text.push('\n');
            for finding in &report.findings {
                let source = finding.location.source.as_str();
                let position = match finding.location.authoring {
                    crate::QueryConsumerAuthoringLocation::Exact { range, .. }
                    | crate::QueryConsumerAuthoringLocation::EnclosingNode { range, .. } => {
                        format!(
                            "{source}:{}:{}",
                            range.start.line + 1,
                            range.start.character + 1
                        )
                    }
                    crate::QueryConsumerAuthoringLocation::Unmapped => source.into(),
                };
                let message = crate::tool_query_consumers::message(finding, localizer);
                let _ = writeln!(text, "{position}: {message} ({})", finding.code);
            }
        }
        Some(crate::QueryConsumerReportStatus::Unavailable { code, .. }) => {
            text.push_str(&localizer.format(
                "tool-query-consumer-unavailable",
                &[("code", LocalizationValue::Text(code))],
            ));
            text.push('\n');
        }
        None => {}
    }
}

/// Setup failures retain stable coordinates without serializing a Debug representation.
pub fn batch_failure(error: BatchError) -> Failure {
    match error {
        BatchError::Request(crate::SessionRequestError::ConfigurationContextRequired)
        | BatchError::Query(crate::QueryError::Request(
            crate::SessionRequestError::ConfigurationContextRequired,
        )) => Failure {
            diagnostics: serde_json::json!({"analysis_scope":"canonical_external","metadata_context_complete":false,"required_context":"owning-configuration"}),
            ..Failure::new("analysis_configuration_context_required")
        },
        BatchError::Open(SessionError::Source(error)) => error.into(),
        BatchError::Open(SessionError::UnsupportedProjectType(project_type)) => Failure {
            diagnostics: serde_json::json!({"project_type":project_type.as_str(),"supported_types":["configuration"]}),
            ..Failure::new("analysis_scope_unsupported").at("project.type")
        },
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
        BatchError::Request(_) | BatchError::Query(_) | BatchError::QuerySyntax(_) => {
            Failure::new("analysis_internal")
        }
    }
}
