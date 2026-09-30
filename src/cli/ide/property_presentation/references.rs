//! Explicit reference syntax is interpreted only in schema-defined property contexts.

use super::{MD, Presenter, READABLE, key_is, text};
use crate::project::{
    metadata_model::{MetadataKind, MetadataProperty, MetadataValue},
    metadata_workspace::{ObjectSummary, WorkspaceError},
};
use serde_json::{Value, json};

impl Presenter<'_> {
    /// Annotated references and known form/template selectors share the same resolver.
    pub(super) fn reference(&mut self, field: &MetadataProperty) -> Option<Value> {
        let explicit = matches!(&field.value, MetadataValue::TypedText { key, .. }
            if key_is(key, READABLE, "MDObjectRef"));
        let selector = field.key.namespace.as_deref() == Some(MD)
            && matches!(
                field.key.name.as_str(),
                "DefaultChoiceForm"
                    | "DefaultFolderChoiceForm"
                    | "DefaultFolderForm"
                    | "DefaultForm"
                    | "DefaultListForm"
                    | "DefaultObjectForm"
                    | "DefaultRecordForm"
                    | "DefaultReportForm"
                    | "DefaultReportSettingsForm"
                    | "DefaultReportVariantForm"
                    | "DefaultSettingsForm"
                    | "DefaultVariantForm"
                    | "DefaultConstantsForm"
                    | "DefaultLoadForm"
                    | "DefaultSaveForm"
                    | "DefaultSearchForm"
                    | "DefaultDynamicListSettingsForm"
                    | "DefaultDataHistoryChangeHistoryForm"
                    | "DefaultDataHistoryVersionDataForm"
                    | "DefaultDataHistoryVersionDifferencesForm"
                    | "DefaultCollaborationSystemUsersChoiceForm"
                    | "ChoiceForm"
                    | "MainDataCompositionSchema"
            );
        if !explicit && !selector {
            return None;
        }
        let raw = text(&field.value)?.trim();
        if raw.is_empty() {
            return Some(
                json!({"kind":"empty", "caption": self.caption("platform-presentation-unset")}),
            );
        }
        let parts = reference_parts(raw)?;
        if !explicit
            && !matches!(
                parts.last()?.0,
                MetadataKind::Form
                    | MetadataKind::CommonForm
                    | MetadataKind::Template
                    | MetadataKind::CommonTemplate
            )
        {
            return None;
        }
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
        match self.project.property_reference(parts) {
            Ok(object) => {
                result["caption"] =
                    self.localized(|locale| reference_name(&object, locale.locale().as_str()));
                result["target"] = json!(object.id.as_str());
                result["status"] = json!("resolved");
            }
            Err(error) => {
                result["status"] = json!(if matches!(
                    error,
                    WorkspaceError::UnknownObject(_) | WorkspaceError::MissingSource(_)
                ) {
                    "missing"
                } else {
                    "unavailable"
                });
            }
        }
        self.references.insert(raw.to_owned(), result.clone());
        Some(result)
    }
}

/// Designer references alternate kind and name; reject partial, oversized or unknown paths.
fn reference_parts(raw: &str) -> Option<Vec<(MetadataKind, &str)>> {
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
fn reference_name(object: &ObjectSummary, language: &str) -> String {
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
