//! Git-aware project operations over the shared Designer XML model.

pub use eska_project_core::project::*;

pub mod clone;
pub mod diff;
pub mod finish;
pub mod history;
pub mod patch;
pub mod save;
pub mod semantic;
pub mod start;
pub mod status;
pub mod switch;
