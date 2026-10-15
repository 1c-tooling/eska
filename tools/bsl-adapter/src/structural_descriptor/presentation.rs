//! Reviewed presentation leaves validate native field shapes without loading image resources.

use std::collections::BTreeSet;

use eska::project_core::{
    MetadataKind,
    metadata_model::{MetadataProperty, MetadataValue},
    metadata_parser::{LocatedProperty, ParsedObject},
    source_format::{MD_NAMESPACE, MetadataReference},
};

use super::enum_value;
use crate::{AdapterError, properties::Properties};

const CORE: &str = "http://v8.1c.ru/8.1/data/core";
const READABLE: &str = "http://v8.1c.ru/8.3/xcf/readable";

/// The reviewed property vocabulary cannot hide a future semantic or inline-picture field.
pub(super) fn validate(root: &ParsedObject, fields: &Properties<'_>) -> Result<(), AdapterError> {
    let picture = root.metadata.kind() == MetadataKind::CommonPicture;
    for field in root.properties.as_deref().unwrap_or_default() {
        if field.property.key.namespace.as_deref() != Some(MD_NAMESPACE) {
            continue;
        }
        let name = field.property.key.name.as_str();
        let known = matches!(
            name,
            "Name" | "Comment" | "Synonym" | "ObjectBelonging" | "ExtendedConfigurationObject"
        ) || if picture {
            matches!(name, "AvailabilityForChoice" | "AvailabilityForAppearance")
        } else {
            matches!(name, "Representation" | "Category" | "ToolTip" | "Picture")
        };
        if !known || !field.property.qualifiers.is_empty() {
            return Err(Properties::invalid(field));
        }
    }
    if picture {
        fields.boolean("AvailabilityForChoice")?;
        fields.boolean("AvailabilityForAppearance")?;
    } else {
        enum_value(
            fields,
            "Representation",
            &["Auto", "Picture", "PictureAndText", "Text"],
        )?;
        enum_value(
            fields,
            "Category",
            &[
                "ActionsPanel",
                "FormCommandBar",
                "FormNavigationPanel",
                "NavigationPanel",
            ],
        )?;
        if let Some(field) = fields.field("ToolTip")? {
            tooltip(field)?;
        }
        if let Some(field) = fields.field("Picture")? {
            picture_reference(field)?;
        }
    }
    Ok(())
}

/// Tooltips are native localized maps; duplicate languages or foreign lookalikes are not entries.
fn tooltip(field: &LocatedProperty) -> Result<(), AdapterError> {
    if matches!(&field.property.value, MetadataValue::Text(text) if text.is_empty()) {
        return Ok(());
    }
    let MetadataValue::Record(items) = &field.property.value else {
        return Err(Properties::invalid(field));
    };
    let mut languages = BTreeSet::new();
    for item in items {
        let MetadataValue::Record(parts) = &item.value else {
            return Err(Properties::invalid(field));
        };
        if !qualified(item, CORE, "item") || parts.len() != 2 {
            return Err(Properties::invalid(field));
        }
        let language = parts.iter().find(|part| qualified(part, CORE, "lang"));
        let content = parts.iter().find(|part| qualified(part, CORE, "content"));
        let (Some(language), Some(content)) = (language, content) else {
            return Err(Properties::invalid(field));
        };
        let (MetadataValue::Text(language), MetadataValue::Text(_)) =
            (&language.value, &content.value)
        else {
            return Err(Properties::invalid(field));
        };
        if language.trim().is_empty() || !languages.insert(language) {
            return Err(Properties::invalid(field));
        }
    }
    Ok(())
}

/// Only the native readable reference shape is covered; target existence and image bytes are separate.
fn picture_reference(field: &LocatedProperty) -> Result<(), AdapterError> {
    if matches!(&field.property.value, MetadataValue::Text(text) if text.is_empty()) {
        return Ok(());
    }
    let MetadataValue::Record(parts) = &field.property.value else {
        return Err(Properties::invalid(field));
    };
    if parts.len() != 2 {
        return Err(Properties::invalid(field));
    }
    let reference = parts.iter().find(|part| qualified(part, READABLE, "Ref"));
    let transparent = parts
        .iter()
        .find(|part| qualified(part, READABLE, "LoadTransparent"));
    let (Some(reference), Some(transparent)) = (reference, transparent) else {
        return Err(Properties::invalid(field));
    };
    let (MetadataValue::Text(reference), MetadataValue::Text(transparent)) =
        (&reference.value, &transparent.value)
    else {
        return Err(Properties::invalid(field));
    };
    if !matches!(transparent.as_str(), "true" | "false") || !picture_name(reference) {
        return Err(Properties::invalid(field));
    }
    Ok(())
}

/// Both reference domains share the native name grammar; no platform picture name is invented or resolved.
fn picture_name(reference: &str) -> bool {
    reference.split_once('.').is_some_and(|(kind, name)| {
        matches!(kind, "StdPicture" | "CommonPicture")
            && !name.contains('.')
            && MetadataReference::parse(&format!("CommonPicture.{name}")).is_ok()
    })
}

/// Namespace identity and empty annotations prevent foreign tags or typed impostors from matching.
fn qualified(property: &MetadataProperty, namespace: &str, name: &str) -> bool {
    property.key.namespace.as_deref() == Some(namespace)
        && property.key.name == name
        && property.qualifiers.is_empty()
}
