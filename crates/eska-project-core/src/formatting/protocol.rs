//! Version-one pure-text component protocol uses UTF-8 byte offsets and explicit styles.

use super::{Error, Language, Options};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_TEXT_BYTES: usize = 4 * 1024 * 1024;

/// Bind a plan to exact UTF-8 source bytes, including BOM and line endings.
#[must_use]
pub fn source_sha256(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(text.as_bytes()) {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 15)]));
    }
    output
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Range {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema_version: u32,
    pub language: Language,
    pub text: String,
    pub options: Options,
    pub range: Option<Range>,
}

impl Request {
    /// Reject mismatched schemas and byte ranges before any syntax or source mutation.
    ///
    /// # Errors
    /// Returns a structured version, size, option or UTF-8 boundary failure.
    pub fn validate(&self) -> Result<(), Error> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(Error::new("incompatible_protocol", "schema_version"));
        }
        if self.text.len() > MAX_TEXT_BYTES {
            return Err(Error::new("text_too_large", "text"));
        }
        if let Some(range) = self.range
            && (range.start > range.end
                || !self.text.is_char_boundary(range.start)
                || !self.text.is_char_boundary(range.end))
        {
            return Err(Error::new("invalid_range", "range"));
        }
        self.options.validate()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Edit {
    pub range: Range,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub schema_version: u32,
    pub source_sha256: String,
    pub edits: Vec<Edit>,
    pub formatted_range: Range,
}

impl Response {
    /// Validate component output before an adapter converts offsets or applies a native edit.
    ///
    /// # Errors
    /// Rejects wrong snapshots, future schemas, unsafe/overlapping ranges and oversized output.
    pub fn validate(&self, request: &Request) -> Result<(), Error> {
        request.validate()?;
        if self.schema_version != SCHEMA_VERSION
            || self.source_sha256 != source_sha256(&request.text)
        {
            return Err(Error::new("invalid_response", "snapshot"));
        }
        let actual = self.formatted_range;
        if actual.start > actual.end
            || !request.text.is_char_boundary(actual.start)
            || !request.text.is_char_boundary(actual.end)
            || self.edits.len() > 1024
        {
            return Err(Error::new("invalid_response", "formatted_range"));
        }
        let mut previous = None;
        let mut length = request.text.len();
        for edit in &self.edits {
            if edit.range.start < actual.start
                || edit.range.end > actual.end
                || edit.range.start > edit.range.end
                || !request.text.is_char_boundary(edit.range.start)
                || !request.text.is_char_boundary(edit.range.end)
                || previous.is_some_and(|range: Range| {
                    range.end > edit.range.start || range.start == edit.range.start
                })
            {
                return Err(Error::new("invalid_response", "edits"));
            }
            length = length
                .checked_sub(edit.range.end - edit.range.start)
                .and_then(|length| length.checked_add(edit.text.len()))
                .filter(|length| *length <= 16 * 1024 * 1024)
                .ok_or_else(|| Error::new("invalid_response", "output_size"))?;
            previous = Some(edit.range);
        }
        Ok(())
    }
}
