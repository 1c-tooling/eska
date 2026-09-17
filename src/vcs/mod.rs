//! Git integration, isolated command execution and declarative workflow policies.

pub mod command;
pub mod diff;
pub(crate) mod git;
pub mod network;
pub mod repository;
pub mod shelves;
pub(crate) mod snapshot;
pub mod status;
pub mod workflow;
