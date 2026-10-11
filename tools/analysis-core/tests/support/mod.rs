//! Each process test owns its project through the standard playground directory guard.

use std::path::PathBuf;

#[path = "../../../bsl-adapter/tests/support/mod.rs"]
mod directory;
pub use directory::TestDirectory;
pub mod form_project;
#[cfg(feature = "check-cli")]
pub mod metadata;
pub mod predefined_project;
mod project;
pub mod role_project;
pub use project::inline_command_project;

/// Populate a new, test-owned minimal project without a binary accepted base.
pub fn project(body: &str, binary: bool) -> (TestDirectory, PathBuf) {
    let directory = TestDirectory::new();
    let root = project::project(&directory.0, body, binary);
    (directory, root)
}
