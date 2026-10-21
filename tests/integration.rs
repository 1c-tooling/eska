//! End-to-end CLI and filesystem-backed project scenarios.

#[path = "support/backend.rs"]
mod backend;
#[path = "support/dependencies.rs"]
mod dependencies;

mod cli;
mod project;
mod support;
mod vcs;

/// Select the full CLI for shared VCS process regressions.
const fn vcs_cli() -> &'static str {
    env!("CARGO_BIN_EXE_eska")
}
