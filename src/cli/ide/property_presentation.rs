//! Optional semantic presentation; every original field and annotation stays on the wire.

mod reference_context;
mod reference_values;
mod references;
mod types;

use super::dto::Labels;
use crate::{
    cli::localization::Localizer,
    project::{
        metadata_model::{MetadataKind, MetadataProperty, MetadataValue, PropertyKey},
        metadata_workspace::ProjectSession,
    },
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const MD: &str = "http://v8.1c.ru/8.3/MDClasses";
const CORE: &str = "http://v8.1c.ru/8.1/data/core";
const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";
const XS: &str = "http://www.w3.org/2001/XMLSchema";
const CFG: &str = "http://v8.1c.ru/8.1/data/enterprise/current-config";
const READABLE: &str = "http://v8.1c.ru/8.3/xcf/readable";

pub(super) struct Presenter<'a> {
    labels: &'a Labels,
    project: &'a mut ProjectSession,
    owner: MetadataKind,
    references: BTreeMap<String, Value>,
}

impl<'a> Presenter<'a> {
    /// Memoize only references in this response; project cache still owns source invalidation.
    pub(super) const fn new(
        labels: &'a Labels,
        project: &'a mut ProjectSession,
        owner: MetadataKind,
    ) -> Self {
        Self {
            labels,
            project,
            owner,
            references: BTreeMap::new(),
        }
    }

    /// Attach presentation at any depth without replacing raw values or unknown structures.
    pub(super) fn annotate(&mut self, field: &MetadataProperty, wire: &mut Value) {
        self.annotate_at(field, wire, &mut Vec::new());
    }

    /// Keep the original property path to distinguish reused XML names.
    fn annotate_at<'f>(
        &mut self,
        field: &'f MetadataProperty,
        wire: &mut Value,
        path: &mut Vec<&'f PropertyKey>,
    ) {
        if let Some(presentation) = self
            .empty(field)
            .or_else(|| self.reference(field, path))
            .or_else(|| self.types(field))
        {
            wire["presentation"] = presentation;
        }
        if let MetadataValue::Record(fields) = &field.value
            && let Some(children) = wire["value"]["fields"].as_array_mut()
        {
            path.push(&field.key);
            for (field, child) in fields.iter().zip(children) {
                self.annotate_at(field, child, path);
            }
            path.pop();
        }
    }

    /// XML nil and an explicitly typed empty string have different visible meanings.
    fn empty(&self, field: &MetadataProperty) -> Option<Value> {
        if !text(&field.value)?.trim().is_empty() {
            return None;
        }
        let nil = field
            .qualifiers
            .iter()
            .any(|(key, value)| key_is(key, XSI, "nil") && matches!(value.trim(), "true" | "1"));
        let key = if nil {
            "platform-presentation-unset"
        } else if matches!(&field.value, MetadataValue::TypedText { key, .. } if key_is(key, XS, "string"))
        {
            "platform-presentation-empty-string"
        } else if let MetadataValue::TypedText { key, .. } = &field.value {
            if key_is(key, CORE, "Undefined") || key_is(key, CORE, "Null") {
                return Some(json!({"kind":"empty", "caption": self.localized(|locale|
                    locale.type_caption(key).unwrap_or_else(|| key.name.clone()))}));
            }
            return None;
        } else {
            return None;
        };
        Some(json!({"kind":"empty", "caption": self.caption(key)}))
    }

    /// Both captions are returned regardless of the transport locale.
    fn localized(&self, format: impl Fn(&Localizer) -> String) -> Value {
        json!({"ru-RU":format(&self.labels.ru),"en-US":format(&self.labels.en)})
    }

    /// Fixed platform vocabulary comes from the dedicated Fluent resource.
    fn caption(&self, key: &str) -> Value {
        self.localized(|locale| locale.text(key))
    }
}

/// Namespace identities, rather than arbitrary prefixes, select a known vocabulary.
fn key_is(key: &PropertyKey, namespace: &str, name: &str) -> bool {
    key.namespace.as_deref() == Some(namespace) && key.name == name
}

/// Preserve the distinction between scalar content and records, including empty records.
fn text(value: &MetadataValue) -> Option<&str> {
    match value {
        MetadataValue::Text(text)
        | MetadataValue::QualifiedText { text, .. }
        | MetadataValue::TypedText { text, .. } => Some(text),
        _ => None,
    }
}
