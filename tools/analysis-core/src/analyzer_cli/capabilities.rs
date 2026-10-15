//! Bundle compatibility is available before any project exists or stdio server starts.

use std::process::ExitCode;

use eska::tool_cli::{self, Failure, OutputFormat, localization::Localizer};
use serde_json::json;

use super::protocol;

/// Availability and protocol agreement do not grant project readiness or semantic completeness.
pub(super) fn emit(format: OutputFormat, localizer: &Localizer) -> Result<ExitCode, Failure> {
    tool_cli::emit_success(
        "eska-analyzer",
        "capabilities",
        format,
        &json!({
            "version": env!("CARGO_PKG_VERSION"),
            "source_core_version": tool_cli::PACKAGE_VERSION,
            "protocol_version": protocol::VERSION,
            "commands": ["serve"],
            "profiles": ["syntax", "semantic"],
            "source_formats": ["eska"]
        }),
        &localizer.text("tool-analyzer-capabilities-summary"),
    )
}
