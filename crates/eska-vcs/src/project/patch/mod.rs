//! Immutable Git patch planning; native generation remains with the platform owner.

mod descriptor;
mod methods;
mod model;
mod plan;

pub use model::{PatchChange, PatchError, PatchModule, PatchPlan};
pub use plan::plan;

const MD: &str = "http://v8.1c.ru/8.3/MDClasses";
