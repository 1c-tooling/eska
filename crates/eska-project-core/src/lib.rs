//! Shared Designer XML project model, discovery and checked metadata operations.
//!
//! This crate contains no Git repository, platform runner or language-server runtime.

pub mod application;
pub mod config;
pub mod project;
pub mod provenance;
pub mod workflow;

#[cfg(test)]
#[path = "../../../tests/support/mod.rs"]
mod test_support;
