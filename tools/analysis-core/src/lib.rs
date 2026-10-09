//! Shared, revisioned ESKA analysis sessions; no platform, Git or transport runtime is required.

mod binding;
mod projection;
mod session;

pub use projection::{ModelState, OwnerModel};
pub use session::{AnalysisSession, SessionError, SessionSummary};
