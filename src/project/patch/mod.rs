//! VCS owns committed planning; this temporary adapter still composes native execution.

mod execute;
mod extension;

pub use eska_vcs::project::patch::{PatchChange, PatchError, PatchModule, PatchPlan, plan};
pub use execute::execute;

const MD: &str = "http://v8.1c.ru/8.3/MDClasses";
