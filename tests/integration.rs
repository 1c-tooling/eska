//! End-to-end CLI and filesystem-backed project scenarios.

mod cli;
mod project;
mod support;
mod vcs;

/// Select the full CLI for shared VCS process regressions.
const fn vcs_cli() -> &'static str {
    env!("CARGO_BIN_EXE_eska")
}
