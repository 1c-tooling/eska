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
pub mod composition;
mod deadline;
mod memory;
mod metadata_diagnostics;
mod metadata_locations;
mod modules;
mod projection;
mod property_edits;
mod queries;
#[cfg(test)]
#[path = "../../../tests/fixtures/eska-format/query_creation.rs"]
mod query_creation_fixture;
mod query_inputs;
mod request;
mod resources;
mod session;
#[cfg(test)]
#[path = "../tests/support/common_module_project.rs"]
mod test_common_module_project;
#[cfg(all(test, any(feature = "analyzer-cli", feature = "check-cli")))]
#[path = "../tests/support/composition_project.rs"]
mod test_composition_project;
#[cfg(test)]
#[path = "../tests/support/query_analysis_project.rs"]
mod test_query_analysis_project;
#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
mod tool_query_consumers;
#[cfg(all(test, any(feature = "analyzer-cli", feature = "check-cli")))]
use test_composition_project as composition_project;
#[cfg(test)]
#[path = "../tests/support/form_project.rs"]
mod test_form_project;
#[cfg(test)]
#[path = "../tests/support/manager_project.rs"]
mod test_manager_project;
#[cfg(test)]
#[path = "../tests/support/predefined_project.rs"]
mod test_predefined_project;
#[cfg(test)]
#[path = "../tests/support/project.rs"]
mod test_project;
#[cfg(test)]
#[path = "../tests/support/property.rs"]
mod test_property;
#[cfg(test)]
#[path = "../tests/support/role_project.rs"]
mod test_role_project;
#[cfg(test)]
#[path = "../../bsl-adapter/tests/support/mod.rs"]
pub(crate) mod test_support;
#[cfg(test)]
use test_predefined_project as predefined_project;
#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
mod tool_metadata;
#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
mod tool_profile;

pub use cancellation::{QueryCancellation, RequestCancellation};
pub use ide::{
    CompletionItem, Diagnostic, DiagnosticCode, DiagnosticsConfig, DocumentSymbol, Locale,
    QueryNativeIssue, QueryOutputField, QueryOutputNameOrigin, QueryOutputType,
    QueryParameterOccurrence, QueryParameterTypes, QuerySelectBranch, QuerySelectStatement,
    QueryTextModel, QueryTextStatement,
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
pub use query_inputs::{
    QueryAnalysisResult, QuerySyntaxError, QuerySyntaxResult, QuerySyntaxScope, QueryTextChange,
    QueryTextDeclaration, QueryTextError, QueryTextInput, QueryTextSnapshot, QueryTextState,
    query_analysis_enabled,
};
pub use request::{SessionRequestError, SessionRevision};
pub use resources::ResourceSummary;
pub use session::composition::{CompositionSummary, ContributionInputs, ContributionSummary};
pub use session::metadata_resources::{
    MetadataResourceInput, MetadataResourceKind, MetadataResourceState,
};
pub use session::query_text::{
    QueryAnalysisDiagnostics, QueryAnalysisRequestError, QueryConsumerAuthoringLocation,
    QueryConsumerCheckCoverage, QueryConsumerContext, QueryConsumerCoverageReport,
    QueryConsumerDiagnosticReport, QueryConsumerDiagnostics, QueryConsumerFinding,
    QueryConsumerIncomplete, QueryConsumerIncompleteReason, QueryConsumerIncompleteReport,
    QueryConsumerLocation, QueryConsumerLocationReport, QueryConsumerReport,
    QueryConsumerReportContext, QueryConsumerReportStatus, QueryConsumerRequestError,
    QueryConsumerTarget, QueryContextRequestError, QueryContextSource, QuerySyntaxDiagnostics,
    QuerySyntaxRequestError, QueryTextSummary,
};
pub use session::{AnalysisScope, AnalysisSession, SessionError, SessionSummary};
