//! Git integration, isolated command execution and declarative workflow policies.

pub mod command;
pub mod diff;
pub(crate) mod git;
pub mod repository;
pub mod status;
pub mod workflow;
