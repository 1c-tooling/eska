//! Run the same clone, diff, history and save regressions against the standalone executable.

#[path = "../../../tests/support/mod.rs"]
mod support;
mod vcs {
    pub mod support {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/vcs/support.rs"
        ));
    }
}
#[path = "../../../tests/cli/clone.rs"]
mod clone;
#[path = "../../../tests/cli/diff.rs"]
mod diff;
#[path = "../../../tests/cli/history.rs"]
mod history;
#[path = "../../../tests/cli/save.rs"]
mod save;

/// Bind shared process regressions to the tool under test, without the umbrella binary.
const fn vcs_cli() -> &'static str {
    env!("CARGO_BIN_EXE_eska-vcs")
}
