//! Shared, revisioned ESKA analysis sessions; no platform, Git or transport runtime is required.

#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
mod analysis_settings;
#[cfg(feature = "analyzer-cli")]
pub mod analyzer_cli;
pub mod batch;
mod binding;
mod cancellation;
#[cfg(feature = "check-cli")]
pub mod check_cli;
mod deadline;
mod memory;
mod metadata_diagnostics;
mod metadata_locations;
mod modules;
mod projection;
mod property_edits;
mod queries;
mod request;
mod resources;
mod session;
#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
mod tool_metadata;

pub use cancellation::{QueryCancellation, RequestCancellation};
pub use ide::{
    CompletionItem, Diagnostic, DiagnosticCode, DiagnosticsConfig, DocumentSymbol, Locale,
};
pub use memory::{KernelMemoryReport, KernelMemoryTable};
pub use metadata_diagnostics::{
    MetadataDiagnostic, MetadataDiagnosticIssue, MetadataDiagnostics, MetadataDiagnosticsError,
};
pub use metadata_locations::{MetadataLocation, MetadataLocationError};
pub use modules::{ModuleFailure, ModuleInput, ModuleState, ModuleSummary, SessionOptions};
pub use projection::{MetadataView, ModelState, OwnerModel};
pub use property_edits::{PropertyEditError, PropertyEditInput, PropertyEditPlan};
pub use queries::definitions::DefinitionTarget;
pub use queries::symbol_hover::{HoverKind, SymbolHover};
pub use queries::{ModuleResult, QueryError};
pub use request::{SessionRequestError, SessionRevision};
pub use resources::ResourceSummary;
pub use session::{AnalysisSession, SessionError, SessionSummary};
