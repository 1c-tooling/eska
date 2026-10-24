//! Constructor ownership and text transformations live in ESKA; a native adapter displays only these DTOs.

use std::path::PathBuf;

use eska_project_core::project::{
    metadata_edit::snapshot,
    query::constructor::{Action, ClauseKind, Constructor},
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{State, dto, failure, params::Key, recovery, session_error, sessions};
use crate::ide::{envelope::error, params};

const MAX_EXPRESSION: usize = 16_384;
const MAX_MODEL_BYTES: usize = 32 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Id {
    query_session_id: String,
    constructor_id: String,
}

/// Older negotiated clients cannot acquire an exclusive draft lock they do not understand.
pub(super) fn is_method(method: &str) -> bool {
    matches!(
        method,
        "query/sessionComments"
            | "query/constructorOpen"
            | "query/constructorPlan"
            | "query/constructorModel"
            | "query/constructorAction"
            | "query/constructorFinish"
            | "query/constructorCancel"
    )
}

/// Read-only inspection and explicit release remain available after a metadata generation changes.
pub(super) fn is_cleanup(method: &str) -> bool {
    matches!(
        method,
        "query/constructorModel"
            | "query/constructorPlan"
            | "query/constructorCancel"
            | "query/sessionComments"
    )
}

/// Visual editing is explicit, lossless, bounded and fenced at the same canonical draft used by XML/BSL sessions.
pub(super) fn capability() -> Value {
    json!({"schemaVersion":1,"writesOwner":false,"writesDraft":true,"explicitFinish":true,"exclusiveDraft":true,
        "maxTokens":65536,"maxStatements":256,"maxStatementClauses":128,"maxStatementSeparators":2048,"lossless":true,"nativeDraftPlan":true,"sourceCommentPositions":true,"maxExpressionBytes":MAX_EXPRESSION,"maxModelBytes":MAX_MODEL_BYTES,"sharedQueryBudget":true,
        "clauses":["fields","sources","joins","conditions","group","order"]})
}

impl State {
    /// Stage every visual transition on a cheap immutable-text clone; failed quota or disk CAS never advances live state.
    pub(super) fn constructor_request(
        &mut self,
        key: &Key,
        method: &str,
        args: &Value,
        recovery: &mut recovery::State,
        forbidden: &[PathBuf],
    ) -> Result<Value, Value> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct SessionId {
            query_session_id: String,
        }
        let input: SessionId = params::decode(args)?;
        let entry = self
            .sessions
            .get(&input.query_session_id)
            .filter(|entry| entry.key.same_owner(key))
            .ok_or_else(|| failure("unknown_query_session"))?;
        if !is_cleanup(method) {
            recovery.checked_record(
                &entry.store_id,
                entry.stored.record.id(),
                Some(&entry.stored.revision),
                true,
            )?;
        }
        if method == "query/sessionComments" {
            return source_comments(&entry.core, args);
        }
        if method == "query/constructorPlan" {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Plan {
                query_session_id: String,
                constructor_id: String,
                constructor_revision: String,
                native_snapshot: String,
            }
            let input: Plan = params::decode(args)?;
            let plan = entry
                .core
                .plan_constructor(
                    params::counter(&input.constructor_id)?,
                    params::counter(&input.constructor_revision)?,
                    &input.native_snapshot,
                )
                .map_err(session_error)?;
            let mut output = sessions::header(&input.query_session_id, entry)?;
            output["draftPlan"] = dto::text_plan(&plan);
            return Ok(output);
        }
        if method == "query/constructorModel" {
            let input: Id = params::decode(args)?;
            let value = entry
                .core
                .constructor(params::counter(&input.constructor_id)?)
                .map_err(session_error)?;
            let mut output = sessions::header(&input.query_session_id, entry)?;
            output["constructor"] = model(
                value,
                &input.constructor_id,
                entry.core.constructor_writable(),
            )?;
            return Ok(output);
        }
        let mut core = entry.core.clone();
        let (id, plan, released) = transition(&mut core, method, args)?;
        self.admit_change(entry.core.retained_bytes(), core.retained_bytes())?;
        let view = id
            .as_deref()
            .map(|id| {
                model(
                    core.constructor(params::counter(id)?)
                        .map_err(session_error)?,
                    id,
                    core.constructor_writable(),
                )
            })
            .transpose()?;
        let stored = if method == "query/constructorFinish" {
            let project_key = entry.stored.record.metadata()["projectKey"]
                .as_str()
                .ok_or_else(|| failure("recovery_invalid"))?;
            recovery.publish(
                &entry.store_id,
                sessions::record(&core, entry.stored.record.id(), project_key)?,
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
        let mut output = sessions::header(&input.query_session_id, entry)?;
        output["constructor"] = view.unwrap_or(Value::Null);
        if let Some(plan) = plan {
            output["draftPlan"] = dto::text_plan(&plan);
        }
        if let Some(released) = released {
            output["released"] = json!(released);
        }
        Ok(output)
    }

    /// Generic checkpoint removal cannot destroy a draft that a visual constructor currently owns.
    pub(super) fn constructor_record_guard(&self, id: &str) -> Result<(), Value> {
        if self
            .sessions
            .values()
            .any(|entry| entry.stored.record.id() == id && entry.core.constructor_active())
        {
            Err(failure("constructor_busy"))
        } else {
            Ok(())
        }
    }
}

type Transition = (
    Option<String>,
    Option<eska_project_core::project::metadata_edit::EditPlan>,
    Option<bool>,
);

/// Tokens identify only this source session; native snapshot guards protect the final canonical-draft handoff.
fn transition(
    core: &mut eska_project_core::project::metadata_workspace::QuerySession,
    method: &str,
    args: &Value,
) -> Result<Transition, Value> {
    #[derive(Deserialize)]
    struct Revision {
        revision: String,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Operation {
        constructor_revision: String,
        action: Action,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Finish {
        constructor_revision: String,
        native_snapshot: String,
        result_snapshot: Option<String>,
    }
    if method == "query/constructorOpen" {
        let input: Revision = params::decode(args)?;
        let id = core
            .open_constructor(params::counter(&input.revision)?)
            .map_err(session_error)?;
        return Ok((Some(id.to_string()), None, None));
    }
    let input: Id = params::decode(args)?;
    let id = params::counter(&input.constructor_id)?;
    match method {
        "query/constructorAction" => {
            let input: Operation = params::decode(args)?;
            let text = match &input.action {
                Action::Append { text, .. } | Action::Replace { text, .. } => Some(text),
                _ => None,
            };
            if text.is_some_and(|text| text.len() > MAX_EXPRESSION) {
                return Err(failure("constructor_limit"));
            }
            core.constructor_action(
                id,
                params::counter(&input.constructor_revision)?,
                &input.action,
            )
            .map_err(session_error)?;
            Ok((Some(id.to_string()), None, None))
        }
        "query/constructorFinish" => {
            let input: Finish = params::decode(args)?;
            if input.result_snapshot.as_ref().is_some_and(|snapshot| {
                core.constructor(id)
                    .is_ok_and(|value| *snapshot != value.snapshot())
            }) {
                return Err(failure("stale_draft"));
            }
            let plan = core
                .finish_constructor(
                    id,
                    params::counter(&input.constructor_revision)?,
                    &input.native_snapshot,
                    true,
                )
                .map_err(session_error)?;
            Ok((None, plan, None))
        }
        "query/constructorCancel" => {
            let released = core.constructor_active();
            if released {
                core.cancel_constructor(id).map_err(session_error)?;
            }
            Ok((None, None, Some(released)))
        }
        _ => Err(error(-32601)),
    }
}

/// DTO text is the exact working draft; long expressions remain readable but cannot be replaced through a short input control.
fn model(value: &Constructor, id: &str, writable: bool) -> Result<Value, Value> {
    let input = value.text();
    let statements: Vec<_> = value.model().statements.iter().enumerate().map(|(index, statement)| {
        let clauses: Vec<_> = statement.clauses.iter().enumerate().map(|(index, clause)| {
            let items: Vec<_> = clause.items.iter().enumerate().map(|(index, item)| {
                let expression = item.expression(input);
                let too_large = expression.len() > MAX_EXPRESSION;
                json!({"index":index,"range":dto::bytes(&item.range),"preview":preview(&input[item.range.clone()]),
                    "expression":if too_large {None} else {Some(expression)},"comments":item.comments.len(),
                    "removable":writable && (clause.items.len()>1 || !matches!(clause.kind,ClauseKind::Fields|ClauseKind::Sources)),
                    "movable":writable && clause.kind!=ClauseKind::Joins && clause.items.len()>1,
                    "unavailable":{
                        "replace":if !writable {Some("read_only")} else if too_large {Some("expression_too_large")} else {None},
                        "remove":if !writable {Some("read_only")} else if clause.items.len()==1 && matches!(clause.kind,ClauseKind::Fields|ClauseKind::Sources) {Some("required_item")} else {None},
                        "move":if !writable {Some("read_only")} else if clause.kind==ClauseKind::Joins {Some("join_order_dependency")} else if clause.items.len()<2 {Some("single_item")} else {None}
                    }})
            }).collect();
            json!({"index":index,"kind":clause.kind,"header":preview(input[clause.range.start..clause.body.start].trim()),"items":items})
        }).collect();
        let append: Vec<_> = [ClauseKind::Fields, ClauseKind::Sources, ClauseKind::Joins, ClauseKind::Conditions, ClauseKind::Group, ClauseKind::Order]
            .into_iter().map(|kind| json!({"kind":kind,"unavailable":if writable {json!(statement.append_unavailable(kind))} else {json!("read_only")}})).collect();
        json!({"index":index,"range":dto::bytes(&statement.range),"unavailable":statement.unavailable,"preview":preview(&input[statement.range.clone()]),"clauses":clauses,"append":append})
    }).collect();
    let output = json!({"id":id,"revision":value.revision().to_string(),"snapshot":snapshot(input),"text":input,"bytes":input.len(),"writable":writable,"statements":statements});
    if serde_json::to_vec(&output)
        .map_err(|_| error(-32603))?
        .len()
        > MAX_MODEL_BYTES
    {
        return Err(failure("constructor_limit"));
    }
    Ok(output)
}

/// Preview clipping uses Unicode scalar boundaries and never changes the canonical text or operation ranges.
fn preview(input: &str) -> String {
    input.chars().take(256).collect()
}

/// Native UTF-16 destinations become core byte anchors, so another IDE need not implement query/source coordinate rules.
fn source_comments(
    core: &eska_project_core::project::metadata_workspace::QuerySession,
    args: &Value,
) -> Result<Value, Value> {
    #[derive(Deserialize)]
    struct Comments {
        revision: String,
        positions: Vec<super::params::Point>,
    }
    let input: Comments = params::decode(args)?;
    if input.positions.len() > 64 {
        return Err(error(-32602));
    }
    let (source, needs_decision) = core
        .source_comment_state(params::counter(&input.revision)?)
        .map_err(session_error)?;
    let comments: Vec<_> = source.comments.iter().enumerate().map(|(index, comment)| json!({"comment":index,"text":comment.text,
        "originalQueryOffset":comment.query_offset,"originalQueryPosition":dto::point(&source.text,comment.query_offset)})).collect();
    let line_starts_only = matches!(
        core.identity(),
        eska_project_core::project::query::QueryIdentity::BslLiteral(_)
    );
    let positions: Vec<_> = input
        .positions
        .into_iter()
        .map(|point| {
            let offset =
                eska_project_core::project::query::position_byte(core.draft(), point.core())
                    .filter(|offset| {
                        !line_starts_only
                            || (*offset > 0 && core.draft()[..*offset].ends_with('\n'))
                    });
            json!({"position":{"line":point.line,"character":point.character},"queryOffset":offset})
        })
        .collect();
    Ok(
        json!({"revision":core.revision().to_string(),"snapshot":snapshot(core.draft()),"needsDecision":needs_decision,
        "lineStartsOnly":line_starts_only,
        "comments":comments,"positions":positions}),
    )
}
