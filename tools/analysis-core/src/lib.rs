//! Shared, revisioned ESKA analysis sessions; no platform, Git or transport runtime is required.

pub mod batch;
mod binding;
mod cancellation;
#[cfg(feature = "check-cli")]
pub mod check_cli;
mod memory;
mod modules;
mod projection;
mod queries;
mod request;
mod resources;
mod session;

pub use cancellation::QueryCancellation;
pub use ide::{CompletionItem, Diagnostic, DiagnosticCode, DiagnosticsConfig, Locale};
pub use memory::{KernelMemoryReport, KernelMemoryTable};
pub use modules::{ModuleFailure, ModuleInput, ModuleState, ModuleSummary, SessionOptions};
pub use projection::{MetadataView, ModelState, OwnerModel};
pub use queries::{ModuleResult, QueryError};
pub use request::{SessionRequestError, SessionRevision};
pub use resources::ResourceSummary;
pub use session::{AnalysisSession, SessionError, SessionSummary};
