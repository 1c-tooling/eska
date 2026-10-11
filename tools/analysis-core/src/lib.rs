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
#[cfg(test)]
#[path = "../tests/support/form_project.rs"]
mod test_form_project;
#[cfg(test)]
#[path = "../tests/support/project.rs"]
mod test_project;
#[cfg(test)]
#[path = "../tests/support/role_project.rs"]
mod test_role_project;
#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
mod tool_metadata;
#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
mod tool_profile;

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
pub use projection::{FormModel, MetadataView, ModelState, OwnerModel};
pub use property_edits::{PropertyEditError, PropertyEditInput, PropertyEditPlan};
pub use queries::definitions::DefinitionTarget;
pub use queries::local_rename::{
    LocalRenameError, LocalRenameInput, LocalRenamePlan, LocalRenameTarget,
};
pub use queries::references::{ReferenceKind, ReferenceOccurrence, ReferenceSearch};
pub use queries::signature_help::{CallParameter, CallSignature, CallSignatures};
pub use queries::symbol_hover::{HoverKind, SymbolHover};
pub use queries::{ModuleResult, QueryError};
pub use request::{SessionRequestError, SessionRevision};
pub use resources::ResourceSummary;
pub use session::metadata_resources::{
    MetadataResourceInput, MetadataResourceKind, MetadataResourceState,
};
pub use session::{AnalysisSession, SessionError, SessionSummary};
