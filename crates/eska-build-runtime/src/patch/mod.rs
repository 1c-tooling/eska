//! Native patch generation consumes prepared XML; it never opens a Git repository.

mod execute;
mod extension;
pub use eska_project_core::patch::{PatchDescription, PatchError};
pub use execute::execute;
const MD: &str = "http://v8.1c.ru/8.3/MDClasses";
