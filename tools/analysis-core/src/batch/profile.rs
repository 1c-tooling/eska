//! Profile identity and readiness do not depend on CLI parsing or localized presentation.

use crate::{AnalysisScope, SessionError};
use crate::{DiagnosticCode, DiagnosticsConfig, SessionOptions};
use eska::project_core::{ProjectType, source_format::SourceReader};

/// Syntax is independently streamable; semantic rules query one retained input snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchProfile {
    Syntax,
    Semantic,
}

impl BatchProfile {
    /// New artifact profiles admit local syntax while canonical semantics keeps its existing guard.
    pub(crate) fn session_scope(
        self,
        reader: &SourceReader,
    ) -> Result<AnalysisScope, SessionError> {
        let kind = reader.project_type().map_err(SessionError::Source)?;
        Ok(
            if self == Self::Syntax && kind != ProjectType::Configuration {
                AnalysisScope::LocalSyntax
            } else {
                AnalysisScope::canonical_for(kind)
            },
        )
    }
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
