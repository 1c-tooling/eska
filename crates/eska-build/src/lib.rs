//! Standalone platform CLI and shared presentation; actual native execution lives in one runtime.

pub mod cli;
pub(crate) mod config {
    pub use eska_build_runtime::config::{
        EditOutcome as GlobalConfigEditOutcome, GlobalConfigError,
        InitOutcome as GlobalConfigInitOutcome, RunnerKind, config_path, edit_at as edit_global_at,
        init_at as init_global_at, load as load_global,
    };
    #[cfg(test)]
    pub use eska_project_core::config::{InvalidSourceReason, ProjectConfigError};
}
pub(crate) mod project {
    pub use eska_build_runtime::{artifact, build};
    pub use eska_project_core::project::*;
}

pub use cli::run;
