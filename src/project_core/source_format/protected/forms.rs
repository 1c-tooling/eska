//! Accepted metadata ownership grants property/layout edits, never stencil or UI identity replacement.

use std::collections::BTreeMap;

use serde_json::Value;

#[cfg(test)]
mod tests;

use super::super::{
    AcceptedBase, Error, FormPropertyName, FormPropertyValue, ManagedFormSource, NativeUuid,
    Operation,
};

/// Restore unaccepted fields independently while validating every selected property/layout change.
pub(super) fn select(
    base: &AcceptedBase,
    uuid: &NativeUuid,
    form: &mut ManagedFormSource,
    current: Option<&Value>,
) -> Result<(), Error> {
    if !base.permission(uuid, Operation::Resource).allowed {
        return Ok(());
    }
    let Some(current) = current else {
        return Ok(());
    };
    let accepted = form.clone();
    properties(&mut form.properties, current.get("properties"))?;
    for (id, definition) in &mut form.elements {
        properties(
            &mut definition.properties,
            current
                .get("elements")
                .and_then(|elements| elements.get(id.as_str()))
                .and_then(|element| element.get("properties")),
        )
        .map_err(|error| error.in_field(id.as_str()).in_field("elements"))?;
    }
    if let Some(layout) = current.get("layout") {
        form.layout = serde_json::from_value(layout.clone())
            .map_err(|_| Error::root("source_form_schema_invalid").in_field("layout"))?;
    }
    form.validate_change(&accepted)
}

/// Only existing property slots, language keys and formatted flags can supply working values.
fn properties(
    selected: &mut BTreeMap<FormPropertyName, FormPropertyValue>,
    current: Option<&Value>,
) -> Result<(), Error> {
    for (name, value) in selected {
        let Some(candidate) = current.and_then(|properties| properties.get(name.as_str())) else {
            continue;
        };
        match value {
            FormPropertyValue::Bool(flag) => {
                *flag = candidate.as_bool().ok_or_else(|| invalid(*name))?;
            }
            FormPropertyValue::Localized(localized) => {
                for (language, text) in &mut localized.text {
                    if let Some(candidate) =
                        candidate.get("text").and_then(|text| text.get(language))
                    {
                        candidate
                            .as_str()
                            .ok_or_else(|| invalid(*name))?
                            .clone_into(text);
                    }
                }
                if let Some(formatted) = &mut localized.formatted
                    && let Some(candidate) = candidate.get("formatted")
                {
                    *formatted = candidate.as_bool().ok_or_else(|| invalid(*name))?;
                }
            }
        }
    }
    Ok(())
}

/// Invalid permitted values retain their concrete source property instead of becoming defaults.
fn invalid(name: FormPropertyName) -> Error {
    Error::root("source_form_schema_invalid")
        .in_field(name.as_str())
        .in_field("properties")
}
