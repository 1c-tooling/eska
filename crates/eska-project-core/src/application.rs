//! Explicit application composition and read-only analysis contexts for Designer XML.

mod context;
mod graph;
mod policy;
mod settings;
mod snapshot;

pub use context::{AnalysisContext, ContextError, ContextSource, SourceRole, resolve};
pub(crate) use policy::RawTools;
pub use policy::{AnalyzerPolicy, CheckPolicy, ToolSettings};
pub use settings::{
    AnalysisOptions, AnalysisSettings, Application, ConfigurationReference, SettingsError,
    TargetPlatformVersion,
};
pub(crate) use settings::{RawAnalysis, RawApplication, RawConfiguration};
pub use snapshot::{ContextSnapshot, resolve_snapshot};

#[cfg(test)]
mod tests;
