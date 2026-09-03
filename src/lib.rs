//! eska's project operations and CLI presentation.
//!
//! Start with [`cli`] for commands, [`project`] for project operations,
//! [`config`] for `eska.toml`, and [`vcs`] for Git and workflow settings.

pub mod cli;
pub mod config;
mod designer_xml;
pub mod project;
pub mod vcs;

#[cfg(test)]
#[path = "../tests/support/mod.rs"]
mod test_support;
