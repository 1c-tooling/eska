//! IDE and CI presentation share machine causes and localized reasons without coupling the core to JSON.

mod adapter_issue;
mod presentation;

use serde_json::{Value, json};

use crate::{MetadataDiagnostic, MetadataDiagnosticIssue};

pub use adapter_issue::value as adapter_cause;
pub use presentation::message;

/// Human and machine findings retain the same stable diagnostic identifier.
pub const fn code(issue: &MetadataDiagnosticIssue) -> &'static str {
    match issue {
        MetadataDiagnosticIssue::Adapter(_) => "analysis_adapter_invalid",
        MetadataDiagnosticIssue::Parse(_) => "analysis_metadata_parse_issue",
        MetadataDiagnosticIssue::UnsupportedKind(_) => "analysis_metadata_kind_unsupported",
    }
}

/// Both tools expose the captured YAML range and native identity with the same cause and severity.
pub fn finding(finding: &MetadataDiagnostic) -> Value {
    let (severity, issue) = match &finding.issue {
        MetadataDiagnosticIssue::Adapter(issue) => {
            (presentation::adapter_severity(issue), adapter_cause(issue))
        }
        MetadataDiagnosticIssue::Parse(issue) => (
            presentation::parse_severity(issue),
            json!({"kind":"parse_issue","cause":issue}),
        ),
        MetadataDiagnosticIssue::UnsupportedKind(kind) => (
            2,
            json!({"kind":"unsupported_kind","metadata_kind":kind.as_str()}),
        ),
    };
    json!({"range":finding.range,"severity":severity,"code":code(&finding.issue),
        "data":{"start_byte":finding.byte_range.start,"end_byte":finding.byte_range.end,
            "native_path":finding.native_path,"issue":issue}})
}
