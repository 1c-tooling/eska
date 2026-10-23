//! Unsaved query owners use the same checked Designer paths and current support rules as other edits.

use std::{
    io::{self, Read},
    path::{Path, PathBuf},
};

use super::{ProjectSession, WorkspaceError};
use crate::project::{
    designer_source::SourceRole,
    metadata_edit::{EditPlan, snapshot},
    metadata_model::{MetadataKind, NodeId, ProjectScope},
    query::{self, CommentPlacement, Query, QueryError, QueryIdentity},
};

/// The source resolver decides the language; a filename extension never grants ownership.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryDocumentKind {
    DataComposition,
    Form,
    Bsl,
}

/// Exact editor state supplied again immediately before planning or consuming a source edit.
#[derive(Clone, Copy, Debug)]
pub struct QueryRevision<'a> {
    pub version: u64,
    pub snapshot: &'a str,
}

/// A document binds unsaved text to an existing logical owner and the disk bytes it overlaid.
#[derive(Debug)]
pub struct QueryDocument {
    root: PathBuf,
    source_root: PathBuf,
    scope: ProjectScope,
    generation: u64,
    node: NodeId,
    path: PathBuf,
    kind: QueryDocumentKind,
    version: u64,
    input: String,
    snapshot: String,
    disk_snapshot: String,
    owner_uuid: String,
    writable: bool,
}

/// Reading a forbidden owner is distinct from authorizing an edit of that owner.
#[derive(Debug)]
pub enum QueryDocumentError {
    Workspace(Box<WorkspaceError>),
    Query(QueryError),
    ReadOnly,
    OwnerChanged,
    StaleVersion,
    Io { path: PathBuf, source: io::Error },
}

impl From<WorkspaceError> for QueryDocumentError {
    /// Preserve structured project/source failures without embedding localized output.
    fn from(error: WorkspaceError) -> Self {
        Self::Workspace(Box::new(error))
    }
}

impl From<QueryError> for QueryDocumentError {
    /// Codec failures never imply that a filesystem/editor write occurred.
    fn from(error: QueryError) -> Self {
        Self::Query(error)
    }
}

impl QueryDocument {
    /// The full unsaved owner is retained for conflict checks and source projection.
    #[must_use]
    pub fn input(&self) -> &str {
        &self.input
    }

    /// The snapshot includes every source byte, not just the extracted query.
    #[must_use]
    pub fn snapshot(&self) -> &str {
        &self.snapshot
    }

    /// Detect external disk writes independently of dirty editor text and editor versions.
    #[must_use]
    pub fn disk_snapshot(&self) -> &str {
        &self.disk_snapshot
    }

    /// This version belongs to the editor buffer, independently of metadata generations.
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Return the resolver-checked path relative to the selected member's source directory.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Report the source language without probing or parsing another file.
    #[must_use]
    pub const fn kind(&self) -> QueryDocumentKind {
        self.kind
    }

    /// Initial availability is informational; planning always refreshes the policy.
    #[must_use]
    pub const fn writable(&self) -> bool {
        self.writable
    }

    /// Discover only persisted query scalars in this exact XML buffer.
    ///
    /// # Errors
    /// Rejects a BSL owner or unsupported/ambiguous XML.
    pub fn xml_queries(&self) -> Result<Vec<Query>, QueryError> {
        if self.kind == QueryDocumentKind::Bsl {
            return Err(QueryError::UnsupportedStructure);
        }
        let (kind, queries) = query::read_xml_with_kind(&self.input)?;
        if !matches!(
            (self.kind, kind),
            (
                QueryDocumentKind::DataComposition,
                query::XmlKind::DataComposition
            ) | (QueryDocumentKind::Form, query::XmlKind::Form)
        ) || queries.iter().any(|query| !self.accepts(&query.identity))
        {
            return Err(QueryError::UnsupportedStructure);
        }
        Ok(queries)
    }

    /// Read a BSL query from the original buffer version, not from an independent disk copy.
    ///
    /// # Errors
    /// Rejects an XML owner or an unsupported literal/expression.
    pub fn bsl_query(&self, offset: usize) -> Result<Query, QueryError> {
        if self.kind != QueryDocumentKind::Bsl {
            return Err(QueryError::UnsupportedStructure);
        }
        query::read_bsl(&self.input, offset)
    }

    /// A dirty form payload cannot be reinterpreted as an SCD template or another source kind.
    const fn accepts(&self, identity: &QueryIdentity) -> bool {
        matches!(
            (self.kind, identity),
            (QueryDocumentKind::Bsl, QueryIdentity::BslLiteral(_))
                | (
                    QueryDocumentKind::DataComposition,
                    QueryIdentity::DataSet(_)
                )
                | (QueryDocumentKind::Form, QueryIdentity::DynamicList { .. })
        )
    }
}

impl ProjectSession {
    /// Bind an editor snapshot only to a declared source of an already exposed logical node.
    ///
    /// # Errors
    /// Rejects unknown owners, unresolvable paths, oversized buffers and invalid document versions.
    pub fn query_document(
        &mut self,
        node: &NodeId,
        path: &Path,
        version: u64,
        buffer: Option<&str>,
    ) -> Result<QueryDocument, QueryDocumentError> {
        if version == 0 {
            return Err(QueryDocumentError::StaleVersion);
        }
        if buffer.is_some_and(|input| input.len() > query::MAX_SNAPSHOT_BYTES) {
            return Err(QueryError::TooLarge.into());
        }
        let kind = self.query_location(node, path)?;
        let (owner_uuid, writable) = self.query_access(node, path)?;
        let owner_uuid = owner_uuid.ok_or(QueryDocumentError::OwnerChanged)?;
        let disk = self.query_disk(path)?;
        let input = buffer.unwrap_or(&disk).to_owned();
        Ok(QueryDocument {
            root: self.project().root().to_path_buf(),
            source_root: self.project().source().to_path_buf(),
            scope: self.scope().clone(),
            generation: self.generation(),
            node: node.clone(),
            path: path.to_path_buf(),
            kind,
            version,
            snapshot: snapshot(&input),
            disk_snapshot: snapshot(&disk),
            input,
            owner_uuid,
            writable,
        })
    }

    /// Recheck identity, disk and policy even if no watcher event has reached the session yet.
    ///
    /// # Errors
    /// Rejects changed generations, editor versions, source ownership, disk bytes or permissions.
    pub fn check_query_document(
        &mut self,
        document: &QueryDocument,
        revision: QueryRevision<'_>,
    ) -> Result<(), QueryDocumentError> {
        if !self.check_query_read(document, revision)? {
            return Err(QueryDocumentError::ReadOnly);
        }
        Ok(())
    }

    /// A forbidden source can be inspected without turning that read into permission to edit.
    ///
    /// # Errors
    /// Rejects stale editor/metadata state, changed ownership and external source writes.
    pub fn check_query_read(
        &mut self,
        document: &QueryDocument,
        revision: QueryRevision<'_>,
    ) -> Result<bool, QueryDocumentError> {
        if revision.version != document.version || revision.snapshot != document.snapshot {
            return Err(QueryDocumentError::StaleVersion);
        }
        if document.root != self.project().root()
            || document.source_root != self.project().source()
            || &document.scope != self.scope()
            || document.generation != self.generation()
        {
            return Err(QueryDocumentError::OwnerChanged);
        }
        if self.query_location(&document.node, &document.path)? != document.kind {
            return Err(QueryDocumentError::OwnerChanged);
        }
        let (uuid, writable) = self.query_access(&document.node, &document.path)?;
        if uuid.as_deref() != Some(document.owner_uuid.as_str()) {
            return Err(QueryDocumentError::OwnerChanged);
        }
        if snapshot(&self.query_disk(&document.path)?) != document.disk_snapshot {
            return Err(QueryError::Conflict.into());
        }
        Ok(document.writable && writable)
    }

    /// Issue a byte plan after fresh source/policy checks; the caller owns editor application.
    ///
    /// # Errors
    /// Rejects stale/forbidden source snapshots and any unsupported or ambiguous query edit.
    pub fn plan_query(
        &mut self,
        document: &QueryDocument,
        revision: QueryRevision<'_>,
        identity: &QueryIdentity,
        value: &str,
        decisions: &[CommentPlacement],
        line_ending: Option<query::LineEnding>,
    ) -> Result<EditPlan, QueryDocumentError> {
        self.check_query_document(document, revision)?;
        if !document.accepts(identity) {
            return Err(QueryError::UnsupportedStructure.into());
        }
        let plan = query::plan(
            &document.input,
            &document.snapshot,
            identity,
            value,
            decisions,
            line_ending,
        )?;
        self.check_query_document(document, revision)?;
        Ok(plan)
    }

    /// A caller cannot substitute another payload, sibling module, traversal or absolute path.
    fn query_location(
        &self,
        node: &NodeId,
        path: &Path,
    ) -> Result<QueryDocumentKind, QueryDocumentError> {
        let source = self
            .source(node)?
            .into_iter()
            .find(|source| source.path == path)
            .ok_or(QueryError::NotFound)?;
        match source.role {
            SourceRole::Module(_) => Ok(QueryDocumentKind::Bsl),
            SourceRole::Payload => {
                let owner = match node {
                    NodeId::Object(owner) | NodeId::Module { owner, .. } => owner,
                    NodeId::Collection { .. } => {
                        return Err(QueryError::UnsupportedStructure.into());
                    }
                };
                match self.object(owner)?.kind {
                    MetadataKind::Form | MetadataKind::CommonForm => Ok(QueryDocumentKind::Form),
                    MetadataKind::Template | MetadataKind::CommonTemplate => {
                        Ok(QueryDocumentKind::DataComposition)
                    }
                    _ => Err(QueryError::UnsupportedStructure.into()),
                }
            }
            SourceRole::Descriptor => Err(QueryError::UnsupportedStructure.into()),
        }
    }

    /// Unknown or mixed physical ownership cannot become writable through a query view.
    fn query_access(
        &mut self,
        node: &NodeId,
        path: &Path,
    ) -> Result<(Option<String>, bool), WorkspaceError> {
        let policy = self.support_files(&[path.to_path_buf()])?;
        let owner = match node {
            NodeId::Object(owner) | NodeId::Module { owner, .. } => owner,
            NodeId::Collection { .. } => return Ok((None, false)),
        };
        let uuid = policy
            .objects
            .iter()
            .find(|object| &object.object_id == owner)
            .map(|object| object.uuid.clone());
        let writable = uuid.is_some()
            && policy.diagnostics.is_empty()
            && policy
                .files
                .iter()
                .any(|file| file.path == path && !file.read_only && !file.unknown);
        Ok((uuid, writable))
    }

    /// Read only the resolver-selected owner, with an independent limit for an unsaved query view.
    fn query_disk(&self, path: &Path) -> Result<String, QueryDocumentError> {
        let mut bytes = Vec::new();
        std::fs::File::open(self.project().source().join(path))
            .and_then(|file| {
                file.take(query::MAX_SNAPSHOT_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)
            })
            .map_err(|source| QueryDocumentError::Io {
                path: path.to_path_buf(),
                source,
            })?;
        if bytes.len() > query::MAX_SNAPSHOT_BYTES {
            return Err(QueryError::TooLarge.into());
        }
        String::from_utf8(bytes).map_err(|error| QueryDocumentError::Io {
            path: path.to_path_buf(),
            source: io::Error::new(io::ErrorKind::InvalidData, error),
        })
    }
}
