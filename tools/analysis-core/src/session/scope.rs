//! Admission separates local source syntax from a composed canonical metadata context.

use serde::Serialize;

/// Syntax admission never grants permission to resolve an uncomposed project's metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisScope {
    CanonicalConfiguration,
    CanonicalExternal,
    LocalSyntax,
}

impl AnalysisScope {
    /// Machine-facing scope names are independent of the selected presentation locale.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CanonicalConfiguration => "canonical_configuration",
            Self::CanonicalExternal => "canonical_external",
            Self::LocalSyntax => "local_syntax",
        }
    }

    /// Typed own metadata is available without implying an owning configuration or global context.
    #[must_use]
    pub const fn has_semantic_context(self) -> bool {
        !matches!(self, Self::LocalSyntax)
    }

    /// Only a composed configuration scope can admit whole-project semantic rules.
    #[must_use]
    pub const fn has_configuration_context(self) -> bool {
        matches!(self, Self::CanonicalConfiguration)
    }

    /// The validated artifact profile supplies the discriminator; unsupported composition still fails admission.
    pub(crate) const fn canonical_for(kind: eska::project_core::ProjectType) -> Self {
        match kind {
            eska::project_core::ProjectType::Processing
            | eska::project_core::ProjectType::Report => Self::CanonicalExternal,
            _ => Self::CanonicalConfiguration,
        }
    }
}
