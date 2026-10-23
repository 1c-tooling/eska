//! Metadata API sessions delegate lifecycle decisions to the core and checkpoints to the shared private store.

use std::path::PathBuf;

use eska_project_core::project::{
    metadata_model::ProjectScope,
    metadata_workspace::{
        ProjectSession, QueryDocumentKind, QueryEditorState, QueryOwnerFormat,
        QueryOwnerRestrictions, QuerySession, QuerySessionRestore,
    },
    query::recovery::{Part, Record, Stored, is_id},
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{State, dto, failure, params as query_params, recovery, session_error};
use crate::ide::{
    dto as metadata_dto,
    envelope::{domain, error},
    params,
};

/// A source session cannot borrow a different member or a later private-store selection.
pub(super) struct Entry {
    pub key: query_params::Key,
    pub store_id: String,
    pub core: QuerySession,
    pub stored: Stored,
}

/// Proposed draft state is installed only after the checkpoint's disk compare-and-swap succeeds.
pub(super) struct Checkpoint {
    id: String,
    core: QuerySession,
}

/// These stateful methods are optional; older clients retain the existing extraction/planning API.
pub(super) fn is_method(method: &str) -> bool {
    matches!(
        method,
        "query/sessionOpen"
            | "query/sessionRestore"
            | "query/sessionGet"
            | "query/sessionResume"
            | "query/sessionPrepare"
            | "query/sessionApplied"
            | "query/sessionAbort"
            | "query/sessionClose"
    )
}

/// Native BSL admission remains disabled until a production adapter proves the required owner lock.
pub(super) fn capability(durable: bool) -> Value {
    let mut capability = json!({"schemaVersion":1,"writesOwner":false,"checkpointStorageVersion":1,"maxSessions":16,
        "maxRetainedBytes":super::RETAINED_BYTES,"nativeApplicationReceipt":true,
        "explicitResume":true,"preparedStateDurable":durable,"bslEditing":false,"sourceKinds":["dataCompositionSchema","form"]});
    if durable {
        capability["preparedStorageVersion"] = json!(2);
    }
    capability
}

impl State {
    /// Generic recovery may change a live session's draft, but cannot rewrite its source binding or baseline.
    pub(super) fn stage_checkpoint(
        &self,
        record: &Record,
        expected: Option<&str>,
    ) -> Result<Option<Checkpoint>, Value> {
        let Some((id, entry)) = self
            .sessions
            .iter()
            .find(|(_, entry)| entry.stored.record.id() == record.id())
        else {
            return Ok(None);
        };
        if expected != Some(entry.stored.revision.as_str())
            || record.metadata() != entry.stored.record.metadata()
            || record
                .part(Part::Owner)
                .map_err(|_| failure("recovery_invalid"))?
                != entry
                    .stored
                    .record
                    .part(Part::Owner)
                    .map_err(|_| failure("recovery_invalid"))?
            || record
                .part(Part::Base)
                .map_err(|_| failure("recovery_invalid"))?
                != entry
                    .stored
                    .record
                    .part(Part::Base)
                    .map_err(|_| failure("recovery_invalid"))?
        {
            return Err(failure("recovery_conflict"));
        }
        let mut core = entry.core.clone();
        core.set_draft(
            core.revision(),
            record
                .part(Part::Draft)
                .map_err(|_| failure("recovery_invalid"))?,
        )
        .map_err(session_error)?;
        self.admit_change(entry.core.retained_bytes(), core.retained_bytes())?;
        Ok(Some(Checkpoint {
            id: id.clone(),
            core,
        }))
    }

    /// One server request owns mutation throughout publication; failed disk writes cannot advance session state.
    pub(super) fn accept_checkpoint(&mut self, staged: Option<Checkpoint>, stored: &Stored) {
        if let Some(staged) = staged
            && let Some(entry) = self.sessions.get_mut(&staged.id)
        {
            entry.core = staged.core;
            entry.stored = stored.clone();
        }
    }

    /// A removed private record cannot be resurrected by a late source-session mutation.
    pub(super) fn release_record(&mut self, id: &str) {
        self.sessions
            .retain(|_, entry| entry.stored.record.id() != id);
    }

    /// Private inspection and cleanup stay legal after a metadata generation becomes stale.
    pub(super) fn session_cleanup(
        &mut self,
        method: &str,
        key: &query_params::Key,
        args: &Value,
    ) -> Result<Value, Value> {
        let input: Id = params::decode(args)?;
        if let Some(entry) = self.sessions.get(&input.query_session_id)
            && !entry.key.same_owner(key)
        {
            return Err(failure("unknown_query_session"));
        }
        if method == "query/sessionGet" {
            let entry = self
                .sessions
                .get(&input.query_session_id)
                .ok_or_else(|| failure("unknown_query_session"))?;
            return header(&input.query_session_id, entry);
        }
        let released = self
            .sessions
            .remove(&input.query_session_id)
            .map(|mut entry| {
                let lock = entry.core.close().map(|lock| lock.to_string());
                (lock, entry.core.restrictions().user_read_only)
            });
        Ok(
            json!({"released":released.is_some(),"releasedOwnerLock":released.as_ref().and_then(|(lock,_)|lock.clone()),
            "userReadOnly":released.map(|(_,read_only)|read_only)}),
        )
    }

    /// Route the current project only; native application acknowledgement is checked again in the core.
    pub(super) fn session_request(
        &mut self,
        project: &mut ProjectSession,
        key: &query_params::Key,
        method: &str,
        args: &Value,
        recovery: &mut recovery::State,
        forbidden: &[PathBuf],
    ) -> Result<Value, Value> {
        match method {
            "query/sessionOpen" => {
                return self.open_session(project, key, args, recovery, forbidden);
            }
            "query/sessionRestore" => return self.restore_session(project, key, args, recovery),
            _ => (),
        }
        let input: Id = params::decode(args)?;
        let query_session_id = &input.query_session_id;
        let entry = self
            .sessions
            .get(&input.query_session_id)
            .filter(|entry| entry.key.same_owner(key))
            .ok_or_else(|| failure("unknown_query_session"))?;
        if method != "query/sessionAbort" {
            recovery.checked_record(
                &entry.store_id,
                entry.stored.record.id(),
                Some(&entry.stored.revision),
                true,
            )?;
        }
        let mut core = entry.core.clone();
        let result = match operation(project, &mut core, method, args) {
            Ok(result) => result,
            Err(error) => {
                if let Some(entry) = self.sessions.get_mut(query_session_id) {
                    entry.core = core;
                }
                return Err(error);
            }
        };
        if method == "query/sessionPrepare" {
            recovery.checked_record(
                &entry.store_id,
                entry.stored.record.id(),
                Some(&entry.stored.revision),
                true,
            )?;
        }
        self.admit_change(entry.core.retained_bytes(), core.retained_bytes())?;
        let stored = if matches!(method, "query/sessionResume" | "query/sessionApplied")
            || (self.durable_sessions
                && matches!(method, "query/sessionPrepare" | "query/sessionAbort"))
        {
            let record = record(&core, entry.stored.record.id(), &project_key(project)?)?;
            recovery.publish(
                &entry.store_id,
                record,
                Some(&entry.stored.revision),
                forbidden,
            )?
        } else {
            entry.stored.clone()
        };
        let entry = self
            .sessions
            .get_mut(&input.query_session_id)
            .ok_or_else(|| failure("unknown_query_session"))?;
        entry.core = core;
        entry.stored = stored;
        let mut output = header(&input.query_session_id, entry)?;
        if let Some(plan) = result
            && !plan.is_null()
        {
            output["plan"] = plan;
        }
        Ok(output)
    }

    /// Opening an editable view atomically publishes its initial complete checkpoint before returning its handle.
    fn open_session(
        &mut self,
        project: &mut ProjectSession,
        key: &query_params::Key,
        args: &Value,
        recovery: &mut recovery::State,
        forbidden: &[PathBuf],
    ) -> Result<Value, Value> {
        let input: Open = params::decode(args)?;
        if !is_id(&input.id) {
            return Err(error(-32602));
        }
        let document = self
            .documents
            .get(&input.document_id)
            .filter(|entry| &entry.key == key)
            .ok_or_else(|| failure("unknown_document"))?;
        if document.document.kind() == QueryDocumentKind::Bsl {
            return Err(failure("owner_lock_required"));
        }
        self.session_slot(key, document.document.path())?;
        if let Some(id) = recovery.pending_owner(
            &project_key(project)?,
            &metadata_dto::path(document.document.path()),
            None,
        ) {
            return Err(domain(
                "query_failed",
                json!({"reason":"pending_apply","id":id}),
            ));
        }
        let core = QuerySession::open(
            project,
            document.document.clone(),
            input.query.core()?,
            document.application.as_deref(),
            input.format.core(),
            QueryOwnerRestrictions {
                user_read_only: input.user_read_only,
                owner_lock: None,
            },
        )
        .map_err(session_error)?;
        if core.context_fingerprint() != document.fingerprint {
            return Err(failure("context_changed"));
        }
        self.admit_change(document.document.input().len(), core.retained_bytes())?;
        let record = record(&core, &input.id, &project_key(project)?)?;
        let id = self.next_id("q")?;
        let stored = recovery.publish(&input.store_id, record, None, forbidden)?;
        self.documents.remove(&input.document_id);
        let entry = Entry {
            key: key.clone(),
            store_id: input.store_id,
            core,
            stored,
        };
        let output = header(&id, &entry)?;
        self.sessions.insert(id, entry);
        Ok(output)
    }

    /// Restore a checkpoint as paused; only a separate resume can inspect current editor/disk state and grant a plan.
    fn restore_session(
        &mut self,
        project: &mut ProjectSession,
        key: &query_params::Key,
        args: &Value,
        recovery: &recovery::State,
    ) -> Result<Value, Value> {
        let input: Restore = params::decode(args)?;
        let stored =
            recovery.checked_record(&input.store_id, &input.id, Some(&input.revision), false)?;
        if stored.record.prepared().is_some() && !self.durable_sessions {
            return Err(failure("pending_apply"));
        }
        let saved = restored(project, &stored.record)?;
        self.session_slot(key, &saved.path)?;
        if let Some(id) = recovery.pending_owner(
            &project_key(project)?,
            &metadata_dto::path(&saved.path),
            Some(&input.id),
        ) {
            return Err(domain(
                "query_failed",
                json!({"reason":"pending_apply","id":id}),
            ));
        }
        let core =
            QuerySession::restore(project, saved, input.format.core()).map_err(session_error)?;
        self.admit(core.retained_bytes())?;
        let id = self.next_id("q")?;
        let entry = Entry {
            key: key.clone(),
            store_id: input.store_id,
            core,
            stored: stored.clone(),
        };
        let output = header(&id, &entry)?;
        self.sessions.insert(id, entry);
        Ok(output)
    }

    /// One source owner has one active query session, regardless of tab labels or a client's duplicate-open logic.
    fn session_slot(&self, key: &query_params::Key, path: &std::path::Path) -> Result<(), Value> {
        if self.sessions.len() >= 16 {
            return Err(domain(
                "resource_limit",
                json!({"resource":"query_sessions","maxEntries":16}),
            ));
        }
        if let Some((id, entry)) = self
            .sessions
            .iter()
            .find(|(_, entry)| entry.key.same_owner(key) && entry.core.document().path() == path)
        {
            return Err(domain(
                "query_failed",
                json!({"reason":"already_open","querySessionId":id,"id":entry.stored.record.id()}),
            ));
        }
        Ok(())
    }
}

/// Execute a proposed core transition; the caller publishes its checkpoint before installing successful state.
fn operation(
    project: &mut ProjectSession,
    core: &mut QuerySession,
    method: &str,
    args: &Value,
) -> Result<Option<Value>, Value> {
    match method {
        "query/sessionResume" => {
            let input: Resume = params::decode(args)?;
            if params::counter(&input.revision)? != core.revision() {
                return Err(failure("stale_draft"));
            }
            core.resume(
                project,
                input.owner.core()?,
                QueryOwnerRestrictions {
                    user_read_only: input.owner.user_read_only,
                    owner_lock: None,
                },
            )
            .map_err(session_error)?;
            Ok(None)
        }
        "query/sessionPrepare" => {
            let input: Prepare = params::decode(args)?;
            let comments = input
                .comments
                .iter()
                .map(query_params::Placement::core)
                .collect::<Result<Vec<_>, _>>()?;
            let prepared = core
                .prepare(
                    project,
                    params::counter(&input.revision)?,
                    input.owner.core()?,
                    &comments,
                    input.line_ending.map(query_params::LineEnding::core),
                )
                .map_err(session_error)?;
            let plan = &prepared.plan;
            Ok(Some(
                json!({"applyToken":prepared.token.to_string(),"saveOwner":prepared.save_owner,
                "noOp":plan.is_empty(),"outputSnapshot":eska_project_core::project::metadata_edit::snapshot(plan.output()),
                "replacements":plan.replacements().iter().map(|replacement|json!({"range":dto::bytes(&replacement.range),
                    "rangeUtf16":{"start":dto::point(plan.original(),replacement.range.start),"end":dto::point(plan.original(),replacement.range.end)},"text":replacement.text})).collect::<Vec<_>>(),
                "projectedQuery":dto::query(plan.output(),&prepared.query)}),
            ))
        }
        "query/sessionApplied" => {
            let input: Applied = params::decode(args)?;
            core.applied(
                project,
                params::counter(&input.apply_token)?,
                input.owner.core()?,
            )
            .map_err(session_error)?;
            Ok(None)
        }
        "query/sessionAbort" => {
            let input: Abort = params::decode(args)?;
            core.abort_apply(params::counter(&input.apply_token)?)
                .map_err(session_error)?;
            Ok(None)
        }
        _ => Err(error(-32601)),
    }
}

/// A header describes a remembered snapshot and its current lifecycle, not permission to skip a later preflight.
fn header(id: &str, entry: &Entry) -> Result<Value, Value> {
    let restrictions = entry.core.restrictions();
    Ok(
        json!({"querySessionId":id,"storeId":entry.store_id,"id":entry.stored.record.id(),
        "revision":entry.core.revision().to_string(),"resumed":entry.core.resumed(),"applyPending":entry.core.applying(),
        "draftWritable":entry.core.draft_writable(),"format":match entry.core.owner_format(){QueryOwnerFormat::Editor=>"editor",QueryOwnerFormat::Source=>"source"},
        "protection":{"capturedSourceWritable":entry.core.document().writable(),"userReadOnly":restrictions.user_read_only,
            "ownerLock":restrictions.owner_lock.map(|id|id.to_string()),
            "sourceRestriction":entry.core.source_restriction_id().map(|id|id.to_string())},"record":recovery::header(&entry.stored)?}),
    )
}

/// Private-format fields are projected from the core; the adapter does not choose owner/baseline updates.
fn record(core: &QuerySession, id: &str, project_key: &str) -> Result<Record, Value> {
    let document = core.document();
    let (descriptor, descriptor_snapshot) = document.descriptor();
    let metadata = json!({"schemaVersion":1,"id":id,"projectKey":project_key,"node":metadata_dto::node_id(document.node()),
        "source":metadata_dto::path(document.path()),"descriptor":metadata_dto::path(descriptor),"descriptorSnapshot":descriptor_snapshot,
        "diskSnapshot":document.disk_snapshot(),"ownerSnapshot":document.snapshot(),"ownerVersion":document.version().to_string(),
        "contextFingerprint":core.context_fingerprint(),"application":core.application(),"identity":dto::identity(core.identity()),"writable":document.writable()});
    Record::new(metadata, core.texts())
        .and_then(|record| record.with_prepared(core.prepared()))
        .map_err(|_| failure("recovery_invalid"))
}

/// Existing checkpoint keys become backend-produced identities while keeping the v1 data readable.
fn project_key(project: &ProjectSession) -> Result<String, Value> {
    let path = metadata_dto::path(project.project().root());
    let scope = match project.scope() {
        ProjectScope::Standalone => json!({"kind":"standalone"}),
        ProjectScope::Member(name) => json!({"kind":"member","name":name.as_str()}),
    };
    serde_json::to_string(&json!([path["encoding"], path["value"], scope]))
        .map_err(|_| error(-32603))
}

/// Remembered paths and node IDs are decoded once; the core checks every binding before granting a source plan.
fn restored(project: &ProjectSession, record: &Record) -> Result<QuerySessionRestore, Value> {
    let metadata = record.metadata();
    if metadata["projectKey"] != project_key(project)? {
        return Err(failure("owner_changed"));
    }
    let text = |key: &str| {
        metadata[key]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| failure("recovery_invalid"))
    };
    let path = |key| params::decode::<params::Path>(&metadata[key])?.native();
    let identity: query_params::Identity = params::decode(&metadata["identity"])?;
    Ok(QuerySessionRestore {
        root: project.project().root().to_owned(),
        scope: project.scope().clone(),
        node: params::node(&metadata["node"])?,
        path: path("source")?,
        descriptor: path("descriptor")?,
        descriptor_snapshot: text("descriptorSnapshot")?,
        disk_snapshot: text("diskSnapshot")?,
        owner_snapshot: text("ownerSnapshot")?,
        owner_version: params::counter(&text("ownerVersion")?)?,
        context_fingerprint: text("contextFingerprint")?,
        application: metadata["application"].as_str().map(str::to_owned),
        identity: identity.core()?,
        owner_text: record
            .part(Part::Owner)
            .map_err(|_| failure("recovery_invalid"))?,
        base: record
            .part(Part::Base)
            .map_err(|_| failure("recovery_invalid"))?,
        draft: record
            .part(Part::Draft)
            .map_err(|_| failure("recovery_invalid"))?,
        writable: metadata["writable"]
            .as_bool()
            .ok_or_else(|| failure("recovery_invalid"))?,
        prepared: record.prepared().cloned(),
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Id {
    query_session_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Open {
    document_id: String,
    store_id: String,
    id: String,
    query: query_params::Identity,
    format: Format,
    user_read_only: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Restore {
    store_id: String,
    id: String,
    revision: String,
    format: Format,
}
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Format {
    Editor,
    Source,
}
impl Format {
    /// The same source encoding signature can be absent or present in an IDE's exact text model.
    const fn core(self) -> QueryOwnerFormat {
        match self {
            Self::Editor => QueryOwnerFormat::Editor,
            Self::Source => QueryOwnerFormat::Source,
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Owner {
    version: String,
    snapshot: String,
    is_dirty: bool,
    user_read_only: bool,
    application: Value,
}
impl Owner {
    /// Native buffer facts remain explicit and never include a VS Code URI or session-readonly command.
    fn core(&self) -> Result<QueryEditorState<'_>, Value> {
        let application = if self.application.is_null() {
            None
        } else {
            Some(
                self.application
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| error(-32602))?,
            )
        };
        Ok(QueryEditorState {
            version: params::counter(&self.version)?,
            snapshot: &self.snapshot,
            is_dirty: self.is_dirty,
            user_read_only: self.user_read_only,
            owner_lock: None,
            application,
        })
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Resume {
    revision: String,
    owner: Owner,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Prepare {
    revision: String,
    owner: Owner,
    comments: Vec<query_params::Placement>,
    line_ending: Option<query_params::LineEnding>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Applied {
    apply_token: String,
    owner: Owner,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Abort {
    apply_token: String,
}
