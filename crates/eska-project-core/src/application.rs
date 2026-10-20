//! Explicit application composition and read-only analysis contexts for Designer XML.

mod context;
mod graph;
mod policy;
mod settings;

pub use context::{AnalysisContext, ContextError, ContextSource, SourceRole, resolve};
pub(crate) use policy::RawTools;
pub use policy::{AnalyzerPolicy, CheckPolicy, ToolSettings};
pub use settings::{
    AnalysisOptions, AnalysisSettings, Application, SettingsError, TargetPlatformVersion,
};
pub(crate) use settings::{RawAnalysis, RawApplication};

#[cfg(test)]
mod tests;
