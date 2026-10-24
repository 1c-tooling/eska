//! Language projections are immutable views; the metadata server never sends LSP or writes a native buffer.

use base64::{Engine, engine::general_purpose::STANDARD};
use eska_project_core::project::{
    metadata_edit::{Replacement, snapshot},
    metadata_workspace::{ProjectSession, QueryProjection, QueryProjectionTarget},
    query::{Affinity, Position, position_byte},
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{INLINE_BYTES, State, dto, failure, params::Key, recovery, session_error, sessions};
use crate::ide::{dto as metadata_dto, envelope::error, params};

pub(super) struct Entry {
    key: Key,
    view: QueryProjection,
}

impl Entry {
    /// Mapping storage and retained original text remain charged after the source session releases its lease.
    pub(super) fn bytes(&self) -> usize {
        self.view.retained_bytes()
    }
}

/// Clients must explicitly negotiate the BSL-only language projection contract.
pub(super) fn capability() -> Value {
    json!({"schemaVersion":1,"writesOwner":false,"writesDraft":false,"sourceKinds":["bsl"],
        "maxProjections":16,"maxSnapshotBytes":eska_project_core::project::query::MAX_SNAPSHOT_BYTES,
        "maxChunkBytes":INLINE_BYTES,"maxPositions":64,"maxEdits":64,"sharedQueryBudget":true})
}

/// Read-only projection operations have a separate namespace from recovery and protected-source snapshots.
pub(super) fn is_method(method: &str) -> bool {
    matches!(
        method,
        "query/projectionOpen"
            | "query/projectionRead"
            | "query/projectionMap"
            | "query/projectionEdits"
            | "query/projectionClose"
    )
}

impl State {
    /// Capture a current checked draft without changing its checkpoint, native text, revision or apply phase.
    pub(super) fn open_projection(
        &mut self,
        project: &mut ProjectSession,
        key: &Key,
        args: &Value,
        recovery: &recovery::State,
    ) -> Result<Value, Value> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Open {
            query_session_id: String,
            revision: String,
            owner: sessions::Owner,
        }
        let input: Open = params::decode(args)?;
        if self.projections.len() >= 16 {
            return Err(failure("projection_limit"));
        }
        let entry = self
            .sessions
            .get(&input.query_session_id)
            .filter(|entry| entry.key.same_owner(key))
            .ok_or_else(|| failure("unknown_query_session"))?;
        recovery.checked_record(
            &entry.store_id,
            entry.stored.record.id(),
            Some(&entry.stored.revision),
            true,
        )?;
        let view = entry
            .core
            .language_projection(
                project,
                params::counter(&input.revision)?,
                input.owner.core()?,
            )
            .map_err(session_error)?;
        self.admit(view.retained_bytes())?;
        let mut output = json!({"source":metadata_dto::path(entry.core.document().path()),"sourceKind":"bsl",
            "revision":view.revision().to_string(),"snapshot":view.snapshot(),"bytes":view.owner().len(),
            "draftSnapshot":snapshot(view.draft()),"draftBytes":view.draft().len(),"readOnly":true,
            "nativeVersion":entry.core.document().version().to_string(),"nativeSnapshot":entry.core.document().snapshot()});
        let id = self.next_id("l")?;
        output["projectionId"] = json!(id);
        self.projections.insert(
            id,
            Entry {
                key: key.clone(),
                view,
            },
        );
        Ok(output)
    }

    /// Immutable reads/maps can finish after a metadata generation changes, but never cross member or process identity.
    pub(super) fn projection_read(
        &mut self,
        key: &Key,
        method: &str,
        args: &Value,
    ) -> Result<Value, Value> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Id {
            projection_id: String,
        }
        #[derive(Deserialize)]
        struct Guard {
            snapshot: String,
        }
        let input: Id = params::decode(args)?;
        let entry = self.projections.get(&input.projection_id);
        if entry.is_some_and(|entry| !entry.key.same_owner(key)) {
            return Err(failure("unknown_projection"));
        }
        if method == "query/projectionClose" {
            return Ok(json!({"released":self.projections.remove(&input.projection_id).is_some()}));
        }
        let view = &entry.ok_or_else(|| failure("unknown_projection"))?.view;
        if params::decode::<Guard>(args)?.snapshot != view.snapshot() {
            return Err(failure("projection_snapshot_mismatch"));
        }
        match method {
            "query/projectionRead" => read(view, &input.projection_id, args),
            "query/projectionMap" => map(view, args),
            "query/projectionEdits" => edits(view, args),
            _ => Err(error(-32601)),
        }
    }
}

/// Byte chunks use canonical base64, so a UTF-8 scalar crossing a frame boundary is never replaced or rounded.
fn read(view: &QueryProjection, id: &str, args: &Value) -> Result<Value, Value> {
    #[derive(Deserialize)]
    struct Read {
        offset: usize,
        bytes: usize,
    }
    let input: Read = params::decode(args)?;
    let body = view.owner().as_bytes();
    if input.offset > body.len() || input.bytes == 0 || input.bytes > INLINE_BYTES {
        return Err(error(-32602));
    }
    let end = input.offset.saturating_add(input.bytes).min(body.len());
    Ok(
        json!({"projectionId":id,"snapshot":view.snapshot(),"offset":input.offset,
        "nextOffset":end,"eof":end==body.len(),"data":STANDARD.encode(&body[input.offset..end])}),
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum Direction {
    DraftToOwner,
    OwnerToDraft,
    OwnerTarget,
}

/// The core owns all UTF-16 and escape translation; adapters cannot infer coordinates from a filename or replacement delta.
fn map(view: &QueryProjection, args: &Value) -> Result<Value, Value> {
    #[derive(Deserialize)]
    struct Map {
        direction: Direction,
        positions: Vec<super::params::Point>,
        affinity: Option<super::params::Affinity>,
    }
    let input: Map = params::decode(args)?;
    if input.positions.len() > 64
        || (!matches!(input.direction, Direction::DraftToOwner) && input.affinity.is_some())
    {
        return Err(error(-32602));
    }
    let affinity = match input.affinity {
        Some(super::params::Affinity::Before) => Affinity::Before,
        _ => Affinity::After,
    };
    let positions: Vec<_> = input
        .positions
        .into_iter()
        .map(|point| {
            let target = match input.direction {
                Direction::DraftToOwner => view
                    .owner_position(point.core(), affinity)
                    .map(QueryProjectionTarget::Owner),
                Direction::OwnerToDraft => view
                    .draft_position(point.core())
                    .map(QueryProjectionTarget::Draft),
                Direction::OwnerTarget => view.target(point.core()),
            };
            match target {
                Some(QueryProjectionTarget::Draft(point)) => {
                    json!({"target":"draft","position":position(point)})
                }
                Some(QueryProjectionTarget::Owner(point)) => {
                    json!({"target":"owner","position":position(point)})
                }
                None => Value::Null,
            }
        })
        .collect();
    Ok(json!({"positions":positions}))
}

/// Language edits produce a draft plan only after decoding and checking the entire edit set in the original BSL context.
fn edits(view: &QueryProjection, args: &Value) -> Result<Value, Value> {
    #[derive(Deserialize)]
    struct Range {
        start: super::params::Point,
        end: super::params::Point,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Edit {
        range: Range,
        new_text: String,
    }
    #[derive(Deserialize)]
    struct Edits {
        edits: Vec<Edit>,
    }
    let input: Edits = params::decode(args)?;
    if input.edits.len() > 64 {
        return Err(error(-32602));
    }
    let replacements = input
        .edits
        .into_iter()
        .map(|edit| {
            let start = position_byte(view.owner(), edit.range.start.core())
                .ok_or_else(|| failure("invalid_position"))?;
            let end = position_byte(view.owner(), edit.range.end.core())
                .ok_or_else(|| failure("invalid_position"))?;
            Ok(Replacement {
                range: start..end,
                text: edit.new_text,
            })
        })
        .collect::<Result<Vec<_>, Value>>()?;
    let plan = view
        .translate_edits(replacements)
        .map_err(super::codec_error)?;
    Ok(
        json!({"noOp":plan.is_empty(),"inputSnapshot":snapshot(plan.original()),"outputSnapshot":snapshot(plan.output()),
        "replacements":plan.replacements().iter().map(|edit| json!({"range":dto::bytes(&edit.range),
            "rangeUtf16":{"start":dto::point(plan.original(),edit.range.start),"end":dto::point(plan.original(),edit.range.end)},
            "text":edit.text})).collect::<Vec<_>>()}),
    )
}

/// Position schema matches existing query positions and stays independent of editor libraries and locale.
fn position(point: Position) -> Value {
    json!({"line":point.line,"character":point.character})
}
