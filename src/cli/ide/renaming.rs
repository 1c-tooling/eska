//! Structural IDE operations share CLI plans and never accept client-supplied source edits.

use super::{
    dto, editing,
    envelope::{domain, error},
    metadata, params,
    server::{ProjectState, Server},
};
use crate::project::{
    metadata_model::ObjectId,
    metadata_workspace::{ProjectSession, PropertyEditError, RenameError},
};
use serde_json::{Value, json};

/// The same stable failure vocabulary serves JSON CLI and the framed IDE protocol.
pub(in crate::cli) fn failure(error: RenameError) -> Value {
    match error {
        RenameError::Workspace(error) => editing::failure(PropertyEditError::Workspace(error)),
        RenameError::Edit(error) => editing::failure(PropertyEditError::Edit(error)),
        RenameError::Name(reason) => domain("rename_invalid_name", json!({"reason":reason})),
        RenameError::Collision(id) => domain("rename_collision", json!({"objectId":id})),
        RenameError::Io { path, .. } => domain("rename_source_unavailable", json!({"path":path})),
        RenameError::Blocked(issues) => domain("rename_blocked", json!({"issues":issues})),
        RenameError::ReviewRequired => domain("rename_review_required", json!({})),
        RenameError::Committed(_) => domain("rename_committed_refresh_required", json!({})),
    }
}

/// Preview returns the same UTF-8 source-relative plan as the standalone JSON CLI.
pub(super) fn preview(project: &mut ProjectSession, args: &Value) -> Result<Value, Value> {
    let id = params::decode(&args["objectId"])?;
    let name = args["newName"].as_str().ok_or_else(|| error(-32602))?;
    let plan = project.preview_rename(&id, name).map_err(failure)?;
    Ok(json!({"applyAvailable":plan.issues.is_empty(),"plan":plan}))
}

/// Publish a reviewed plan, notify every tab of the identity change and return the fresh property sheet.
pub(super) fn apply(
    labels: &dto::Labels,
    session: &str,
    state: &mut ProjectState,
    project: &mut ProjectSession,
    args: &Value,
    events: &mut Vec<Value>,
) -> Result<Value, Value> {
    if state.event == u64::MAX {
        return Err(domain("generation_exhausted", json!({})));
    }
    let id: ObjectId = params::decode(&args["objectId"])?;
    let name = args["newName"].as_str().ok_or_else(|| error(-32602))?;
    let expected = args["snapshot"].as_str().ok_or_else(|| error(-32602))?;
    let reviewed = args["reviewedUncertain"]
        .as_bool()
        .ok_or_else(|| error(-32602))?;
    let generation = project.generation();
    let result = project.apply_rename(&id, name, expected, reviewed);
    if project.generation() != generation {
        state.refresh = result.is_err();
        metadata::changed(session, state, project, Value::Null, events)?;
        if let Ok(plan) = &result {
            annotate_event(events, &id, &plan.new_object_id);
        }
        Server::progress_event(session, state, project, events);
    }
    let plan = result.map_err(failure)?;
    let mut result = metadata::edited_properties(labels, project, &plan.new_object_id)?;
    result["renamed"] = transition(&id, &plan.new_object_id);
    Ok(result)
}

/// Clients rekey open object tabs by segment boundary, including descendants of a renamed owner.
pub(super) fn transition(from: &ObjectId, to: &ObjectId) -> Value {
    json!({"from":from,"to":to,"descendantFrom":format!("{from}/"),"descendantTo":format!("{to}/")})
}

/// Mutation notifications precede their response, so identity migration accompanies invalidation itself.
pub(super) fn annotate_event(events: &mut [Value], from: &ObjectId, to: &ObjectId) {
    if from != to
        && let Some(event) = events
            .iter_mut()
            .rev()
            .find(|event| event["method"] == "metadata/changed")
    {
        event["params"]["renamed"] = transition(from, to);
    }
}
