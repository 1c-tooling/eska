//! Shared presentation of property schemas and validated mutations for IDE and JSON CLI clients.

use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    dto::{self, Labels},
    envelope::domain,
    errors, params,
};
use crate::project::{
    metadata_edit::{EditError, FieldStep, PropertyChange, ScalarSchema},
    metadata_model::{ObjectId, PropertyKey},
    metadata_workspace::{ProjectSession, PropertyEditError, PropertyTypeChoice},
};

/// One machine-facing logical change, shared with the one-shot CLI request document.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::cli) struct ChangeRequest {
    pub object_id: ObjectId,
    pub snapshot: String,
    pub path: Vec<FieldStep>,
    pub change: PropertyChange,
}

/// Publish both locale captions while keeping enum values and paths independent of locale.
pub(in crate::cli) fn describe(
    labels: &Labels,
    project: &mut ProjectSession,
    id: &ObjectId,
) -> Result<Value, Value> {
    let state = project.property_editing(id).map_err(failure)?;
    let owner = project
        .object(id)
        .map_err(|error| errors::workspace(&error))?
        .kind;
    let fields: Vec<_> = state
        .properties
        .fields
        .iter()
        .map(|field| {
            let mut value = json!({"path":field.path,"value":field.value,"schema":field.schema,"language":field.language});
            value["captions"] = json!(
                field
                    .path
                    .iter()
                    .map(|step| paired(labels, |locale| locale
                        .property_caption(Some(owner), &step.key)
                        .unwrap_or_else(|| step.key.name.clone())))
                    .collect::<Vec<_>>()
            );
            if let ScalarSchema::Enum { domain, values } = &field.schema {
                value["options"] = json!(
                    values
                        .iter()
                        .map(|token| json!({"value":token,"caption":paired(labels,
                |locale| locale.enum_caption(domain, token).unwrap_or_else(|| token.clone()))}))
                        .collect::<Vec<_>>()
                );
            }
            if let ScalarSchema::DataType { key, .. } = &field.schema {
                value["caption"] = type_caption(labels, key);
            }
            value
        })
        .collect();
    Ok(
        json!({"snapshot":state.properties.snapshot,"source":dto::path(&state.path),"writable":state.writable,
        "readOnlyReason":if state.writable { Value::Null } else { json!("support_or_source_unavailable") },
        "profile":state.properties.profile,"readOnlyProperties":state.properties.read_only_properties,"fields":fields,"undo":state.undo,"redo":state.redo,"addRemove":false}),
    )
}

/// Return the backend's allowed semantic type identities with readable paired captions.
pub(in crate::cli) fn choices(
    labels: &Labels,
    project: &mut ProjectSession,
    id: &ObjectId,
    path: &[FieldStep],
) -> Result<Value, Value> {
    let choices = project.property_type_choices(id, path).map_err(failure)?;
    Ok(
        json!({"choices":choices.iter().map(|choice| type_choice(labels, choice)).collect::<Vec<_>>()}),
    )
}

/// Preview uses the mutation planner and support checks, with no source or cache publication.
pub(in crate::cli) fn preview(
    project: &mut ProjectSession,
    request: &ChangeRequest,
) -> Result<Value, Value> {
    let plan = project
        .preview_property(
            &request.object_id,
            &request.snapshot,
            &request.path,
            &request.change,
        )
        .map_err(failure)?;
    Ok(
        json!({"valid":true,"changed":!plan.is_empty(),"snapshot":request.snapshot,
        "changes":plan.replacements().iter().map(|replacement| json!({"range":{"start":replacement.range.start,"end":replacement.range.end},"replacement":replacement.text})).collect::<Vec<_>>()}),
    )
}

/// Dispatch read-only editing queries through the same domain methods used by mutation.
pub(super) fn query(
    labels: &Labels,
    project: &mut ProjectSession,
    method: &str,
    args: &Value,
) -> Result<Value, Value> {
    let id: ObjectId = params::decode(&args["objectId"])?;
    match method {
        "metadata/propertyEditing" => describe(labels, project, &id),
        "metadata/propertyTypeChoices" => choices(
            labels,
            project,
            &id,
            &params::decode::<Vec<FieldStep>>(&args["path"])?,
        ),
        "metadata/previewProperty" => preview(project, &params::decode(args)?),
        _ => Err(super::envelope::error(-32601)),
    }
}

/// Stable machine errors never expose source content or platform diagnostics as instructions.
pub(in crate::cli) fn failure(error: PropertyEditError) -> Value {
    let code = match error {
        PropertyEditError::Workspace(error) => return errors::workspace(&error),
        PropertyEditError::Committed(_) => "property_committed_refresh_required",
        PropertyEditError::Edit(EditError::Conflict) => "property_conflict",
        PropertyEditError::Edit(EditError::ReadOnly) => "property_read_only",
        PropertyEditError::Edit(EditError::InvalidValue) => "property_invalid",
        PropertyEditError::Edit(EditError::UnsupportedValue) => "property_unsupported",
        PropertyEditError::Edit(EditError::HistoryUnavailable) => "property_history_unavailable",
        PropertyEditError::Edit(EditError::UnsafePath) => "source_invalid",
        PropertyEditError::Edit(EditError::InvalidXml) => "xml_invalid",
        PropertyEditError::Edit(EditError::Io(_)) => "property_write_failed",
    };
    domain(code, json!({}))
}

/// Both caption variants are returned together so callers never localize enum identifiers.
fn paired(
    labels: &Labels,
    format: impl Fn(&crate::cli::localization::Localizer) -> String,
) -> Value {
    json!({"ru-RU":format(&labels.ru),"en-US":format(&labels.en)})
}

/// Primitive names use the platform dictionary; generated types retain user-provided names.
fn type_caption(labels: &Labels, key: &PropertyKey) -> Value {
    paired(labels, |locale| {
        locale.type_caption(key).unwrap_or_else(|| key.name.clone())
    })
}

/// Type choices show the owning metadata category and synonym without changing the stored key.
fn type_choice(labels: &Labels, choice: &PropertyTypeChoice) -> Value {
    let caption = choice.object.as_ref().map_or_else(
        || type_caption(labels, &choice.key),
        |object| {
            paired(labels, |locale| {
                let language = locale.locale().as_str();
                let synonym = object
                    .synonyms
                    .iter()
                    .find(|synonym| synonym.language == language[..2])
                    .map_or(object.name.as_str(), |synonym| synonym.content.as_str());
                format!(
                    "{} · {synonym}",
                    locale.text(&format!("platform-kind-{}", object.kind.as_str()))
                )
            })
        },
    );
    json!({"key":choice.key,"caption":caption,"metadataKind":choice.object.as_ref().map(|object| object.kind.as_str())})
}
