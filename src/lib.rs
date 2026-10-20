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
