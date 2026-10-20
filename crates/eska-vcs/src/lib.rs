//! Git workflow and semantic Designer XML comparisons shared by standalone and full CLI.

pub mod bridge;
pub mod cli;
pub mod project;
pub mod vcs;

#[cfg(test)]
#[path = "../../../tests/support/mod.rs"]
mod test_support;

pub use cli::run;
