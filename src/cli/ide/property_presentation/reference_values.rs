//! Design-time values and standard attributes keep their meaning and navigable owner.

use super::{
    Presenter, READABLE,
    references::{reference_name, reference_parts},
};
use crate::project::metadata_model::{MetadataKind, PropertyKey};
use serde_json::{Value, json};

impl Presenter<'_> {
    /// Standard attributes have no separate tree node; open their owner and name the field.
    pub(super) fn standard_attribute(&mut self, owner: &str, name: &str) -> Option<Value> {
        let field = PropertyKey {
            namespace: Some(READABLE.into()),
            name: "StandardAttribute".into(),
        };
        let qualifier = PropertyKey {
            namespace: None,
            name: "name".into(),
        };
        let ru = self.labels.ru.qualifier_caption(&field, &qualifier, name)?;
        let en = self.labels.en.qualifier_caption(&field, &qualifier, name)?;
        let mut result = self.resolved_reference(owner, &reference_parts(owner)?)?;
        result["detail"] = self.localized(|locale| {
            let language = locale.locale().as_str();
            let name = result["caption"][language].as_str().unwrap_or(owner);
            result["detail"][language]
                .as_str()
                .map_or_else(|| name.to_owned(), |parent| format!("{parent} › {name}"))
        });
        result["caption"] = json!({"ru-RU":ru,"en-US":en});
        result["category"] = self.caption("platform-presentation-standard-attribute");
        Some(result)
    }

    /// Method selectors navigate to the module's properties and retain the exact procedure name.
    pub(super) fn procedure_reference(&mut self, raw: &str) -> Option<Value> {
        let tokens: Vec<_> = raw.split('.').collect();
        let [tag, module, method] = tokens.as_slice() else {
            return None;
        };
        if *tag != "CommonModule" || method.is_empty() || method.chars().any(char::is_whitespace) {
            return None;
        }
        let mut result = self.resolved_reference(
            &format!("{tag}.{module}"),
            &[(MetadataKind::CommonModule, module)],
        )?;
        result["detail"] = result["caption"].clone();
        result["caption"] = json!({"ru-RU":method,"en-US":method});
        result["category"] = self.caption("platform-presentation-procedure");
        Some(result)
    }

    /// Empty references, enum values and predefined data are different XML value variants.
    pub(in crate::cli::ide) fn design_reference(&mut self, raw: &str) -> Option<Value> {
        if let Some(value) = self.references.get(raw) {
            return Some(value.clone());
        }
        if let Some(parts) = reference_parts(raw) {
            return self.resolved_reference(raw, &parts);
        }
        let tokens: Vec<_> = raw.split('.').collect();
        let [tag, name, item] = tokens.as_slice() else {
            return None;
        };
        let kind = MetadataKind::from_xml_tag(tag).ok()?;
        if !matches!(
            kind,
            MetadataKind::Catalog
                | MetadataKind::ChartOfAccounts
                | MetadataKind::ChartOfCalculationTypes
                | MetadataKind::ChartOfCharacteristicTypes
                | MetadataKind::Document
                | MetadataKind::Enum
                | MetadataKind::ExchangePlan
                | MetadataKind::BusinessProcess
                | MetadataKind::Task
        ) || name.is_empty()
            || item.is_empty()
        {
            return None;
        }
        if *item == "EmptyRef" {
            return Some(
                json!({"kind":"empty", "caption":self.caption("platform-presentation-empty-reference")}),
            );
        }
        if !matches!(
            kind,
            MetadataKind::Catalog
                | MetadataKind::ChartOfAccounts
                | MetadataKind::ChartOfCalculationTypes
                | MetadataKind::ChartOfCharacteristicTypes
        ) {
            return None;
        }
        let mut result = json!({"kind":"reference", "metadataKind":"predefined-item",
            "caption":{"ru-RU":item,"en-US":item}, "category":self.caption("platform-kind-predefined-item")});
        let owner = self.project.property_reference(&[(kind, name)]);
        if let Ok(owner) = &owner {
            result["detail"] =
                self.localized(|locale| reference_name(owner, locale.locale().as_str()));
        }
        let resolved = owner.and_then(|owner| self.project.predefined_reference(&owner.id, item));
        self.reference_result(&mut result, resolved);
        self.references.insert(raw.to_owned(), result.clone());
        Some(result)
    }

    /// Failed targets remain readable; only a successfully resolved identity becomes actionable.
    pub(super) fn reference_result(
        &self,
        result: &mut Value,
        resolved: Result<
            crate::project::metadata_workspace::ObjectSummary,
            crate::project::metadata_workspace::WorkspaceError,
        >,
    ) {
        use crate::project::metadata_workspace::WorkspaceError;
        match resolved {
            Ok(object) => {
                result["caption"] =
                    self.localized(|locale| reference_name(&object, locale.locale().as_str()));
                result["target"] = json!(object.id.as_str());
                result["status"] = json!("resolved");
            }
            Err(error) => {
                result["status"] = json!(if matches!(
                    error,
                    WorkspaceError::UnknownObject(_)
                        | WorkspaceError::UnknownNode(_)
                        | WorkspaceError::MissingSource(_)
                ) {
                    "missing"
                } else {
                    "unavailable"
                });
            }
        }
    }
}
