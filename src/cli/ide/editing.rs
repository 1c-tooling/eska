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
    pub context_snapshot: Option<String>,
    pub path: Vec<FieldStep>,
    pub change: PropertyChange,
}

/// Older clients retain only editor variants and write scopes negotiated by their API minor.
pub(super) fn retain_legacy_fields(schema: &mut Value, minor: u32) {
    let mut hidden = Vec::new();
    if let Some(fields) = schema["fields"].as_array_mut() {
        fields.retain(|field| {
            let keep = (minor >= 10 || field["linked"] != true)
                && (minor >= 11 || field["schema"]["kind"] != "unsignedInteger")
                && (minor >= 8
                    || matches!(
                        field["schema"]["kind"].as_str(),
                        Some("text" | "boolean" | "integer" | "decimal" | "enum" | "dataType")
                    )
                    || (minor >= 7 && field["schema"]["kind"] == "reference"));
            if !keep {
                hidden.push(field["path"][0]["key"].clone());
            }
            keep
        });
        hidden.retain(|key| !fields.iter().any(|field| &field["path"][0]["key"] == key));
    }
    if let Some(properties) = schema["readOnlyProperties"].as_array_mut() {
        for key in hidden {
            if !properties.iter().any(|property| property["key"] == key) {
                properties.push(json!({"key":key,"reason":"client_version"}));
            }
        }
    }
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
    let rename_available = state.writable && project.rename_available(id);
    let mut presenter = super::property_presentation::Presenter::new(labels, project, owner);
    let fields: Vec<_> = state
        .properties
        .fields
        .iter()
        .map(|field| {
            let mut value = json!({"path":field.path,"value":field.value,"schema":field.schema,"language":field.language});
            if crate::project::metadata_edit::numbering::linked_field(owner, &field.path) {
                value["linked"] = json!(true);
            }
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
            if matches!(field.schema, ScalarSchema::Reference { .. }) {
                value["caption"] = paired(labels, |locale| reference_caption(locale, &field.value));
            }
            if let ScalarSchema::Value { key, types, .. } = &field.schema {
                value["schema"]["types"] = json!(types.iter().map(|choice| {
                    let mut choice_json = json!(choice);
                    choice_json["caption"] = type_caption(labels, &choice.key);
                    if matches!(choice.constraints, crate::project::metadata_edit::ValueConstraints::Boolean) {
                        choice_json["options"] = json!(["true", "false"].map(|token| json!({"value":token,"caption":paired(labels, |locale| locale.text(&format!("platform-presentation-boolean-{token}")))})));
                    }
                    choice_json
                }).collect::<Vec<_>>());
                value["caption"] = paired(labels, |locale| if key.is_none() {
                    locale.text("platform-presentation-unset")
                } else if key.as_ref().is_some_and(|key| key.name == "boolean") {
                    locale.text(if matches!(field.value.as_str(), "true" | "1") { "platform-presentation-boolean-true" } else { "platform-presentation-boolean-false" })
                } else if field.value.is_empty() { locale.text("platform-presentation-empty-string")
                } else if field.value.ends_with(".EmptyRef") { locale.text("platform-presentation-empty-reference")
                } else { field.value.clone() });
                if key.as_ref().is_some_and(|key| crate::project::metadata_edit::value_schema::reference_prefix(key).is_some())
                    && let Some(reference) = presenter.design_reference(&field.value)
                {
                    value["caption"] = reference["caption"].clone();
                }
            }
            value
        })
        .collect();
    Ok(
        json!({"snapshot":state.properties.snapshot,"source":dto::path(&state.path),"writable":state.writable,
        "readOnlyReason":if state.writable { Value::Null } else { json!("support_or_source_unavailable") },
        "contextSnapshot":state.context_snapshot,"linkedObjects":state.linked_objects,
        "undoLinked":state.undo == Some(crate::project::metadata_workspace::HistoryOperation::Linked),"redoLinked":state.redo == Some(crate::project::metadata_workspace::HistoryOperation::Linked),
        "profile":state.properties.profile,"readOnlyProperties":state.properties.read_only_properties,"fields":fields,"undo":state.undo.is_some(),"redo":state.redo.is_some(),"addRemove":false,
        "renameAvailable":rename_available,"undoRename":state.undo == Some(crate::project::metadata_workspace::HistoryOperation::Rename),"redoRename":state.redo == Some(crate::project::metadata_workspace::HistoryOperation::Rename)}),
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

/// Publish only valid declared targets; an empty scalar is distinct from removing a list entry.
pub(in crate::cli) fn reference_choices(
    labels: &Labels,
    project: &mut ProjectSession,
    id: &ObjectId,
    path: &[FieldStep],
) -> Result<Value, Value> {
    let choices = project
        .property_reference_choices(id, path)
        .map_err(failure)?;
    Ok(json!({"choices":choices.iter().map(|choice| {
        let mut value = json!({
            "value":choice.value,"objectId":choice.object.id,
            "caption":paired(labels, |locale| reference_caption(locale, &choice.value)),
            "metadataKind":choice.object.kind.as_str()
        });
        if let Some(name) = &choice.standard_attribute { value["standardAttribute"] = json!(name); }
        value
    }).collect::<Vec<_>>()}))
}

/// A typed value's reference domain includes empty references and existing enum/predefined values.
pub(in crate::cli) fn value_choices(
    labels: &Labels,
    project: &mut ProjectSession,
    id: &ObjectId,
    path: &[FieldStep],
    key: &PropertyKey,
) -> Result<Value, Value> {
    let choices = project
        .property_value_choices(id, path, key)
        .map_err(failure)?;
    Ok(
        json!({"choices":choices.iter().map(|choice| json!({"value":choice.value,
        "caption":paired(labels, |locale| choice.object.as_ref().map_or_else(||locale.text("platform-presentation-empty-reference"), |object| super::property_presentation::reference_name(object, locale.locale().as_str())))
    })).collect::<Vec<_>>()}),
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
            request.context_snapshot.as_deref(),
            &request.path,
            &request.change,
        )
        .map_err(failure)?;
    let changes = |replacements: &[crate::project::metadata_edit::Replacement]| {
        replacements.iter().map(|replacement| json!({"range":{"start":replacement.range.start,"end":replacement.range.end},"replacement":replacement.text})).collect::<Vec<_>>()
    };
    Ok(
        json!({"valid":true,"changed":!plan.is_empty(),"snapshot":request.snapshot,
        "changes":changes(plan.replacements()),
        "files":plan.files().filter(|file| !file.plan.is_empty()).map(|file| json!({
            "objectId":file.object_id,"source":dto::path(&file.path),
            "snapshot":crate::project::metadata_edit::snapshot(file.plan.original()),
            "changes":changes(file.plan.replacements())
        })).collect::<Vec<_>>() }),
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
        "metadata/propertyReferenceChoices" => reference_choices(
            labels,
            project,
            &id,
            &params::decode::<Vec<FieldStep>>(&args["path"])?,
        ),
        "metadata/propertyValueChoices" => value_choices(
            labels,
            project,
            &id,
            &params::decode::<Vec<FieldStep>>(&args["path"])?,
            &params::decode(&args["key"])?,
        ),
        "metadata/previewProperty" => preview(project, &params::decode(args)?),
        _ => Err(super::envelope::error(-32601)),
    }
}

/// Stable machine errors never expose source content or platform diagnostics as instructions.
pub(in crate::cli) fn failure(error: PropertyEditError) -> Value {
    let code = match error {
        PropertyEditError::Related {
            object_id,
            name,
            error,
        } => {
            let mut result = failure(PropertyEditError::Edit(error));
            result["data"]["details"]["objectId"] = json!(object_id);
            result["data"]["details"]["objectName"] = json!(name);
            return result;
        }
        PropertyEditError::Workspace(error) => return errors::workspace(&error),
        PropertyEditError::Committed(_) => "property_committed_refresh_required",
        PropertyEditError::Edit(EditError::ContextRequired) => "property_context_required",
        PropertyEditError::Edit(EditError::Conflict) => "property_conflict",
        PropertyEditError::Edit(EditError::ReadOnly) => "property_read_only",
        PropertyEditError::Edit(EditError::InvalidValue) => "property_invalid",
        PropertyEditError::Edit(EditError::IncompatibleProperty(property)) => {
            return domain(
                "property_dependency",
                json!({"property":property,"reason":"incompatible_type"}),
            );
        }
        PropertyEditError::Edit(EditError::UnsupportedValue) => "property_unsupported",
        PropertyEditError::Edit(EditError::HistoryUnavailable) => "property_history_unavailable",
        PropertyEditError::Edit(EditError::Busy) => "property_edit_busy",
        PropertyEditError::Edit(EditError::RecoveryRequired) => "property_recovery_required",
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

/// Keep field scope visible and localize implicit standard names without inventing tree identities.
fn reference_caption(locale: &crate::cli::localization::Localizer, raw: &str) -> String {
    if raw.is_empty() {
        return locale.text("platform-presentation-unset");
    }
    if let Some((owner, name)) = raw.rsplit_once(".StandardAttribute.") {
        let field = PropertyKey {
            namespace: Some("http://v8.1c.ru/8.3/xcf/readable".into()),
            name: "StandardAttribute".into(),
        };
        let qualifier = PropertyKey {
            namespace: None,
            name: "name".into(),
        };
        let caption = locale
            .qualifier_caption(&field, &qualifier, name)
            .unwrap_or_else(|| name.to_owned());
        return format!("{} › {caption}", reference_caption(locale, owner));
    }
    crate::project::metadata_edit::references::parts(raw).map_or_else(
        || raw.to_owned(),
        |parts| {
            parts
                .iter()
                .map(|(kind, name)| {
                    format!(
                        "{} · {name}",
                        locale.text(&format!("platform-kind-{}", kind.as_str()))
                    )
                })
                .collect::<Vec<_>>()
                .join(" › ")
        },
    )
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
