//! eska's project operations and CLI presentation.
//!
//! [`project_core`] contains shared source operations without application dependencies.
//! The `build` feature adds `build_core` without the application or Git engine.
//! The default `legacy` feature retains the existing application and CLI modules.

#[cfg(feature = "build")]
pub mod build_core;
#[cfg(feature = "legacy")]
pub mod cli;
#[cfg(feature = "legacy")]
pub mod config;
#[cfg(feature = "legacy")]
pub mod project;
pub mod project_core;
#[cfg(feature = "legacy")]
mod update;
#[cfg(feature = "legacy")]
pub mod vcs;

#[cfg(test)]
#[path = "../tests/support/mod.rs"]
mod test_support;
