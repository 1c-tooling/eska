//! Explicit application composition and read-only analysis contexts for Designer XML.

mod context;
mod graph;
mod settings;

pub use context::{AnalysisContext, ContextError, ContextSource, SourceRole, resolve};
pub use settings::{
    AnalysisOptions, AnalysisSettings, Application, SettingsError, TargetPlatformVersion,
};
pub(crate) use settings::{RawAnalysis, RawApplication};

#[cfg(test)]
mod tests;
