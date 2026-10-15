//! Translate logical `QNames` and ordered type alternatives without creating an XML intermediary.

#[cfg(test)]
mod tests;

use bsl_metadata::{AttributeType, MdoType, metadata_object::PlatformValueType};
use eska::project_core::{
    metadata_model::{MetadataProperty, MetadataValue, PropertyKey},
    metadata_parser::{LocatedProperty, ParsedObject},
};

use crate::{AdapterError, AdapterIssue, properties::Properties};

const CORE: &str = "http://v8.1c.ru/8.1/data/core";
const CONFIG: &str = "http://v8.1c.ru/8.1/data/enterprise/current-config";
const SCHEMA: &str = "http://www.w3.org/2001/XMLSchema";
const COMPOSITION_SETTINGS: &str = "http://v8.1c.ru/8.1/data-composition-system/settings";

/// Required types share one missing-field boundary across objects, registers and defined types.
pub fn required_type(object: &ParsedObject) -> Result<AttributeType, AdapterError> {
    let fields = Properties::new(object)?;
    let field = fields.field("Type")?.ok_or_else(|| AdapterError {
        issue: AdapterIssue::MissingProperty {
            name: "Type".into(),
        },
        range: object.range.clone(),
    })?;
    attribute_type(field)
}

/// Convert one complete Type property; unsupported alternatives fail rather than disappear.
///
/// # Errors
/// Returns malformed records, duplicate qualifiers, invalid numbers or unresolved/unsupported `QNames`.
pub fn attribute_type(source: &LocatedProperty) -> Result<AttributeType, AdapterError> {
    let reader = TypeReader { source };
    let fields = reader.record(&source.property)?;
    reader.check_fields(
        fields,
        &[
            "Type",
            "TypeSet",
            "StringQualifiers",
            "NumberQualifiers",
            "DateQualifiers",
        ],
    )?;
    let qualifiers = reader.qualifiers(fields)?;
    // The selected kernel groups Type before TypeSet, retaining order within each group.
    let mut types = Vec::new();
    for name in ["Type", "TypeSet"] {
        for field in fields.iter().filter(|field| is_core(field, name)) {
            let key = match &field.value {
                MetadataValue::QualifiedText { key, .. } => key,
                MetadataValue::Text(text) if text.is_empty() => continue,
                MetadataValue::Text(text) => {
                    return Err(AdapterError {
                        issue: AdapterIssue::UnresolvedTypeName { text: text.clone() },
                        range: source.range.clone(),
                    });
                }
                _ => return Err(Properties::invalid(source)),
            };
            let value = if name == "TypeSet" {
                type_set(key)
            } else {
                single_type(key, &qualifiers)
            }
            .ok_or_else(|| AdapterError {
                issue: AdapterIssue::UnsupportedType { key: key.clone() },
                range: source.range.clone(),
            })?;
            types.push(value);
        }
    }
    Ok(match types.len() {
        0 => AttributeType::Unknown,
        1 => types.remove(0),
        _ => AttributeType::Composite { types },
    })
}

struct TypeReader<'a> {
    source: &'a LocatedProperty,
}

impl TypeReader<'_> {
    /// Unknown fields cannot be silently interpreted as an empty, successful type description.
    fn check_fields(
        &self,
        fields: &[MetadataProperty],
        names: &[&str],
    ) -> Result<(), AdapterError> {
        if let Some(field) = fields.iter().find(|field| {
            field.key.namespace.as_deref() != Some(CORE)
                || !names.contains(&field.key.name.as_str())
        }) {
            return Err(AdapterError {
                issue: AdapterIssue::UnsupportedTypeField {
                    key: field.key.clone(),
                },
                range: self.source.range.clone(),
            });
        }
        Ok(())
    }

    /// Empty nodes are meaningful; nonempty scalars cannot masquerade as a Type record.
    fn record<'a>(
        &self,
        field: &'a MetadataProperty,
    ) -> Result<&'a [MetadataProperty], AdapterError> {
        match &field.value {
            MetadataValue::Record(fields) => Ok(fields),
            MetadataValue::Text(text) if text.is_empty() => Ok(&[]),
            _ => Err(Properties::invalid(self.source)),
        }
    }

    /// Repeated Type/TypeSet are lists; a qualifier is a single semantic field.
    fn unique<'a>(
        &self,
        fields: &'a [MetadataProperty],
        name: &str,
    ) -> Result<Option<&'a MetadataProperty>, AdapterError> {
        let mut matching = fields.iter().filter(|field| is_core(field, name));
        let first = matching.next();
        if matching.next().is_some() {
            return Err(AdapterError {
                issue: AdapterIssue::DuplicateProperty { name: name.into() },
                range: self.source.range.clone(),
            });
        }
        Ok(first)
    }

    /// Read modeled qualifiers strictly; invalid explicit values do not fall back to defaults.
    fn scalar<'a>(
        &self,
        fields: &'a [MetadataProperty],
        name: &str,
    ) -> Result<Option<&'a str>, AdapterError> {
        self.unique(fields, name)?
            .map(|field| match &field.value {
                MetadataValue::Text(text) => Ok(text.as_str()),
                _ => Err(Properties::invalid(self.source)),
            })
            .transpose()
    }

    /// The outer property range also addresses nested opaque qualifier failures in YAML.
    fn qualifier_fields<'a>(
        &self,
        fields: &'a [MetadataProperty],
        name: &str,
    ) -> Result<&'a [MetadataProperty], AdapterError> {
        self.unique(fields, name)?
            .map_or(Ok(&[]), |field| self.record(field))
    }

    /// Validate source enums even when the kernel does not retain their distinction.
    fn validate_enum(
        &self,
        fields: &[MetadataProperty],
        name: &str,
        allowed: &[&str],
    ) -> Result<(), AdapterError> {
        if self
            .scalar(fields, name)?
            .is_some_and(|value| !allowed.contains(&value))
        {
            return Err(Properties::invalid(self.source));
        }
        Ok(())
    }

    /// Integer width follows the selected kernel's model, with checked narrowing.
    fn qualifiers(&self, fields: &[MetadataProperty]) -> Result<Qualifiers, AdapterError> {
        let strings = self.qualifier_fields(fields, "StringQualifiers")?;
        let numbers = self.qualifier_fields(fields, "NumberQualifiers")?;
        let dates = self.qualifier_fields(fields, "DateQualifiers")?;
        self.check_fields(strings, &["Length", "AllowedLength"])?;
        self.check_fields(numbers, &["Digits", "FractionDigits", "AllowedSign"])?;
        self.check_fields(dates, &["DateFractions"])?;
        self.validate_enum(strings, "AllowedLength", &["Variable", "Fixed"])?;
        self.validate_enum(numbers, "AllowedSign", &["Any", "Nonnegative"])?;
        let invalid = || Properties::invalid(self.source);
        let length = self
            .scalar(strings, "Length")?
            .map(|value| value.parse().map_err(|_| invalid()))
            .transpose()?;
        let precision = self
            .scalar(numbers, "Digits")?
            .map(|value| value.parse().map_err(|_| invalid()))
            .transpose()?
            .unwrap_or(10);
        let scale = self
            .scalar(numbers, "FractionDigits")?
            .map(|value| value.parse().map_err(|_| invalid()))
            .transpose()?
            .unwrap_or(0);
        let datetime = match self.scalar(dates, "DateFractions")? {
            None => false,
            Some(value) if value.eq_ignore_ascii_case("DateTime") => true,
            Some(value)
                if value.eq_ignore_ascii_case("Date") || value.eq_ignore_ascii_case("Time") =>
            {
                false
            }
            Some(_) => return Err(invalid()),
        };
        Ok(Qualifiers {
            length,
            precision,
            scale,
            datetime,
        })
    }
}

struct Qualifiers {
    length: Option<u32>,
    precision: u8,
    scale: u8,
    datetime: bool,
}

/// Prefix spelling has no role in recognizing properties or values.
fn is_core(field: &MetadataProperty, name: &str) -> bool {
    field.key.namespace.as_deref() == Some(CORE) && field.key.name == name
}

/// Broad reference sets and named defined/characteristic types remain separate alternatives.
fn type_set(key: &PropertyKey) -> Option<AttributeType> {
    if key.namespace.as_deref() != Some(CONFIG) {
        return None;
    }
    match key.name.as_str() {
        // Designer TypeSet retains the legacy spelling of the AnyIBRef reference category.
        "AnyRef" | "AnyIBRef" => Some(AttributeType::AnyRef),
        "BusinessProcessRoutePointRef" => Some(AttributeType::AnyObjectRef {
            mdo_type: MdoType::BusinessProcess,
        }),
        name if name.starts_with("DefinedType.") => {
            named(name, "DefinedType").map(|name| AttributeType::DefinedType { name: name.into() })
        }
        name if name.starts_with("Characteristic.") => {
            named(name, "Characteristic").map(|name| AttributeType::Ref {
                mdo_type: MdoType::ChartOfCharacteristicTypes,
                name: name.into(),
            })
        }
        name => reference_kind(name.strip_suffix("Ref")?)
            .map(|mdo_type| AttributeType::AnyObjectRef { mdo_type }),
    }
}

/// Reject malformed reference identities instead of slicing off extra segments.
fn named<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    let (kind, name) = value.split_once('.')?;
    (kind == prefix
        && !name.is_empty()
        && !name.contains('.')
        && !name.contains(char::is_whitespace))
    .then_some(name)
}

/// Only metadata kinds with modeled reference semantics can be reconstructed from a `QName`.
fn reference_kind(value: &str) -> Option<MdoType> {
    let kind: MdoType = value.parse().ok()?;
    if kind.english_name() != value {
        return None;
    }
    matches!(
        kind,
        MdoType::Catalog
            | MdoType::Document
            | MdoType::Enum
            | MdoType::Task
            | MdoType::ExchangePlan
            | MdoType::BusinessProcess
            | MdoType::ChartOfAccounts
            | MdoType::ChartOfCalculationTypes
            | MdoType::ChartOfCharacteristicTypes
            | MdoType::InformationRegister
            | MdoType::AccumulationRegister
            | MdoType::AccountingRegister
            | MdoType::CalculationRegister
    )
    .then_some(kind)
}

/// Preserve the selected kernel's distinction between specific and generic object references.
fn configuration_type(value: &str) -> Option<AttributeType> {
    let platform = match value {
        "DynamicList" => Some(PlatformValueType::DynamicList),
        "ConstantsSet" => Some(PlatformValueType::ConstantsSet),
        "ReportBuilder" => Some(PlatformValueType::ReportBuilder),
        _ => None,
    };
    if let Some(value) = platform {
        return Some(AttributeType::Platform(value));
    }
    if value == "BusinessProcessRoutePointRef" {
        return Some(AttributeType::AnyObjectRef {
            mdo_type: MdoType::BusinessProcess,
        });
    }
    let (token, name) = value
        .split_once('.')
        .map_or((value, None), |(token, name)| (token, Some(name)));
    if name.is_some_and(|name| {
        name.is_empty() || name.contains('.') || name.contains(char::is_whitespace)
    }) {
        return None;
    }
    let kind = if let Some(kind) = token.strip_suffix("Object") {
        let parsed = match kind {
            "DataProcessor" => MdoType::DataProcessor,
            "Report" => MdoType::Report,
            "ExternalDataProcessor" => MdoType::ExternalDataProcessor,
            "ExternalReport" => MdoType::ExternalReport,
            _ => reference_kind(kind)?,
        };
        if matches!(
            parsed,
            MdoType::Enum
                | MdoType::InformationRegister
                | MdoType::AccumulationRegister
                | MdoType::AccountingRegister
                | MdoType::CalculationRegister
        ) {
            return None;
        }
        if name.is_none() {
            if matches!(
                parsed,
                MdoType::Enum
                    | MdoType::ChartOfAccounts
                    | MdoType::InformationRegister
                    | MdoType::AccumulationRegister
                    | MdoType::AccountingRegister
                    | MdoType::CalculationRegister
                    | MdoType::ExchangePlan
            ) {
                return None;
            }
            return Some(AttributeType::AnyObjectRef { mdo_type: parsed });
        }
        parsed
    } else if token == "ConstantValueManager" {
        MdoType::Constant
    } else if token == "BusinessProcessRoutePointRef" {
        MdoType::BusinessProcess
    } else if let Some(kind) = token.strip_suffix("RecordSet") {
        let kind = reference_kind(kind)?;
        if !matches!(
            kind,
            MdoType::InformationRegister
                | MdoType::AccumulationRegister
                | MdoType::AccountingRegister
                | MdoType::CalculationRegister
        ) {
            return None;
        }
        kind
    } else {
        reference_kind(token.strip_suffix("Ref")?)?
    };
    Some(AttributeType::Ref {
        mdo_type: kind,
        name: name?.into(),
    })
}

/// Resolve platform types only in 1C namespaces; foreign namespaces cannot borrow catalog names.
fn single_type(key: &PropertyKey, qualifiers: &Qualifiers) -> Option<AttributeType> {
    let namespace = key.namespace.as_deref()?;
    let name = key.name.as_str();
    if namespace == SCHEMA {
        return match name {
            "boolean" => Some(AttributeType::Boolean),
            "string" => Some(AttributeType::String {
                length: qualifiers.length,
            }),
            "decimal" => Some(AttributeType::Number {
                precision: qualifiers.precision,
                scale: qualifiers.scale,
            }),
            "dateTime" => Some(if qualifiers.datetime {
                AttributeType::DateTime
            } else {
                AttributeType::Date
            }),
            _ => None,
        };
    }
    if namespace == CONFIG {
        return configuration_type(name);
    }
    if namespace == COMPOSITION_SETTINGS {
        return match name {
            "SettingsComposer" => {
                Some(AttributeType::Platform(PlatformValueType::SettingsComposer))
            }
            "Filter" => Some(AttributeType::Platform(
                PlatformValueType::DataCompositionFilter,
            )),
            _ => None,
        };
    }
    if !namespace
        .strip_prefix("http://v8.1c.ru/")
        .is_some_and(|rest| rest.contains("/data/"))
    {
        return None;
    }
    if namespace == CORE {
        match name {
            "UUID" => return Some(AttributeType::Uuid),
            "ValueStorage" => return Some(AttributeType::ValueStorage),
            _ => {}
        }
    }
    let platform = match name {
        "ValueListType" => Some(PlatformValueType::ValueList),
        "ValueTable" => Some(PlatformValueType::ValueTable),
        "ValueTree" => Some(PlatformValueType::ValueTree),
        "StandardPeriod" => Some(PlatformValueType::StandardPeriod),
        "StandardBeginningDate" => Some(PlatformValueType::StandardBeginningDate),
        "TypeDescription" => Some(PlatformValueType::TypeDescription),
        "FixedStructure" => Some(PlatformValueType::FixedStructure),
        "FixedArray" => Some(PlatformValueType::FixedArray),
        "FixedMap" => Some(PlatformValueType::FixedMap),
        "Type" => Some(PlatformValueType::Type),
        "Null" => Some(PlatformValueType::Null),
        "FormattedString" => Some(PlatformValueType::FormattedString),
        "Picture" => Some(PlatformValueType::Picture),
        "Color" => Some(PlatformValueType::Color),
        "Font" => Some(PlatformValueType::Font),
        "SpreadsheetDocument" => Some(PlatformValueType::SpreadsheetDocument),
        "FormattedDocument" => Some(PlatformValueType::FormattedDocument),
        "Chart" => Some(PlatformValueType::Chart),
        "GanttChart" => Some(PlatformValueType::GanttChart),
        "SettingsComposer" => Some(PlatformValueType::SettingsComposer),
        "Filter" => Some(PlatformValueType::DataCompositionFilter),
        _ => None,
    };
    platform.map(AttributeType::Platform).or_else(|| {
        bsl_platform::PlatformDataInner::instance()
            .get_type(name)
            .map(|value| AttributeType::PlatformNamed(value.name.to_string()))
    })
}
