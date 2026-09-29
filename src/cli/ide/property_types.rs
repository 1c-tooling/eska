//! Presentation hints for known boolean fields; source text and core values stay lossless.

use crate::project::metadata_model::{MetadataKind, MetadataProperty, MetadataValue, PropertyKey};

const MD: &str = "http://v8.1c.ru/8.3/MDClasses";
const READABLE: &str = "http://v8.1c.ru/8.3/xcf/readable";
const APP: &str = "http://v8.1c.ru/8.2/managed-application/core";

// Exported EBoolean properties from the EDT model recorded in platform-sources.json.
// Ambiguous names are handled by owner below; never infer a type from the value alone.
const BOOLEAN_PROPERTIES: &[&str] = &[
    "ActionPeriod",
    "ActionPeriodUse",
    "AllowNull",
    "AutoConnect",
    "AutoOrderByCode",
    "Autonumbering",
    "AvailabilityForAppearance",
    "AvailabilityForChoice",
    "Balance",
    "BaseDimension",
    "BasePeriod",
    "CheckUnique",
    "ClientManagedApplication",
    "ClientOrdinaryApplication",
    "Correspondence",
    "CreateTaskInPrivilegedMode",
    "DenyIncompleteValues",
    "DistributedInfoBase",
    "EnableTotalsSliceFirst",
    "EnableTotalsSliceLast",
    "EnableTotalsSplitting",
    "ExecuteAfterWriteDataHistoryVersionProcessing",
    "ExtendedEdit",
    "ExternalConnection",
    "FillFromFillingValue",
    "FoldersOnTop",
    "Global",
    "Hierarchical",
    "IncludeConfigurationExtensions",
    "IncludeHelpInContents",
    "IncludeInCommandInterface",
    "KeepMappingToExtendedConfigurationObjectsByIDs",
    "LimitLevelCount",
    "MainFilter",
    "MainFilterOnPeriod",
    "MarkNegatives",
    "Master",
    "ModifiesData",
    "MultiLine",
    "Nillable",
    "PasswordMode",
    "PostInPrivilegedMode",
    "Privileged",
    "PrivilegedGetMode",
    "ReadOnly",
    "ReturnValue",
    "Server",
    "ServerCall",
    "Switchable",
    "Transactioned",
    "UnpostInPrivilegedMode",
    "UpdateDataHistoryImmediatelyAfterWrite",
    "UseInTotals",
    "UseManagedFormInOrdinaryApplication",
    "UseOSAuthentication",
    "UseOSProxy",
    "UseOneCommand",
    "UseOrdinaryFormInManagedApplication",
    "UseStandardCommands",
];

/// Explicit XML types take precedence over inferred schema hints, without rewriting raw text.
pub(super) fn is_boolean_value(
    owner: MetadataKind,
    root: &PropertyKey,
    path: &[&PropertyKey],
    value: &MetadataProperty,
) -> bool {
    let (text, known) = match &value.value {
        MetadataValue::TypedText { text, key } => (
            text,
            key.namespace.as_deref() == Some("http://www.w3.org/2001/XMLSchema")
                && key.name == "boolean",
        ),
        MetadataValue::Text(text) => (text, is_boolean(owner, root, path, &value.key)),
        _ => return false,
    };
    known && matches!(text.trim(), "true" | "false" | "1" | "0")
}

/// Recognize only audited XML contexts, leaving user strings and future fields untouched.
fn is_boolean(
    owner: MetadataKind,
    root: &PropertyKey,
    path: &[&PropertyKey],
    key: &PropertyKey,
) -> bool {
    use MetadataKind as K;
    let namespace = key.namespace.as_deref();
    if path.is_empty() && namespace == Some(MD) {
        return BOOLEAN_PROPERTIES.contains(&key.name.as_str())
            || matches!(
                (key.name.as_str(), owner),
                (
                    "QuickChoice",
                    K::Catalog
                        | K::ChartOfAccounts
                        | K::ChartOfCalculationTypes
                        | K::ChartOfCharacteristicTypes
                        | K::Enum
                        | K::ExchangePlan
                ) | ("Use", K::ScheduledJob)
                    | ("Predefined", K::ScheduledJob | K::Bot | K::WebSocketClient)
            );
    }
    if namespace == Some("http://v8.1c.ru/8.3/xcf/predef") {
        return matches!(
            key.name.as_str(),
            "ActionPeriodIsBase" | "IsFolder" | "OffBalance" | "Turnover"
        );
    }
    if root.namespace.as_deref() != Some(MD) {
        return false;
    }
    match (root.name.as_str(), namespace, key.name.as_str()) {
        (
            "StandardAttributes",
            Some(READABLE),
            "ExtendedEdit"
            | "FillFromFillingValue"
            | "MarkNegatives"
            | "MultiLine"
            | "PasswordMode",
        )
        | (
            "UsedMobileApplicationFunctionalities"
            | "RequiredMobileApplicationPermissions"
            | "RequiredMobileApplicationPermissions8315",
            Some(APP),
            "use",
        )
        | ("Visible", Some(MD), "Common")
        | ("Picture", Some(READABLE), "LoadTransparent") => true,
        ("Visible", Some(MD), "Value") => path
            .last()
            .is_some_and(|parent| parent.namespace.as_deref() == Some(MD) && parent.name == "For"),
        _ => false,
    }
}
