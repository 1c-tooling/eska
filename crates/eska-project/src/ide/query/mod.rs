//! Snapshot-bound query RPCs plan editor changes and never publish source files.

mod buffers;
mod documents;
mod dto;
mod params;
mod recovery;

use std::collections::BTreeMap;

use eska_project_core::project::{metadata_workspace::QueryDocumentError, query::QueryError};
use serde_json::{Value, json};

use super::{
    envelope::{domain, error},
    errors,
    server::Server,
};
use params::Key;
pub(super) use recovery::State as RecoveryState;

const INLINE_BYTES: usize = 128 * 1024;
const RETAINED_BYTES: usize = 32 * 1024 * 1024;
const MAX_DOCUMENTS: usize = 16;

/// Every retained byte belongs to this connection and is released on workspace close/shutdown.
#[derive(Default)]
pub(super) struct State {
    buffers: BTreeMap<String, buffers::Upload>,
    documents: BTreeMap<String, documents::Entry>,
    serial: u64,
}

/// Request-only routing prevents notifications from allocating retained source snapshots.
pub(super) fn is_method(method: &str) -> bool {
    recovery::is_method(method)
        || matches!(
            method,
            "query/bufferBegin"
                | "query/bufferAppend"
                | "query/bufferClose"
                | "query/open"
                | "query/close"
                | "query/list"
                | "query/read"
                | "query/map"
                | "query/plan"
        )
}

/// Recovery is independent of source generations and never writes a project owner.
pub(super) fn recovery_capability() -> Value {
    recovery::capability()
}

/// Recovery handles belong to the connection rather than to a successfully opened metadata workspace.
pub(super) fn is_recovery_method(method: &str) -> bool {
    recovery::is_method(method)
}

/// Negotiate resource bounds explicitly without changing framing for older API clients.
pub(super) fn capability() -> Value {
    json!({"schemaVersion":1,"planOnly":true,"writesOwner":false,"editorLineEndings":true,
        "maxSnapshotBytes":eska_project_core::project::query::MAX_SNAPSHOT_BYTES,
        "maxInlineBytes":INLINE_BYTES,"maxRetainedBytes":RETAINED_BYTES,"maxDocuments":MAX_DOCUMENTS,
        "maxRetainedEntries":64,"maxMappingPositions":64,"maxListItems":64,
        "maxXmlQueries":1024,"maxMappings":65_536,
        "maxSourceComments":eska_project_core::project::query::MAX_SOURCE_COMMENTS,
        "sourceKinds":["dataCompositionSchema","form","bsl"]})
}

impl State {
    /// Include reserved upload capacity so unfinished requests cannot evade the byte budget.
    fn admit(&self, bytes: usize) -> Result<(), Value> {
        let used: usize = self
            .buffers
            .values()
            .map(|buffer| buffer.bytes)
            .sum::<usize>()
            + self
                .documents
                .values()
                .map(|entry| entry.document.input().len())
                .sum::<usize>();
        if self.buffers.len() + self.documents.len() >= 64
            || used.saturating_add(bytes) > RETAINED_BYTES
        {
            return Err(resource());
        }
        Ok(())
    }

    /// IDs are never reused within the connection's live query state.
    fn next_id(&mut self, prefix: &str) -> Result<String, Value> {
        self.serial = self.serial.checked_add(1).ok_or_else(resource)?;
        Ok(format!("{prefix}{}", self.serial))
    }

    /// Cleanup remains legal after a source event, without accepting another member's IDs.
    fn close(&mut self, method: &str, key: &Key, args: &Value) -> Result<Value, Value> {
        if method == "query/bufferClose" {
            let input: params::BufferId = super::params::decode(args)?;
            return buffers::close(&mut self.buffers, key, &input.buffer_id);
        }
        let input: params::DocumentId = super::params::decode(args)?;
        if self
            .documents
            .get(&input.document_id)
            .is_some_and(|entry| !entry.key.same_owner(key))
        {
            return Err(failure("unknown_document"));
        }
        Ok(json!({"released":self.documents.remove(&input.document_id).is_some()}))
    }
}

impl Server {
    /// A broken manifest cannot prevent inspection or preservation of an inert private checkpoint.
    pub(super) fn query_recovery(&mut self, method: &str, args: &Value) -> Result<Value, Value> {
        let mut forbidden = self.recovery_root.iter().cloned().collect::<Vec<_>>();
        if let Some(session) = &self.session {
            forbidden.extend(session.workspace.projects().iter().flat_map(|project| {
                [
                    project.project().root().to_path_buf(),
                    project.project().source().to_path_buf(),
                ]
            }));
        }
        self.recovery.request(method, args, &forbidden)
    }

    /// Share the metadata session's refresh/reopen boundary while retaining query state separately.
    pub(super) fn query(&mut self, method: &str, args: &Value) -> Result<Value, Value> {
        let key = Key::decode(args)?;
        self.check_session(args)?;
        let session = self.session.as_mut().ok_or_else(|| error(-32603))?;
        let state = session
            .projects
            .iter()
            .find(|state| state.id == key.project)
            .ok_or_else(|| domain("unknown_project", json!({"projectId":key.project})))?;
        if matches!(method, "query/close" | "query/bufferClose") {
            return self.queries.close(method, &key, args);
        }
        if state.reopen {
            return Err(domain(
                "reopen_required",
                json!({"reason":"manifest_changed"}),
            ));
        }
        if state.refresh {
            return Err(domain("resync_required", json!({"reason":"file_events"})));
        }
        let project = session
            .workspace
            .project_mut(&state.scope)
            .map_err(|failure| errors::workspace(&failure))?;
        project
            .check_generation(super::params::counter(&key.generation)?)
            .map_err(|failure| errors::workspace(&failure))?;
        let result = match method {
            "query/bufferBegin" => self.queries.begin_buffer(&key, args),
            "query/bufferAppend" => self.queries.append_buffer(&key, args),
            "query/open" => self.queries.open_document(project, &key, args),
            _ => self.queries.document_request(project, &key, method, args),
        };
        result.map(|mut result| {
            result["sessionId"] = json!(key.session);
            result["projectId"] = json!(key.project);
            result["generation"] = json!(key.generation);
            result
        })
    }
}

/// Query failures contain machine reasons, not source fragments, debug values or localized messages.
fn failure(reason: &str) -> Value {
    domain("query_failed", json!({"reason":reason}))
}

/// Shared limits cover both pending upload reservations and completed owner snapshots.
fn resource() -> Value {
    domain(
        "resource_limit",
        json!({"resource":"query_snapshots","maxBytes":RETAINED_BYTES,"maxEntries":64}),
    )
}

/// Preserve ownership and policy failures while keeping query codecs independent of transport.
fn document_error(error: &QueryDocumentError) -> Value {
    match error {
        QueryDocumentError::Workspace(error) => errors::workspace(error),
        QueryDocumentError::Query(error) => codec_error(*error),
        QueryDocumentError::ReadOnly => failure("read_only"),
        QueryDocumentError::OwnerChanged => failure("owner_changed"),
        QueryDocumentError::StaleVersion => failure("stale_version"),
        QueryDocumentError::Io { path, source } => domain(
            "source_invalid",
            json!({"path":super::dto::path(path),"reason":if source.kind() == std::io::ErrorKind::NotFound {"missing"} else {"io"}}),
        ),
    }
}

/// Codec error names are versioned independently of human UI wording.
fn codec_error(error: QueryError) -> Value {
    failure(match error {
        QueryError::TooLarge => "too_large",
        QueryError::InvalidXml => "invalid_xml",
        QueryError::UnsupportedStructure => "unsupported_structure",
        QueryError::InvalidLiteral => "invalid_literal",
        QueryError::UnsupportedExpression => "unsupported_expression",
        QueryError::NotFound => "not_found",
        QueryError::InvalidPosition => "invalid_position",
        QueryError::InvalidValue => "invalid_value",
        QueryError::Conflict => "conflict",
        QueryError::CommentsRequireDecision => "comments_require_decision",
    })
}
