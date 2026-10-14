//! End-to-end CLI and filesystem-backed project scenarios.

mod cli;
mod project;
mod support;
mod vcs;

#[cfg(feature = "build-cli")]
#[path = "fixtures/eska-format/query_creation.rs"]
mod query_creation;
