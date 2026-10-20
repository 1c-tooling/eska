//! eska's project operations and CLI presentation.
//!
//! Start with [`cli`] for commands, [`project`] for project operations,
//! [`config`] for `eska.toml`, and [`vcs`] for Git and workflow settings.

pub mod cli;
pub(crate) mod component;
pub mod config;
pub mod project;
mod update;
pub mod vcs;
pub use eska_project_core::application;

#[cfg(test)]
#[path = "../tests/support/mod.rs"]
mod test_support;

/// Locate the explicitly built component outside Cargo's unit-test `deps` directory.
#[cfg(test)]
fn test_component(component: eska_tool_runtime::Component) -> std::path::PathBuf {
    let executable = std::env::current_exe().expect("test executable");
    executable
        .parent()
        .and_then(std::path::Path::parent)
        .expect("Cargo target directory")
        .join(format!(
            "{}{}",
            component.name(),
            std::env::consts::EXE_SUFFIX
        ))
}
