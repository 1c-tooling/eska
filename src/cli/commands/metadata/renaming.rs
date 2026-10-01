//! JSON rename commands share backend plans and never accept filesystem destinations.

use super::{ProjectSession, Value, failure, json};
use crate::cli::ide::renaming::failure as failure_rename;

/// Expose a read-only structural plan and its publication blockers.
pub(super) fn preview(project: &mut ProjectSession, request: &Value) -> Result<Value, Value> {
    let id = serde_json::from_value(request["objectId"].clone())
        .map_err(|_| failure("invalid_request"))?;
    let name = request["newName"]
        .as_str()
        .ok_or_else(|| failure("invalid_request"))?;
    let plan = project.preview_rename(&id, name).map_err(failure_rename)?;
    Ok(json!({"applyAvailable":plan.issues.is_empty(),"plan":plan}))
}

/// Applying requires the precise preview token and an explicit acknowledgement of uncertain matches.
pub(super) fn apply(project: &mut ProjectSession, request: &Value) -> Result<Value, Value> {
    let id = serde_json::from_value(request["objectId"].clone())
        .map_err(|_| failure("invalid_request"))?;
    let name = request["newName"]
        .as_str()
        .ok_or_else(|| failure("invalid_request"))?;
    let expected = request["snapshot"]
        .as_str()
        .ok_or_else(|| failure("invalid_request"))?;
    let reviewed = request["reviewedUncertain"]
        .as_bool()
        .ok_or_else(|| failure("invalid_request"))?;
    let plan = project
        .apply_rename(&id, name, expected, reviewed)
        .map_err(failure_rename)?;
    Ok(
        json!({"applied":true,"objectId":plan.new_object_id,"uuid":plan.uuid,"files":plan.files.iter().filter(|file| !file.replacements.is_empty()).map(|file| &file.path).collect::<Vec<_>>(),"moves":plan.moves}),
    )
}

/// Recovery uses a separate journal token and never reruns the original rename request.
pub(super) fn recovery(project: &mut ProjectSession, request: &Value) -> Result<Value, Value> {
    match request["action"].as_str() {
        Some("inspect") if request.get("snapshot").is_none() => {
            Ok(json!(project.rename_recovery().map_err(failure_rename)?))
        }
        Some("restore") => {
            let expected = request["snapshot"]
                .as_str()
                .ok_or_else(|| failure("invalid_request"))?;
            project.recover_rename(expected).map_err(failure_rename)?;
            Ok(json!({"recovered":true}))
        }
        _ => Err(failure("invalid_request")),
    }
}
