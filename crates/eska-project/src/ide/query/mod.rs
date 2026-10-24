//! Snapshot-bound query RPCs plan editor changes and never publish source files.

mod buffers;
mod constructors;
mod documents;
mod dto;
mod params;
mod projections;
mod recovery;
mod sessions;
mod source;

use std::collections::BTreeMap;

use eska_project_core::project::{
    metadata_workspace::{QueryDocumentError, QuerySessionError},
    query::QueryError,
};
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
    sessions: BTreeMap<String, sessions::Entry>,
    source_snapshots: BTreeMap<String, source::Entry>,
    projections: BTreeMap<String, projections::Entry>,
    serial: u64,
    durable_sessions: bool,
    bsl_sessions: bool,
}

/// Request-only routing prevents notifications from allocating retained source snapshots.
pub(super) fn is_method(method: &str) -> bool {
    recovery::is_method(method)
        || sessions::is_method(method)
        || source::is_method(method)
        || projections::is_method(method)
        || constructors::is_method(method)
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

/// Stateful session methods require their separately negotiated contract.
pub(super) fn is_session_method(method: &str) -> bool {
    sessions::is_method(method)
}

/// Language projection is additive and requires its own negotiated minor version.
pub(super) fn is_projection_method(method: &str) -> bool {
    projections::is_method(method)
}

/// Only immutable BSL projections are available; XML language context is deliberately not advertised.
pub(super) fn projection_capability() -> Value {
    projections::capability()
}

/// The constructor never writes the owner; only explicit finish updates its existing private draft.
pub(super) fn constructor_capability(minor: u32) -> Value {
    constructors::capability(minor)
}

/// Each constructor extension has one explicit minimum, before any session lookup or mutation.
pub(super) fn constructor_api_minor(method: &str, args: &Value) -> u32 {
    if !constructors::is_method(method) {
        return 0;
    }
    if method == "query/constructorCatalog" {
        if !args["query"]["atStatement"].is_null() {
            return 26;
        }
        return if args["query"]["virtualTables"] == true
            || !args["query"]["source"].is_null()
            || !args["query"]["scope"].is_null()
        {
            23
        } else {
            22
        };
    }
    if method == "query/constructorAction" {
        if matches!(
            args["action"]["kind"].as_str(),
            Some("addTotals" | "removeTotals" | "moveTotals" | "composeTotal" | "appendTotal")
        ) {
            return 28;
        }
        if args["action"]["clause"] == "having" || args["action"]["expression"]["kind"] == "having"
        {
            return 27;
        }
        if matches!(
            args["action"]["kind"].as_str(),
            Some("setInto" | "addStatement" | "removeStatement" | "moveStatement")
        ) || matches!(
            args["action"]["clause"].as_str(),
            Some("into" | "destroy" | "index")
        ) {
            return 26;
        }
        if matches!(
            args["action"]["kind"].as_str(),
            Some("addUnion" | "setUnion" | "removeUnion")
        ) {
            return 23;
        }
        if matches!(
            args["action"]["expression"]["kind"].as_str(),
            Some(
                "virtualSource"
                    | "virtualJoin"
                    | "subquerySource"
                    | "subqueryJoin"
                    | "totalField"
                    | "totalGroup"
                    | "overall"
            )
        ) {
            return 23;
        }
        if matches!(
            args["action"]["clause"].as_str(),
            Some("totals" | "totalGroups")
        ) {
            return 23;
        }
        if matches!(
            args["action"]["kind"].as_str(),
            Some("compose" | "appendFields")
        ) {
            return 22;
        }
    }
    21
}

/// Shared protected-source inspection is an additive contract negotiated independently from query lifecycle.
pub(super) fn is_source_method(method: &str) -> bool {
    source::is_method(method)
}

/// Source snapshots reuse the existing upload and retained-text limits.
pub(super) fn source_capability(rooted_previews: bool) -> Value {
    source::capability(rooted_previews)
}

/// Manifest-independent preview requests remain read-only and have no metadata session authority.
pub(super) fn is_source_preview_method(method: &str) -> bool {
    source::is_preview_method(method)
}

/// Existing query extraction remains independent from optional lifecycle/checkpoint ownership.
pub(super) fn session_capability(durable: bool, bsl: bool) -> Value {
    sessions::capability(durable, bsl)
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

impl Server {
    /// Preserve protected-view restoration without a manifest; the server owns the fixed containment root.
    pub(super) fn source_preview(&mut self, method: &str, args: &Value) -> Result<Value, Value> {
        self.queries
            .source_preview(self.source_root.as_deref(), method, args)
    }
}

impl State {
    /// Workspace teardown drops every owned resource without reusing a connection's immutable preview identifiers.
    pub(super) fn clear(&mut self) {
        *self = Self {
            serial: self.serial,
            ..Self::default()
        };
    }

    /// Include reserved upload capacity so unfinished requests cannot evade the byte budget.
    fn admit(&self, bytes: usize) -> Result<(), Value> {
        if self.buffers.len()
            + self.documents.len()
            + self.sessions.len()
            + self.source_snapshots.len()
            + self.projections.len()
            >= 64
            || self.retained_bytes().saturating_add(bytes) > RETAINED_BYTES
        {
            return Err(resource());
        }
        Ok(())
    }

    /// Replacing state must not charge its unchanged shared owner twice or allocate another entry.
    fn admit_change(&self, previous: usize, next: usize) -> Result<(), Value> {
        if self
            .retained_bytes()
            .saturating_sub(previous)
            .saturating_add(next)
            > RETAINED_BYTES
        {
            return Err(resource());
        }
        Ok(())
    }

    /// Budgets cover reserved uploads, transient documents and all persistent-in-connection session texts.
    fn retained_bytes(&self) -> usize {
        self.buffers
            .values()
            .map(|buffer| buffer.bytes)
            .sum::<usize>()
            + self
                .documents
                .values()
                .map(|entry| entry.document.input().len())
                .sum::<usize>()
            + self
                .projections
                .values()
                .map(projections::Entry::bytes)
                .sum::<usize>()
            + self
                .sessions
                .values()
                .map(|entry| entry.core.retained_bytes())
                .sum::<usize>()
            + self
                .source_snapshots
                .values()
                .map(source::Entry::bytes)
                .sum::<usize>()
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
        if method == "query/recoveryRemove"
            && args["discardPrepared"] == true
            && self.client_minor < 16
        {
            return Err(error(-32602));
        }
        let forbidden = self.recovery_forbidden();
        let mut result = self
            .recovery
            .request(method, args, &forbidden, &mut self.queries)?;
        if method == "query/recoveryOpen" && self.client_minor < 16 {
            let mut pending = Vec::new();
            if let Some(records) = result["records"].as_array_mut() {
                records.retain(|record| {
                    if record["prepared"].is_object() {
                        pending.push(record["id"].clone());
                        false
                    } else {
                        true
                    }
                });
            }
            if let Some(damaged) = result["damaged"].as_array_mut() {
                damaged.extend(pending);
            }
        }
        if matches!(method, "query/recoveryOpen" | "query/recoveryClose") {
            self.queries.sessions.clear();
        }
        if method == "query/recoveryRemove"
            && let Some(id) = args["id"].as_str()
        {
            self.queries.release_record(id);
        }
        Ok(result)
    }

    /// Session checkpoint writes use the same complete source-location guard as generic recovery writes.
    fn recovery_forbidden(&self) -> Vec<std::path::PathBuf> {
        let mut forbidden = self.recovery_root.iter().cloned().collect::<Vec<_>>();
        if let Some(session) = &self.session {
            forbidden.extend(session.workspace.projects().iter().flat_map(|project| {
                [
                    project.project().root().to_path_buf(),
                    project.project().source().to_path_buf(),
                ]
            }));
        }
        forbidden
    }

    /// Share the metadata session's refresh/reopen boundary while retaining query state separately.
    pub(super) fn query(&mut self, method: &str, args: &Value) -> Result<Value, Value> {
        self.queries.durable_sessions = self.client_minor >= 16;
        self.queries.bsl_sessions = self.client_minor >= 19;
        let forbidden = self.recovery_forbidden();
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
        if method == "source/bufferClose" {
            return self.queries.close("query/bufferClose", &key, args);
        }
        if matches!(method, "source/snapshotRead" | "source/snapshotClose") {
            return self.queries.source_cleanup(Some(&key), method, args);
        }
        if projections::is_method(method) && method != "query/projectionOpen" {
            return self.queries.projection_read(&key, method, args);
        }
        if constructors::is_cleanup(method) {
            return self.queries.constructor_request(
                &key,
                method,
                args,
                &mut self.recovery,
                &forbidden,
                self.client_minor,
            );
        }
        if matches!(method, "query/sessionClose" | "query/sessionGet") {
            return self.queries.session_cleanup(method, &key, args);
        }
        let receipt = matches!(method, "query/sessionApplied" | "query/sessionAbort");
        if state.reopen && !receipt {
            return Err(domain(
                "reopen_required",
                json!({"reason":"manifest_changed"}),
            ));
        }
        if state.refresh && !receipt {
            return Err(domain("resync_required", json!({"reason":"file_events"})));
        }
        let project = session
            .workspace
            .project_mut(&state.scope)
            .map_err(|failure| errors::workspace(&failure))?;
        if !receipt {
            project
                .check_generation(super::params::counter(&key.generation)?)
                .map_err(|failure| errors::workspace(&failure))?;
        }
        let result = match method {
            "query/constructorCatalog" => self.queries.constructor_catalog(project, &key, args),
            "query/bufferBegin" => self.queries.begin_buffer(&key, args),
            "query/bufferAppend" => self.queries.append_buffer(&key, args),
            "query/open" => self.queries.open_document(project, &key, args),
            "query/projectionOpen" => {
                self.queries
                    .open_projection(project, &key, args, &self.recovery)
            }
            method if source::is_method(method) => {
                self.queries.source_request(project, &key, method, args)
            }
            method if constructors::is_method(method) => self.queries.constructor_request(
                &key,
                method,
                args,
                &mut self.recovery,
                &forbidden,
                self.client_minor,
            ),
            method if sessions::is_method(method) => self.queries.session_request(
                project,
                &key,
                method,
                args,
                &mut self.recovery,
                &forbidden,
            ),
            _ => self.queries.document_request(project, &key, method, args),
        };
        result.map(|mut result| {
            result["sessionId"] = json!(key.session);
            result["projectId"] = json!(key.project);
            result["generation"] = json!(if receipt {
                project.generation().to_string()
            } else {
                key.generation.clone()
            });
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

/// The core owns lifecycle/context decisions; transport supplies only stable machine categories.
fn session_error(error: QuerySessionError) -> Value {
    match error {
        QuerySessionError::Document(error) => document_error(&error),
        QuerySessionError::Context(error) => domain(
            "query_context_unavailable",
            crate::analyzer_config::error_document(&eska_analyzer::Error::Context(error)),
        ),
        QuerySessionError::ContextMemberMismatch => failure("context_member_mismatch"),
        QuerySessionError::ContextChanged => failure("context_changed"),
        QuerySessionError::ResumeRequired => failure("resume_required"),
        QuerySessionError::OwnerBusy => failure("owner_busy"),
        QuerySessionError::UserReadOnly => failure("user_read_only"),
        QuerySessionError::StaleDraft => failure("stale_draft"),
        QuerySessionError::ApplyBusy => failure("apply_busy"),
        QuerySessionError::UnknownApply => failure("unknown_apply"),
        QuerySessionError::InvalidReceipt => failure("invalid_receipt"),
        QuerySessionError::Closed => failure("closed_session"),
        QuerySessionError::RevisionExhausted => failure("session_limit"),
        QuerySessionError::ConstructorBusy => failure("constructor_busy"),
        QuerySessionError::UnknownConstructor => failure("unknown_constructor"),
        QuerySessionError::Constructor(error) => constructor_error(error),
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

/// Keep visual editing failures portable and independent of any IDE presentation.
fn constructor_error(error: eska_project_core::project::query::constructor::Error) -> Value {
    use eska_project_core::project::query::constructor::Error;
    failure(match error {
        Error::TooLarge => "constructor_limit",
        Error::InvalidSyntax => "constructor_invalid_syntax",
        Error::Unsupported => "constructor_unsupported",
        Error::InvalidSelection => "constructor_invalid_selection",
        Error::InvalidExpression => "constructor_invalid_expression",
        Error::RequiredItem => "constructor_required_item",
        Error::SourceRequired => "constructor_source_required",
        Error::UnsafeMove => "constructor_unsafe_move",
        Error::CommentsRequireDecision => "constructor_comments_require_decision",
        Error::StaleRevision => "stale_constructor",
    })
}
