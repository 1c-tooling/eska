//! Patch-extension planning and generation boundaries.

mod descriptor;
mod execute;
mod extension;
mod methods;
mod model;
mod plan;

pub use execute::execute;
pub use model::{PatchChange, PatchError, PatchModule, PatchPlan};
pub use plan::plan;

const MD: &str = "http://v8.1c.ru/8.3/MDClasses";
