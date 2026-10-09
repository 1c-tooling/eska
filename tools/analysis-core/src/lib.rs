//! Shared, revisioned ESKA analysis sessions; no platform, Git or transport runtime is required.

pub mod batch;
mod binding;
mod cancellation;
#[cfg(feature = "check-cli")]
pub mod check_cli;
mod modules;
mod projection;
mod queries;
mod request;
mod session;

pub use cancellation::QueryCancellation;
pub use ide::{CompletionItem, Diagnostic, DiagnosticCode, DiagnosticsConfig, Locale};
pub use modules::{ModuleFailure, ModuleInput, ModuleState, ModuleSummary, SessionOptions};
pub use projection::{MetadataView, ModelState, OwnerModel};
pub use queries::{ModuleResult, QueryError};
pub use request::{SessionRequestError, SessionRevision};
pub use session::{AnalysisSession, SessionError, SessionSummary};
