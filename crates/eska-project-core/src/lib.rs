//! Shared Designer XML project model, discovery and checked metadata operations.
//!
//! This crate contains no Git repository, platform runner or language-server runtime.

pub mod doctor;
pub mod formatting;

pub mod application;
pub mod artifact;
pub mod config;
pub mod patch;
pub mod project;
pub mod provenance;
pub mod workflow;

#[cfg(test)]
#[path = "../../../tests/support/mod.rs"]
mod test_support;
