//! Test-only aliases bind the original regression suite directly to component implementations.

pub use eska_project_core::config;
pub use eska_vcs::vcs;

pub mod project {
    pub use eska_project::onboarding::project::{create, init};
    pub use eska_project_core::project::*;
    pub use eska_vcs::project::{diff, finish, history, patch, save, semantic, start, switch};
}
