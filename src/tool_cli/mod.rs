//! Shared localized presentation for independently compiled tools.

pub mod localization;

mod project;
mod support;
pub use project::*;
pub use support::{Failure, OutputFormat, bootstrap_value, emit_failure, localize, parse};
#[cfg(any(feature = "project-cli", feature = "build-cli"))]
pub(crate) use support::{accepted_ref, emit_success};
