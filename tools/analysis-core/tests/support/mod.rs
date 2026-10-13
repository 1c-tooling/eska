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
pub mod property;
pub mod role_project;
pub use project::inline_command_project;

/// Populate a new, test-owned minimal project without a binary accepted base.
pub fn project(body: &str, binary: bool) -> (TestDirectory, PathBuf) {
    let directory = TestDirectory::new();
    let root = project::project(&directory.0, body, binary);
    (directory, root)
}

/// Select a real extension root while deliberately providing no admitted base-analysis context.
pub fn extension_project(body: &str) -> (TestDirectory, PathBuf) {
    let (directory, root) = project(body, false);
    let native = directory.0.join("native");
    let path = native.join("Configuration.xml");
    let contents = std::fs::read_to_string(&path).unwrap().replace(
        "<Properties>",
        "<Properties><ConfigurationExtensionPurpose>Patch</ConfigurationExtensionPurpose>",
    );
    std::fs::write(path, contents).unwrap();
    eska::project_core::source_format::import_designer(&native, &root.join("extension-src"))
        .unwrap();
    std::fs::write(root.join("eska.toml"), "[project]\ntype='extension'\nsource='extension-src'\nsource_format='eska'\n[eska]\naccepted_base='.eska/accepted'\n").unwrap();
    (directory, root)
}
