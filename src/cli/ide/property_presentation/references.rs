//! Explicit reference syntax is interpreted only in schema-defined property contexts.

use super::{MD, Presenter, READABLE, key_is, reference_context, text};
use crate::project::{
    metadata_model::{MetadataKind, MetadataProperty, MetadataValue, PropertyKey},
    metadata_workspace::ObjectSummary,
};
use serde_json::{Value, json};

impl Presenter<'_> {
    /// Resolve explicit annotations or audited selectors, including their nested fields.
    pub(super) fn reference(
        &mut self,
        field: &MetadataProperty,
        path: &[&PropertyKey],
    ) -> Option<Value> {
        if path.is_empty()
            && field.key.namespace.as_deref() == Some(MD)
            && matches!(
                (self.owner, field.key.name.as_str()),
                (MetadataKind::EventSubscription, "Handler")
                    | (MetadataKind::ScheduledJob, "MethodName")
            )
        {
            return self.procedure_reference(text(&field.value)?.trim());
        }
        if let MetadataValue::TypedText { key, .. } = &field.value
            && key_is(key, READABLE, "DesignTimeRef")
        {
            return self.design_reference(text(&field.value)?.trim());
        }
        let explicit = matches!(&field.value, MetadataValue::TypedText { key, .. }
            if key_is(key, READABLE, "MDObjectRef"));
        if !explicit && !reference_context::is_reference(self.owner, &field.key, path) {
            return None;
        }
        let raw = text(&field.value)?.trim();
        if raw.is_empty() {
            return Some(
                json!({"kind":"empty", "caption": self.caption("platform-presentation-unset")}),
            );
        }
        if let Some((owner, name)) = raw.rsplit_once(".StandardAttribute.") {
            return self.standard_attribute(owner, name);
        }
        let parts = reference_parts(raw)?;
        self.resolved_reference(raw, &parts)
    }

    /// Deduplicate repeated targets within one property response, including failed reads.
    pub(super) fn resolved_reference(
        &mut self,
        raw: &str,
        parts: &[(MetadataKind, &str)],
    ) -> Option<Value> {
        if let Some(reference) = self.references.get(raw) {
            return Some(reference.clone());
        }
        let &(kind, name) = parts.last()?;
        let mut result = json!({"kind":"reference", "metadataKind":kind.as_str(),
            "caption":{"ru-RU":name,"en-US":name},
            "category":self.caption(&format!("platform-kind-{}",kind.as_str()))});
        let resolved = self.project.property_reference(parts);
        self.reference_result(&mut result, resolved);
        if parts.len() > 1 {
            let parents: Vec<_> = (1..parts.len())
                .filter_map(|length| self.project.property_reference(&parts[..length]).ok())
                .collect();
            if !parents.is_empty() {
                result["detail"] = self.localized(|locale| {
                    parents
                        .iter()
                        .map(|object| reference_name(object, locale.locale().as_str()))
                        .collect::<Vec<_>>()
                        .join(" › ")
                });
            }
        }
        self.references.insert(raw.to_owned(), result.clone());
        Some(result)
    }
}

/// Designer references alternate kind and name; reject partial, oversized or unknown paths.
pub(super) fn reference_parts(raw: &str) -> Option<Vec<(MetadataKind, &str)>> {
    let tokens: Vec<_> = raw.split('.').collect();
    if tokens.is_empty() || tokens.len() > 64 || !tokens.len().is_multiple_of(2) {
        return None;
    }
    tokens
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            if pair[1].is_empty() || pair[1].chars().any(char::is_whitespace) {
                return None;
            }
            Some((MetadataKind::from_xml_tag(pair[0]).ok()?, pair[1]))
        })
        .collect()
}

/// Follow the selected language, then another nonempty synonym, then the exact object name.
pub(super) fn reference_name(object: &ObjectSummary, language: &str) -> String {
    let base = language.split('-').next().unwrap_or(language);
    object
        .synonyms
        .iter()
        .find(|s| s.language.eq_ignore_ascii_case(language) && !s.content.trim().is_empty())
        .or_else(|| {
            object
                .synonyms
                .iter()
                .find(|s| s.language.eq_ignore_ascii_case(base) && !s.content.trim().is_empty())
        })
        .or_else(|| {
            object
                .synonyms
                .iter()
                .find(|s| !s.content.trim().is_empty())
        })
        .map_or_else(|| object.name.clone(), |s| s.content.trim().to_owned())
}
