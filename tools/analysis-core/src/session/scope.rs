//! Admission separates local source syntax from a composed canonical metadata context.

use serde::Serialize;

/// Syntax admission never grants permission to resolve an uncomposed project's metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisScope {
    CanonicalConfiguration,
    LocalSyntax,
}

impl AnalysisScope {
    /// Machine-facing scope names are independent of the selected presentation locale.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CanonicalConfiguration => "canonical_configuration",
            Self::LocalSyntax => "local_syntax",
        }
    }
}
