//! Profile identity and readiness do not depend on CLI parsing or localized presentation.

use crate::{DiagnosticCode, DiagnosticsConfig, SessionOptions};

/// Syntax is independently streamable; semantic rules query one retained input snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchProfile {
    Syntax,
    Semantic,
}

impl BatchProfile {
    /// Both consumers give cross-module rules the shared source reserve while syntax remains lazy.
    pub(crate) fn session_options(self, max_retained_source_bytes: u64) -> SessionOptions {
        SessionOptions {
            max_loaded_module_bytes: match self {
                Self::Syntax => SessionOptions::default().max_loaded_module_bytes,
                Self::Semantic => max_retained_source_bytes,
            },
            max_retained_source_bytes,
        }
    }

    /// An entirely disabled profile must not claim a successful empty check.
    pub(crate) fn enabled(self, config: &DiagnosticsConfig) -> bool {
        match self {
            Self::Syntax => !config.is_disabled(DiagnosticCode::ParseError),
            Self::Semantic => ide::all_diagnostic_codes().any(|code| !config.is_disabled(code)),
        }
    }
}
