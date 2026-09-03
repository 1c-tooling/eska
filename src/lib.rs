pub mod cli;
pub mod config;
pub mod creation;
mod designer;
pub mod discovery;
pub mod initialization;
pub mod localization;
pub mod project;
pub mod templates;
mod vcs;

#[cfg(test)]
#[path = "../tests/support/mod.rs"]
mod test_support;
