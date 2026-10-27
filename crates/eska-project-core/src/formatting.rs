//! IDE-independent formatting policy, source snapshots and checked text contracts.

pub mod context;
pub mod files;
pub mod migration;
mod options;
mod policy;
pub mod protocol;

pub use options::{Comments, IndentStyle, KeywordCase, Options, Patch, Wrapping};
pub use policy::{Config, Effective, Language, Layer, Override, Profile, matches, resolve};

use serde::{Deserialize, Serialize};

/// Stable failures cross process boundaries without localized implementation details.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Error {
    pub code: String,
    pub field: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
}

impl Error {
    /// Keep syntax, policy and transport errors distinguishable in every frontend.
    #[must_use]
    pub fn new(code: &str, field: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            field: field.into(),
            offset: None,
        }
    }

    /// Attach a UTF-8 source position when a concrete syntax unit is available.
    #[must_use]
    pub const fn at(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }
}

impl std::fmt::Display for Error {
    /// Domain diagnostics expose machine identifiers; presentation supplies human text.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.field)
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests;
