//! Embedded Git and isolated capability fallbacks; no native platform or analyzer.

pub mod command;
pub mod diff;
pub mod git;
pub mod network;
pub mod repository;
pub mod shelves;
pub(crate) mod snapshot;
pub mod status;
pub mod workflow;
