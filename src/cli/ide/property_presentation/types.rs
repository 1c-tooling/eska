//! Compact type descriptions are emitted only when every field has known semantics.

use super::{CFG, CORE, MD, Presenter, XS, key_is, text};
use crate::{
    cli::localization::LocalizationValue,
    project::metadata_model::{MetadataKind, MetadataProperty, MetadataValue, PropertyKey},
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

impl Presenter<'_> {
    /// Collapse the XML Type wrapper only if no unknown qualifiers or children would disappear.
    pub(super) fn types(&mut self, field: &MetadataProperty) -> Option<Value> {
        if !matches!(
            field.key.namespace.as_deref(),
            Some(MD | CORE | "http://v8.1c.ru/8.3/xcf/predef")
        ) || !matches!(
            field.key.name.as_str(),
            "Type" | "TypeDescription" | "CommandParameterType" | "Source"
        ) {
            return None;
        }
        let MetadataValue::Record(fields) = &field.value else {
            return None;
        };
        let mut qualifiers = Vec::new();
        let mut keys = BTreeSet::new();
        for child in fields {
            if !child.qualifiers.is_empty() {
                return None;
            }
            if key_is(&child.key, CORE, "Type") {
                let MetadataValue::QualifiedText { key, .. } = &child.value else {
                    return None;
                };
                keys.insert((key.namespace.as_deref(), key.name.as_str()));
            } else {
                let primitive = qualifier_primitive(child)?;
                if qualifiers.iter().any(|(name, _)| *name == primitive) {
                    return None;
                }
                qualifiers.push((primitive, child));
            }
        }
        if keys.is_empty()
            || qualifiers
                .iter()
                .any(|(name, _)| !keys.contains(&(Some(XS), *name)))
        {
            return None;
        }
        let mut items = Vec::new();
        for child in fields {
            if let MetadataValue::QualifiedText { key, .. } = &child.value {
                let mut item = self.type_item(key)?;
                if let Some((_, qualifier)) =
                    qualifiers.iter().find(|(name, _)| key_is(key, XS, name))
                {
                    item["detail"] = self.qualifier_caption(field, qualifier)?;
                }
                items.push(item);
            }
        }
        Some(json!({"kind":"types","items":items}))
    }

    /// Built-ins use their namespace; generated configuration types retain their exact identity.
    fn type_item(&mut self, key: &PropertyKey) -> Option<Value> {
        if let (Some(ru), Some(en)) = (
            self.labels.ru.type_caption(key),
            self.labels.en.type_caption(key),
        ) {
            return Some(json!({"caption":{"ru-RU":ru,"en-US":en}}));
        }
        if key.namespace.as_deref() != Some(CFG) {
            return None;
        }
        let (prefix, name) = key.name.split_once('.')?;
        if name.is_empty() || name.contains('.') || name.chars().any(char::is_whitespace) {
            return None;
        }
        let (tag, role) = generated_type(prefix)?;
        let kind = MetadataKind::from_xml_tag(tag).ok()?;
        let mut reference = self.resolved_reference(&format!("{tag}.{name}"), &[(kind, name)])?;
        if let Some(role) = role {
            let category = reference["category"].clone();
            reference["category"] = self.localized(|locale| {
                locale.format(
                    "platform-presentation-type-category",
                    &[
                        (
                            "category",
                            LocalizationValue::Text(
                                category[locale.locale().as_str()].as_str().unwrap_or(tag),
                            ),
                        ),
                        (
                            "role",
                            LocalizationValue::Text(
                                &locale.text(&format!("platform-type-role-{role}")),
                            ),
                        ),
                    ],
                )
            });
        }
        Some(reference)
    }

    /// Keep every supported constraint beside its primitive type in the original order.
    fn qualifier_caption(
        &self,
        root: &MetadataProperty,
        qualifier: &MetadataProperty,
    ) -> Option<Value> {
        let MetadataValue::Record(fields) = &qualifier.value else {
            return None;
        };
        let captions = |locale: &crate::cli::localization::Localizer| -> Option<String> {
            fields
                .iter()
                .map(|field| {
                    let name = locale.property_caption(None, &field.key)?;
                    let raw = text(&field.value)?;
                    let value = locale
                        .property_value_caption(
                            MetadataKind::Attribute,
                            &root.key,
                            &[&root.key, &qualifier.key],
                            &field.key,
                            raw,
                        )
                        .unwrap_or_else(|| raw.to_owned());
                    Some(locale.format(
                        "platform-presentation-constraint",
                        &[
                            ("name", LocalizationValue::Text(&name)),
                            ("value", LocalizationValue::Text(&value)),
                        ],
                    ))
                })
                .collect::<Option<Vec<_>>>()
                .map(|values| values.join(" · "))
        };
        Some(json!({"ru-RU":captions(&self.labels.ru)?,"en-US":captions(&self.labels.en)?}))
    }
}

/// A closed list prevents future or foreign constraints from being hidden in a compact row.
fn qualifier_primitive(field: &MetadataProperty) -> Option<&'static str> {
    if field.key.namespace.as_deref() != Some(CORE) {
        return None;
    }
    let (primitive, names): (_, &[&str]) = match field.key.name.as_str() {
        "StringQualifiers" => ("string", &["Length", "AllowedLength"]),
        "NumberQualifiers" => ("decimal", &["Digits", "FractionDigits", "AllowedSign"]),
        "DateQualifiers" => ("dateTime", &["DateFractions"]),
        "BinaryDataQualifiers" => ("base64Binary", &["Length", "AllowedLength"]),
        _ => return None,
    };
    let MetadataValue::Record(fields) = &field.value else {
        return None;
    };
    let mut seen = BTreeSet::new();
    (!fields.is_empty()
        && fields.iter().all(|child| {
            child.qualifiers.is_empty()
                && child.key.namespace.as_deref() == Some(CORE)
                && names.contains(&child.key.name.as_str())
                && seen.insert(&child.key.name)
                && text(&child.value).is_some_and(|v| !v.trim().is_empty())
        }))
    .then_some(primitive)
}

/// Generated type families differ from metadata kinds; only audited platform names are mapped.
fn generated_type(prefix: &str) -> Option<(&str, Option<&str>)> {
    if prefix == "DefinedType" {
        return Some(("DefinedType", None));
    }
    for (suffix, role) in [
        ("Ref", "reference"),
        ("Object", "object"),
        ("Manager", "manager"),
        ("RecordSet", "record-set"),
        ("RecordKey", "record-key"),
        ("List", "list"),
    ] {
        if let Some(tag) = prefix.strip_suffix(suffix) {
            let reference_family = matches!(
                tag,
                "Catalog"
                    | "Document"
                    | "Enum"
                    | "ChartOfAccounts"
                    | "ChartOfCalculationTypes"
                    | "ChartOfCharacteristicTypes"
                    | "ExchangePlan"
                    | "BusinessProcess"
                    | "Task"
            );
            let register_family = matches!(
                tag,
                "InformationRegister"
                    | "AccumulationRegister"
                    | "AccountingRegister"
                    | "CalculationRegister"
            );
            let allowed = match suffix {
                "Ref" => reference_family,
                "Object" => {
                    (reference_family && tag != "Enum") || matches!(tag, "Report" | "DataProcessor")
                }
                "RecordSet" => register_family || tag == "Sequence",
                "RecordKey" => register_family,
                "Manager" => {
                    reference_family
                        || register_family
                        || matches!(
                            tag,
                            "Report"
                                | "DataProcessor"
                                | "Constant"
                                | "Sequence"
                                | "DocumentJournal"
                                | "FilterCriterion"
                                | "SettingsStorage"
                                | "IntegrationService"
                        )
                }
                "List" => {
                    reference_family
                        || register_family
                        || matches!(tag, "DocumentJournal" | "FilterCriterion")
                }
                _ => false,
            };
            return allowed.then_some((tag, Some(role)));
        }
    }
    None
}
