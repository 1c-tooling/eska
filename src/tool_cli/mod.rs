//! Shared localized presentation for independently compiled tools.

pub mod localization;
pub mod rpc_framing;
pub mod rpc_json;

mod project;
mod support;
pub use project::*;
#[cfg(any(feature = "project-cli", feature = "build-cli"))]
pub(crate) use support::emit_success;
pub use support::{
    Failure, OutputFormat, accepted_ref, bootstrap_value, emit_failure, localize, parse,
};
