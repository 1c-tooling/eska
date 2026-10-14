//! eska's project operations and CLI presentation.
//!
//! [`project_core`] contains shared source operations without application dependencies.
//! The `build` feature adds `build_core` without the application or Git engine.
//! The default `legacy` feature retains the existing application and CLI modules.

pub mod analysis_core;
#[cfg(feature = "build-cli")]
pub mod build_cli;
#[cfg(feature = "build")]
pub mod build_core;
#[cfg(feature = "legacy")]
pub mod cli;
#[cfg(feature = "legacy")]
pub mod config;
#[cfg(feature = "legacy")]
pub mod project;
#[cfg(feature = "project-cli")]
pub mod project_cli;
pub mod project_core;
#[cfg(feature = "tool-cli")]
pub mod tool_cli;
#[cfg(feature = "legacy")]
mod update;
#[cfg(feature = "legacy")]
pub mod vcs;

#[cfg(test)]
#[path = "../tests/support/mod.rs"]
mod test_support;

#[cfg(test)]
#[path = "../tests/fixtures/eska-format/query_creation.rs"]
mod query_creation_fixture;
