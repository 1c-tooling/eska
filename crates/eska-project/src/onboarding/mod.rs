//! Compatibility onboarding retains its interactive contract while invoking native and Git owners.

mod artifact;
pub mod init;
pub mod new;
pub mod project {
    pub use eska_project_core::project::*;
    pub mod create;
    pub mod init;
    pub mod artifact {
        pub use super::super::artifact::PreparedSources;
    }
}
mod config {
    pub use eska_project_core::config::*;
}
mod vcs {
    pub use eska_project_core::workflow;
}
mod cli {
    pub use eska_presentation::{diagnostics, localization, path_output};
    pub use eska_terminal::interactive;
}
