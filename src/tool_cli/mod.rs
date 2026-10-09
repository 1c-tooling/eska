//! Shared localized presentation for independently compiled tools.

pub mod localization;

#[cfg(feature = "project-cli")]
mod support;
#[cfg(feature = "project-cli")]
pub(crate) use support::*;
