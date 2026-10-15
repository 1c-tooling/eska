//! Shared localized presentation for independently compiled tools.

pub mod localization;
pub mod rpc_framing;
pub mod rpc_json;

mod project;
mod support;
pub use project::*;
pub use support::{
    Failure, OutputFormat, accepted_ref, bootstrap_value, emit_failure, emit_success, localize,
    parse,
};

/// Independently built tools expose the version of their linked source/project core.
pub const PACKAGE_VERSION: &str = env!("CARGO_PKG_VERSION");
