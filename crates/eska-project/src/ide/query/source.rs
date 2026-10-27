//! Protected source views share checked ownership, immutable bytes and query upload budgets.

use base64::{Engine, engine::general_purpose::STANDARD};
use eska_project_core::project::metadata_workspace::{
    MAX_SOURCE_SNAPSHOT_BYTES, NativeSourceSnapshot, ProjectSession, SourceAccessRequest,
    SourceProtection, SourceSnapshot, SourceSnapshotError, SourceSnapshotOrigin,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{INLINE_BYTES, MAX_DOCUMENTS, RETAINED_BYTES, State, params::Key};
use crate::ide::{
    dto,
    envelope::{domain, error},
    errors, params as metadata_params,
};

pub(super) struct Entry {
    key: Option<Key>,
    view: SourceSnapshot,
}

impl Entry {
    /// Immutable view bytes participate in the same aggregate budget as native owner uploads and query sessions.
    pub(super) fn bytes(&self) -> usize {
        self.view.bytes().len()
    }
}

/// This optional API does not change the framing or opt older clients into protected-source handles.
pub(super) fn capability(rooted_previews: bool) -> Value {
    json!({"schemaVersion":1,"writesOwner":false,"sessionOwnerPriority":true,"binarySnapshots":true,
        "maxSnapshotBytes":MAX_SOURCE_SNAPSHOT_BYTES,"maxSnapshots":MAX_DOCUMENTS,
        "maxRetainedBytes":RETAINED_BYTES,"sharedQueryBudget":true,"maxChunkBytes":INLINE_BYTES,
        "maxPaths":128,"maxRetainedEntries":64,"rootedPreviews":rooted_previews})
}

/// Project-independent previews cannot address metadata handles or supply a query/native owner.
pub(super) fn is_preview_method(method: &str) -> bool {
    matches!(
        method,
        "source/previewOpen" | "source/previewRead" | "source/previewClose"
    )
}

/// Keep source buffer aliases request-only and behind their separately negotiated minor.
pub(super) fn is_method(method: &str) -> bool {
    matches!(
        method,
        "source/protection"
            | "source/snapshotOpen"
            | "source/snapshotRead"
            | "source/snapshotClose"
            | "source/bufferBegin"
            | "source/bufferAppend"
            | "source/bufferClose"
    )
}

impl State {
    /// Stale metadata cannot revoke already captured bytes or prevent exact cleanup of a foreign-checked handle.
    pub(super) fn source_cleanup(
        &mut self,
        key: Option<&Key>,
        method: &str,
        args: &Value,
    ) -> Result<Value, Value> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Input {
            snapshot_id: String,
        }
        #[derive(Deserialize)]
        struct Read {
            snapshot: String,
            offset: usize,
            bytes: usize,
        }
        let input: Input = metadata_params::decode(args)?;
        let entry = self.source_snapshots.get(&input.snapshot_id);
        if entry.is_some_and(|entry| match (&entry.key, key) {
            (Some(entry), Some(key)) => !entry.same_owner(key),
            (None, None) => false,
            _ => true,
        }) {
            return Err(failure("unknown_snapshot"));
        }
        if matches!(method, "source/snapshotClose" | "source/previewClose") {
            return Ok(
                json!({"released":self.source_snapshots.remove(&input.snapshot_id).is_some()}),
            );
        }
        let entry = entry.ok_or_else(|| failure("unknown_snapshot"))?;
        let input: Read = metadata_params::decode(args)?;
        let body = entry.view.bytes();
        if input.snapshot != entry.view.snapshot {
            return Err(failure("snapshot_mismatch"));
        }
        if input.bytes == 0 || input.bytes > INLINE_BYTES || input.offset > body.len() {
            return Err(error(-32602));
        }
        let end = input.offset.saturating_add(input.bytes).min(body.len());
        Ok(
            json!({"snapshotId":args["snapshotId"],"snapshot":entry.view.snapshot,"offset":input.offset,
            "data":STANDARD.encode(&body[input.offset..end]),"nextOffset":end,"complete":end==body.len()}),
        )
    }

    /// The served folder is fixed by the process entry point; a protocol request cannot choose a different root.
    pub(super) fn source_preview(
        &mut self,
        root: Option<&std::path::Path>,
        method: &str,
        args: &Value,
    ) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Input {
            path: metadata_params::Path,
        }
        if method != "source/previewOpen" {
            return self.source_cleanup(None, method, args);
        }
        let input: Input = metadata_params::decode(args)?;
        let path = input.path.native()?;
        if self.source_snapshots.len() >= MAX_DOCUMENTS {
            return Err(super::resource());
        }
        let root = root.ok_or_else(|| failure("preview_unavailable"))?;
        let view = SourceSnapshot::preview(root, &path).map_err(snapshot_error)?;
        self.admit(view.bytes().len())?;
        let id = self.next_id("s")?;
        let result = header(&id, &path, &view);
        self.source_snapshots.insert(id, Entry { key: None, view });
        Ok(result)
    }

    /// Source admission reuses the current project and never selects an arbitrary path outside its declared source.
    pub(super) fn source_request(
        &mut self,
        project: &mut ProjectSession,
        key: &Key,
        method: &str,
        args: &Value,
    ) -> Result<Value, Value> {
        match method {
            "source/bufferBegin" => self.begin_buffer(key, args),
            "source/bufferAppend" => self.append_buffer(key, args),
            "source/protection" => protection(project, args),
            "source/snapshotOpen" => self.open_source_snapshot(project, key, args),
            _ => Err(error(-32601)),
        }
    }

    /// An active query's full owner outranks both the current disk and a later adapter-supplied buffer.
    fn open_source_snapshot(
        &mut self,
        project: &mut ProjectSession,
        key: &Key,
        args: &Value,
    ) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Input {
            path: metadata_params::Path,
            native: Option<Native>,
        }
        let input: Input = metadata_params::decode(args)?;
        let path = input.path.native()?;
        if self.source_snapshots.len() >= MAX_DOCUMENTS {
            return Err(super::resource());
        }
        let owner = self
            .sessions
            .values()
            .find(|entry| entry.key.same_owner(key) && entry.core.document().path() == path);
        let native = input
            .native
            .as_ref()
            .map(|native| -> Result<NativeSourceSnapshot, Value> {
                let text = self
                    .input_text(key, native.text.as_deref(), native.buffer_id.as_deref())?
                    .ok_or_else(|| error(-32602))?;
                let version = metadata_params::counter(&native.version)?;
                if version == 0 {
                    return Err(failure("invalid_editor_state"));
                }
                Ok(NativeSourceSnapshot {
                    version,
                    text: text.into(),
                    user_read_only: native.user_read_only,
                })
            })
            .transpose()?;
        let view = if let Some(owner) = owner {
            owner.core.source_snapshot(project)
        } else {
            project.source_snapshot(&path, native)
        }
        .map_err(snapshot_error)?;
        let buffer = input
            .native
            .as_ref()
            .and_then(|native| native.buffer_id.as_ref());
        if let Some(buffer) = buffer {
            let reserved = self
                .buffers
                .get(buffer)
                .ok_or_else(|| super::failure("unknown_buffer"))?
                .bytes;
            self.admit_change(reserved, view.bytes().len())?;
        } else {
            self.admit(view.bytes().len())?;
        }
        let id = self.next_id("s")?;
        let result = header(&id, &path, &view);
        if let Some(buffer) = buffer {
            self.buffers.remove(buffer);
        }
        self.source_snapshots.insert(
            id,
            Entry {
                key: Some(key.clone()),
                view,
            },
        );
        Ok(result)
    }
}

/// Both view namespaces retain exact binary snapshots but only metadata views carry project authority.
fn header(id: &str, path: &std::path::Path, view: &SourceSnapshot) -> Value {
    json!({"snapshotId":id,"path":dto::path(path),"bytes":view.bytes().len(),"snapshot":view.snapshot,
        "origin":match view.origin { SourceSnapshotOrigin::Disk=>"disk",SourceSnapshotOrigin::Editor=>"editor",SourceSnapshotOrigin::QuerySession=>"querySession" },
        "version":view.version.map(|version|version.to_string()),"protection":describe(&view.protection)})
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Native {
    version: String,
    user_read_only: bool,
    text: Option<String>,
    buffer_id: Option<String>,
}

/// The explicit support switch affects only native editing policy; query/source ownership is always enforced.
fn protection(project: &mut ProjectSession, args: &Value) -> Result<Value, Value> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Input {
        files: Vec<File>,
        enforce_support: bool,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct File {
        path: metadata_params::Path,
        user_read_only: bool,
    }
    let input: Input = metadata_params::decode(args)?;
    if input.files.is_empty() || input.files.len() > 128 {
        return Err(error(-32602));
    }
    let requests = input
        .files
        .into_iter()
        .map(|file| {
            Ok(SourceAccessRequest {
                path: file.path.native()?,
                user_read_only: file.user_read_only,
            })
        })
        .collect::<Result<Vec<_>, Value>>()?;
    let states = project
        .source_protection(&requests, input.enforce_support)
        .map_err(|failure| errors::workspace(&failure))?;
    Ok(json!({"files":states.iter().map(describe).collect::<Vec<_>>()}))
}

/// Core decisions remain independent from the wire's string counters and explicit path encoding.
fn describe(state: &SourceProtection) -> Value {
    json!({"path":dto::path(&state.support.path),"objects":state.support.objects,"supportReadOnly":state.support.read_only,
        "supportReason":state.support.reason,"supportUnknown":state.support.unknown,"mixed":state.support.mixed,
        "supportEnforced":state.support_enforced,"userReadOnly":state.user_read_only,"filesystemReadOnly":state.filesystem_read_only(),
        "exists":state.exists(),"writable":state.writable(),"diagnostics":state.diagnostics,"sharedReadOnly":state.shared_read_only,
        "restrictions":state.restrictions.iter().map(|owner|json!({"id":owner.id.to_string(),"source":dto::path(&owner.source)})).collect::<Vec<_>>()})
}

/// Source read failures never leak source bytes, absolute IO diagnostics or platform-specific error messages.
fn snapshot_error(error: SourceSnapshotError) -> Value {
    match error {
        SourceSnapshotError::Workspace(error) => errors::workspace(&error),
        SourceSnapshotError::UnknownSource => failure("unknown_source"),
        SourceSnapshotError::Missing => failure("missing"),
        SourceSnapshotError::TooLarge => domain(
            "resource_limit",
            json!({"resource":"source_snapshot","maxBytes":MAX_SOURCE_SNAPSHOT_BYTES}),
        ),
        SourceSnapshotError::InvalidEditorState => failure("invalid_editor_state"),
        SourceSnapshotError::Io { path, .. } => domain(
            "source_invalid",
            json!({"path":dto::path(&path),"reason":"io"}),
        ),
    }
}

/// Stable machine reasons are separate from every supported adapter's human messages.
fn failure(reason: &str) -> Value {
    domain("source_view_failed", json!({"reason":reason}))
}
