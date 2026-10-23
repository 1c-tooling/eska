//! Explicit query DTO inputs remain independent of Rust domain serialization.

use eska_project_core::project::query::{CommentPlacement, Position, QueryIdentity};
use serde::Deserialize;
use serde_json::Value;

use crate::ide::{envelope::error, params};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Key {
    pub session: String,
    pub project: String,
    pub generation: String,
}

impl Key {
    /// Bind retained state to the exact metadata context that admitted the upload.
    pub fn decode(value: &Value) -> Result<Self, Value> {
        let context: params::Context = params::decode(value)?;
        let generation = params::counter(&context.generation)?.to_string();
        Ok(Self {
            session: context.session_id,
            project: context.project_id,
            generation,
        })
    }

    /// Cleanup remains possible after a generation change while staying inside the same session/member.
    pub fn same_owner(&self, other: &Self) -> bool {
        self.session == other.session && self.project == other.project
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BufferBegin {
    pub bytes: usize,
    pub snapshot: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BufferAppend {
    pub buffer_id: String,
    pub offset: usize,
    pub text: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BufferId {
    pub buffer_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Open {
    pub node: Value,
    pub source: params::Path,
    pub version: String,
    pub application: Option<String>,
    pub buffer_id: Option<String>,
    pub text: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DocumentId {
    pub document_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Revision {
    pub document_id: String,
    pub version: String,
    pub snapshot: String,
    pub context_fingerprint: String,
}

#[derive(Deserialize)]
pub(super) struct List {
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Selection {
    pub query: Option<Identity>,
    pub at: Option<Point>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Plan {
    pub query: Identity,
    pub text: Option<String>,
    pub buffer_id: Option<String>,
    pub comments: Vec<Placement>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Placement {
    pub comment: usize,
    pub query_offset: Value,
}

impl Placement {
    /// JSON null is explicit deletion, rather than an omitted implicit comment decision.
    pub fn core(&self) -> Result<CommentPlacement, Value> {
        let query_offset = if self.query_offset.is_null() {
            None
        } else {
            Some(
                self.query_offset
                    .as_u64()
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(|| error(-32602))?,
            )
        };
        Ok(CommentPlacement {
            comment: self.comment,
            query_offset,
        })
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(super) enum Identity {
    DataSet { path: Vec<String> },
    DynamicList { name: String, id: String },
    BslLiteral { range: ByteRange },
}

impl Identity {
    /// Byte coordinates remain version-scoped and are verified again by the core reader.
    pub fn core(&self) -> Result<QueryIdentity, Value> {
        Ok(match self {
            Self::DataSet { path }
                if !path.is_empty() && path.iter().all(|part| !part.is_empty()) =>
            {
                QueryIdentity::DataSet(path.clone())
            }
            Self::DynamicList { name, id } if !name.is_empty() && !id.is_empty() => {
                QueryIdentity::DynamicList {
                    name: name.clone(),
                    id: id.clone(),
                }
            }
            Self::BslLiteral { range } if range.start < range.end => {
                QueryIdentity::BslLiteral(range.start..range.end)
            }
            _ => return Err(error(-32602)),
        })
    }
}

#[derive(Deserialize)]
pub(super) struct ByteRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Copy, Deserialize)]
pub(super) struct Point {
    pub line: usize,
    pub character: usize,
}

impl Point {
    /// Editor positions use UTF-16 code units, independently of the byte map representation.
    pub const fn core(self) -> Position {
        Position {
            line: self.line,
            character: self.character,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum Direction {
    QueryToSource,
    SourceToQuery,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum Affinity {
    Before,
    After,
}

#[derive(Deserialize)]
pub(super) struct Map {
    pub query: Identity,
    pub direction: Direction,
    pub positions: Vec<Point>,
    pub affinity: Option<Affinity>,
}
