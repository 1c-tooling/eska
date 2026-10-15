//! Read-only admission data needs no project, filesystem input or analysis host.

use std::process::ExitCode;

use eska::tool_cli::{self, Failure, OutputFormat, localization::Localizer};
use serde_json::json;

/// Advertise supported entry points without declaring their metadata coverage complete.
pub(super) fn emit(format: OutputFormat, localizer: &Localizer) -> Result<ExitCode, Failure> {
    tool_cli::emit_success(
        "eska-check",
        "capabilities",
        format,
        &json!({
            "version": env!("CARGO_PKG_VERSION"),
            "source_core_version": tool_cli::PACKAGE_VERSION,
            "protocol_version": 1,
            "commands": ["check"],
            "profiles": ["syntax", "semantic"],
            "source_formats": ["eska"],
            "report_schema_version": 1
        }),
        &localizer.text("tool-check-capabilities-summary"),
    )
}
