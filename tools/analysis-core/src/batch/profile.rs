//! Profile identity and readiness do not depend on CLI parsing or localized presentation.

use crate::{DiagnosticCode, DiagnosticsConfig};

/// Syntax is independently streamable; semantic rules query one retained input snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchProfile {
    Syntax,
    Semantic,
}

impl BatchProfile {
    /// An entirely disabled profile must not claim a successful empty check.
    pub(super) fn enabled(self, config: &DiagnosticsConfig) -> bool {
        match self {
            Self::Syntax => !config.is_disabled(DiagnosticCode::ParseError),
            Self::Semantic => ide::all_diagnostic_codes().any(|code| !config.is_disabled(code)),
        }
    }
}
