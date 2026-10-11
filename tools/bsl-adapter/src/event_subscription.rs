//! Event subscriptions retain ordered source `QNames` and current callback bindings.

#[cfg(test)]
mod tests;

use bsl_metadata::EventSubscription;
use eska::project_core::{
    MetadataKind,
    metadata_model::MetadataValue,
    metadata_parser::{LocatedProperty, ParsedObject},
};

use crate::{AdapterError, AdapterIssue, properties::Properties};

const CORE: &str = "http://v8.1c.ru/8.1/data/core";
const CONFIG: &str = "http://v8.1c.ru/8.1/data/enterprise/current-config";

/// Project the native subscription model from shared properties and resolved namespaces.
///
/// # Errors
/// Returns a wrong kind, invalid UUID, missing property capture, duplicate fields or malformed values.
pub fn event_subscription(object: &ParsedObject) -> Result<EventSubscription, AdapterError> {
    if object.metadata.kind() != MetadataKind::EventSubscription {
        return Err(AdapterError {
            issue: AdapterIssue::WrongKind {
                actual: object.metadata.kind(),
            },
            range: object.range.clone(),
        });
    }
    let fields = Properties::new(object)?;
    let source = fields
        .field("Source")?
        .map_or_else(|| Ok(String::new()), source_types)?;
    Ok(EventSubscription::from_parts(
        crate::identity::uuid(object)?,
        object.metadata.name().into(),
        fields
            .text("Comment")?
            .filter(|text| !text.is_empty())
            .map(str::to_owned),
        source,
        fields.text("Event")?.unwrap_or_default().into(),
        fields.text("Handler")?.unwrap_or_default().into(),
    ))
}

/// Source spelling is canonicalized once; alias prefixes cannot change a subscription's identity.
fn source_types(field: &LocatedProperty) -> Result<String, AdapterError> {
    let entries = match &field.property.value {
        MetadataValue::Record(entries) => entries,
        MetadataValue::Text(text) if text.is_empty() => return Ok(String::new()),
        _ => return Err(Properties::invalid(field)),
    };
    let mut names = Vec::with_capacity(entries.len());
    for entry in entries {
        if entry.key.namespace.as_deref() != Some(CORE)
            || !matches!(entry.key.name.as_str(), "Type" | "TypeSet")
        {
            return Err(AdapterError {
                issue: AdapterIssue::UnsupportedTypeField {
                    key: entry.key.clone(),
                },
                range: field.range.clone(),
            });
        }
        match &entry.value {
            MetadataValue::QualifiedText { key, .. }
                if key.namespace.as_deref() == Some(CONFIG) =>
            {
                names.push(format!("cfg:{}", key.name));
            }
            MetadataValue::QualifiedText { key, .. } => {
                return Err(AdapterError {
                    issue: AdapterIssue::UnsupportedType { key: key.clone() },
                    range: field.range.clone(),
                });
            }
            MetadataValue::Text(text) if text.is_empty() => {}
            MetadataValue::Text(text) => {
                return Err(AdapterError {
                    issue: AdapterIssue::UnresolvedTypeName { text: text.clone() },
                    range: field.range.clone(),
                });
            }
            _ => return Err(Properties::invalid(field)),
        }
    }
    Ok(names.join(";"))
}
