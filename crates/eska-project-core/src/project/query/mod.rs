//! Query views retain exact owner coordinates; they never become another source file.

mod bsl;
mod editing;
mod mapping;
pub mod recovery;
mod xml;

pub use bsl::read_bsl;
pub use editing::{CommentPlacement, LineEnding, plan};
pub use mapping::{Affinity, Mapping, Position, SourceMap, byte_position, position_byte};
pub use xml::read_xml;
pub(crate) use xml::{XmlKind, read_with_kind as read_xml_with_kind};

use std::ops::Range;

/// Bound an unsaved owner independently of filesystem and transport limits.
pub const MAX_SNAPSHOT_BYTES: usize = 8 * 1024 * 1024;

/// Explicit placement decisions must fit the existing bounded RPC frame alongside a query value.
pub const MAX_SOURCE_COMMENTS: usize = 8192;

/// An editor model may deliberately omit the file's UTF-8 encoding signature.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum QueryOwnerFormat {
    Editor,
    Source,
}

/// Identity is meaningful only in its checked owner snapshot, never across arbitrary edits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueryIdentity {
    DataSet(Vec<String>),
    DynamicList { name: String, id: String },
    BslLiteral(Range<usize>),
}

/// Source comments are not part of the decoded SDBL value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceComment {
    pub source: Range<usize>,
    pub query_offset: usize,
    pub text: String,
}

/// All coordinates address UTF-8 bytes in the exact source and decoded value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Query {
    pub identity: QueryIdentity,
    pub text: String,
    pub source: Range<usize>,
    pub map: SourceMap,
    pub comments: Vec<SourceComment>,
}

/// Unsupported input is explicit and never authorizes a partial source replacement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryError {
    TooLarge,
    InvalidXml,
    UnsupportedStructure,
    InvalidLiteral,
    UnsupportedExpression,
    NotFound,
    InvalidPosition,
    InvalidValue,
    Conflict,
    CommentsRequireDecision,
}

/// Reject oversized buffers before parsing or allocating coordinate maps.
const fn check_size(input: &str) -> Result<(), QueryError> {
    if input.len() > MAX_SNAPSHOT_BYTES {
        return Err(QueryError::TooLarge);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oversized input fails before either language parser allocates token or position maps.
    #[test]
    fn bounds_unsaved_owner_size() {
        let input = " ".repeat(MAX_SNAPSHOT_BYTES + 1);
        assert_eq!(read_xml(&input), Err(QueryError::TooLarge));
        assert_eq!(read_bsl(&input, 0), Err(QueryError::TooLarge));
    }

    /// A small but escape-dense query cannot produce an unbounded coordinate response.
    #[test]
    fn bounds_mapping_expansion_independently_of_source_size() {
        let input = format!("x = \"{}\";", "\"\"".repeat(65_537));
        assert_eq!(read_bsl(&input, 5), Err(QueryError::TooLarge));
    }
}
