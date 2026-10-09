//! Shared localized presentation for independently compiled tools.

pub mod localization;

#[cfg(any(feature = "project-cli", feature = "build-cli"))]
mod project;
#[cfg(any(feature = "project-cli", feature = "build-cli"))]
mod support;
#[cfg(any(feature = "project-cli", feature = "build-cli"))]
pub(crate) use project::*;
#[cfg(any(feature = "project-cli", feature = "build-cli"))]
pub(crate) use support::*;
