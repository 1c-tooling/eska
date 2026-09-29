//! Contextual presentation of platform tokens, never a replacement for source values.

use crate::project::metadata_model::{MetadataKind, PropertyKey};

use super::{Localizer, platform_value_types::PROPERTY_ENUMS};

const MD: &str = "http://v8.1c.ru/8.3/MDClasses";
const CORE: &str = "http://v8.1c.ru/8.1/data/core";
const APP: &str = "http://v8.1c.ru/8.2/managed-application/core";
const READABLE: &str = "http://v8.1c.ru/8.3/xcf/readable";

impl Localizer {
    /// Translate only a token belonging to the field's known enum domain.
    pub(crate) fn property_value_caption(
        &self,
        owner: MetadataKind,
        root: &PropertyKey,
        path: &[&PropertyKey],
        key: &PropertyKey,
        text: &str,
    ) -> Option<String> {
        let domain = if key.namespace.as_deref() == Some("http://v8.1c.ru/8.3/xcf/predef")
            && key.name == "AccountType"
        {
            Some("AccountType")
        } else if path.is_empty() && key.namespace.as_deref() == Some(MD) {
            top_level_type(owner, &key.name)
        } else {
            nested_type(owner, root, path, key)
        }?;
        self.enum_caption(domain, text)
    }

    /// Preserve unknown enum tokens, including future platform additions.
    fn enum_caption(&self, domain: &str, token: &str) -> Option<String> {
        if !token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return None;
        }
        let token = if domain == "BorderStyle" && token == "WithoutBorder" {
            "None"
        } else {
            token
        };
        self.optional_text(&format!("platform-value-{domain}-{token}"))
    }

    /// Resolve built-in XML type names using namespace URIs, independent of prefix spelling.
    pub(crate) fn type_caption(&self, key: &PropertyKey) -> Option<String> {
        let known = match key.namespace.as_deref()? {
            "http://www.w3.org/2001/XMLSchema" => matches!(
                key.name.as_str(),
                "string" | "decimal" | "boolean" | "dateTime" | "base64Binary"
            ),
            CORE => matches!(
                key.name.as_str(),
                "ValueStorage"
                    | "UUID"
                    | "Undefined"
                    | "Null"
                    | "Array"
                    | "FixedArray"
                    | "Structure"
                    | "FixedStructure"
                    | "Map"
                    | "FixedMap"
                    | "StandardPeriod"
                    | "ValueTable"
                    | "ValueListType"
                    | "ValueTree"
            ),
            "http://v8.1c.ru/8.1/data-composition-system/settings" => {
                key.name == "SettingsComposer"
            }
            "http://v8.1c.ru/8.2/data/chart" => key.name == "Chart",
            "http://v8.1c.ru/8.1/data/enterprise/current-config" => {
                matches!(key.name.as_str(), "ConstantsSet" | "ReportBuilder")
            }
            _ => false,
        };
        known
            .then(|| self.enum_caption("PrimitiveType", &key.name))
            .flatten()
    }

    /// Standard-attribute identities are platform vocabulary; arbitrary object names are not.
    pub(crate) fn qualifier_caption(
        &self,
        field: &PropertyKey,
        qualifier: &PropertyKey,
        text: &str,
    ) -> Option<String> {
        if field.namespace.as_deref() == Some(READABLE)
            && field.name == "StandardAttribute"
            && qualifier.namespace.is_none()
            && qualifier.name == "name"
        {
            self.enum_caption("StandardAttribute", text)
        } else {
            None
        }
    }
}

/// Some identically named metadata fields use different enums according to their owner.
fn top_level_type(owner: MetadataKind, name: &str) -> Option<&'static str> {
    use MetadataKind as K;
    let contextual = match (name, owner) {
        ("Event", K::EventSubscription) => "MetadataEvent",
        ("Group", K::Command | K::CommonCommand) => "StandardCommandsGroup",
        ("Type", K::StyleItem) => "StyleElementType",
        ("CodeType", K::ChartOfCalculationTypes) => "ChartOfCalculationTypesCodeType",
        (
            "CodeType",
            K::Catalog | K::ChartOfAccounts | K::ChartOfCharacteristicTypes | K::ExchangePlan,
        ) => "CatalogCodeType",
        ("CodeSeries", K::ChartOfAccounts) => "CharOfAccountCodeSeries",
        ("CodeSeries", K::ChartOfCharacteristicTypes) => "CharacteristicKindCodesSeries",
        ("CodeSeries", K::Catalog) => "CatalogCodesSeries",
        ("DefaultPresentation", K::Catalog) => "CatalogMainPresentation",
        ("DefaultPresentation", K::ChartOfAccounts) => "AccountMainPresentation",
        ("DefaultPresentation", K::ChartOfCalculationTypes) => "CalculationTypeMainPresentation",
        ("DefaultPresentation", K::ChartOfCharacteristicTypes) => {
            "CharacteristicTypeMainPresentation"
        }
        ("DefaultPresentation", K::ExchangePlan) => "DataExchangeMainPresentation",
        ("DefaultPresentation", K::Task) => "TaskMainPresentation",
        ("NumberType", K::Document | K::DocumentNumerator) => "DocumentNumberType",
        ("NumberType", K::BusinessProcess) => "BusinessProcessNumberType",
        ("NumberType", K::Task) => "TaskNumberType",
        ("NumberPeriodicity", K::Document | K::DocumentNumerator) => "DocumentNumberPeriodicity",
        ("NumberPeriodicity", K::BusinessProcess) => "BusinessProcessNumberPeriodicity",
        _ => {
            return PROPERTY_ENUMS
                .iter()
                .find_map(|(field, domain)| (*field == name).then_some(*domain));
        }
    };
    Some(contextual)
}

/// Nested scalar names such as Value and functionality require an enclosing semantic context.
fn nested_type(
    owner: MetadataKind,
    root: &PropertyKey,
    path: &[&PropertyKey],
    key: &PropertyKey,
) -> Option<&'static str> {
    let namespace = key.namespace.as_deref()?;
    let parent = path.last()?;
    if namespace == CORE && parent.namespace.as_deref() == Some(CORE) {
        return match (parent.name.as_str(), key.name.as_str()) {
            ("StringQualifiers" | "BinaryDataQualifiers", "AllowedLength") => Some("AllowedLength"),
            ("NumberQualifiers", "AllowedSign") => Some("AllowedSign"),
            ("DateQualifiers", "DateFractions") => Some("DateFractions"),
            _ => None,
        };
    }
    if root.namespace.as_deref() != Some(MD) {
        return None;
    }
    match (root.name.as_str(), namespace, key.name.as_str()) {
        ("Content", READABLE, "Use") if owner == MetadataKind::CommonAttribute => {
            Some("CommonAttributeUse")
        }
        ("UsePurposes", CORE, "Value") => Some("ApplicationUsePurpose"),
        ("UsedMobileApplicationFunctionalities", APP, "functionality") => {
            Some("MobileApplicationFunctionalities")
        }
        ("UsedMobileApplicationFunctionalities", APP, "permission") => {
            Some("RequiredMobileApplicationPermissionMessages")
        }
        (
            "RequiredMobileApplicationPermissions" | "RequiredMobileApplicationPermissions8315",
            APP,
            "permission",
        ) => Some("RequiredMobileApplicationPermissions"),
        ("AllowedIncomingShareRequestTypes", APP, "processingVariant") => {
            Some("AllowedIncomingShareRequestTypeProcessingVariant")
        }
        ("ChoiceParameterLinks" | "StandardAttributes", READABLE, "ValueChange") => {
            Some("LinkedValueChangeMode")
        }
        ("StandardAttributes" | "StandardTabularSections", READABLE, name) => match name {
            "QuickChoice" => Some("UseQuickChoice"),
            "FillChecking" => Some("FillChecking"),
            "FullTextSearch" => Some("FullTextSearchUsing"),
            "CreateOnInput" => Some("CreateOnInput"),
            "ChoiceHistoryOnInput" => Some("ChoiceHistoryOnInput"),
            "TypeReductionMode" => Some("TypeReductionMode"),
            "DataHistory" => Some("DataHistoryUse"),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::localization::Locale;

    /// The frozen EDT enum inventory and supplemental Designer tokens have both captions.
    #[test]
    fn all_catalog_values_have_translations() {
        for locale in [Locale::RuRu, Locale::EnUs] {
            let localizer = Localizer::try_new(locale).unwrap();
            for line in include_str!("../../../locales/platform-values.tsv")
                .lines()
                .skip(1)
            {
                let cells: Vec<_> = line.split('\t').collect();
                assert_eq!(cells.len(), 4);
                assert_eq!(
                    localizer.enum_caption(cells[0], cells[1]),
                    localizer.optional_text(cells[3]),
                    "{line}"
                );
                assert!(
                    localizer
                        .optional_text(cells[3])
                        .is_some_and(|text| !text.trim().is_empty())
                );
                // EDT's Russian palette contains English placeholders; do not reimport them.
                if locale == Locale::RuRu && cells[0] != "HTTPMethod" {
                    let caption = localizer.optional_text(cells[3]).unwrap();
                    assert!(
                        caption
                            .chars()
                            .any(|c| ('\u{0400}'..='\u{04ff}').contains(&c))
                            || ["NFC", "SMS", "Null"].contains(&caption.as_str()),
                        "Untranslated Russian caption: {line} = {caption}"
                    );
                }
            }
        }
    }

    /// The same text in Comment or a foreign namespace remains a user string.
    #[test]
    fn enum_translation_requires_its_property_domain() {
        let localizer = Localizer::try_new(Locale::RuRu).unwrap();
        let mut key = PropertyKey {
            namespace: Some(MD.into()),
            name: "CompatibilityMode".into(),
        };
        assert_eq!(
            localizer
                .property_value_caption(
                    MetadataKind::Configuration,
                    &key,
                    &[],
                    &key,
                    "Version8_3_27"
                )
                .as_deref(),
            Some("Версия 8.3.27")
        );
        key.name = "Comment".into();
        assert_eq!(
            localizer.property_value_caption(
                MetadataKind::Configuration,
                &key,
                &[],
                &key,
                "Version8_3_27"
            ),
            None
        );
        key.name = "CompatibilityMode".into();
        key.namespace = Some("urn:custom".into());
        assert_eq!(
            localizer.property_value_caption(
                MetadataKind::Configuration,
                &key,
                &[],
                &key,
                "Version8_3_27"
            ),
            None
        );
        key.namespace = Some(MD.into());
        assert_eq!(
            localizer.property_value_caption(
                MetadataKind::Configuration,
                &key,
                &[],
                &key,
                "FutureMode"
            ),
            None
        );
    }

    /// A generic nested Value field translates only inside the relevant platform property.
    #[test]
    fn nested_values_are_contextual() {
        let localizer = Localizer::try_new(Locale::RuRu).unwrap();
        let mut root = PropertyKey {
            namespace: Some(MD.into()),
            name: "UsedMobileApplicationFunctionalities".into(),
        };
        let key = PropertyKey {
            namespace: Some(APP.into()),
            name: "functionality".into(),
        };
        assert_eq!(
            localizer
                .property_value_caption(
                    MetadataKind::Configuration,
                    &root,
                    &[&root, &key],
                    &key,
                    "Biometrics"
                )
                .as_deref(),
            Some("Биометрия")
        );
        root.name = "Comment".into();
        assert_eq!(
            localizer.property_value_caption(
                MetadataKind::Configuration,
                &root,
                &[&root, &key],
                &key,
                "Biometrics"
            ),
            None
        );
    }

    /// Reference-like platform values are translated only for their owning metadata kinds.
    #[test]
    fn command_groups_and_subscription_events_have_scoped_captions() {
        let localizer = Localizer::try_new(Locale::RuRu).unwrap();
        for (owner, name, token, caption) in [
            (
                MetadataKind::CommonCommand,
                "Group",
                "NavigationPanelOrdinary",
                "Панель навигации: Обычное",
            ),
            (
                MetadataKind::EventSubscription,
                "Event",
                "BeforeWrite",
                "Перед записью",
            ),
        ] {
            let key = PropertyKey {
                namespace: Some(MD.into()),
                name: name.into(),
            };
            assert_eq!(
                localizer
                    .property_value_caption(owner, &key, &[], &key, token)
                    .as_deref(),
                Some(caption)
            );
            assert!(
                localizer
                    .property_value_caption(MetadataKind::Catalog, &key, &[], &key, token)
                    .is_none()
            );
        }
    }
}
