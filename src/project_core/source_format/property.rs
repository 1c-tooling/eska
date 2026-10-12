//! Owner-specific editable scalar vocabulary; unsupported shapes remain preserved XML.

#[cfg(test)]
mod tests;

use serde::{Deserialize, Deserializer, Serialize};

use crate::project_core::MetadataKind;

use super::{Error, PropertyName, PropertyValue};

/// An exact nonnegative integer spelling, without a machine-size or floating-point conversion.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ExactUnsigned(String);

impl ExactUnsigned {
    /// Retain all digits, including leading zeroes, and refuse signs, fractions and coercions.
    ///
    /// # Errors
    /// Returns `source_property_type_invalid` for an empty or non-integer spelling.
    pub fn parse(value: &str) -> Result<Self, Error> {
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(Error::root("source_property_type_invalid"));
        }
        Ok(Self(value.into()))
    }

    /// Return the exact spelling used by YAML, native XML and semantic metadata consumers.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ExactUnsigned {
    /// A YAML numeric token cannot silently discard precision or leading zeroes.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::parse(&String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Scalar domains belong to a native property and owner, never to a coincidental text value.
#[derive(Clone, Copy)]
pub(super) enum ValueKind {
    Text,
    Localized,
    Boolean,
    Integer,
    Enumeration(&'static [&'static str]),
    TypeDescription,
}

impl PropertyName {
    /// Resolve the closed native vocabulary shared by import, policy diagnostics and validation.
    #[must_use]
    pub fn from_native(name: &str) -> Option<Self> {
        Some(match name {
            "Name" => Self::Name,
            "Comment" => Self::Comment,
            "Version" => Self::Version,
            "Synonym" => Self::Synonym,
            "Global" => Self::Global,
            "ClientManagedApplication" => Self::ClientManagedApplication,
            "ClientOrdinaryApplication" => Self::ClientOrdinaryApplication,
            "Server" => Self::Server,
            "ExternalConnection" => Self::ExternalConnection,
            "ServerCall" => Self::ServerCall,
            "Privileged" => Self::Privileged,
            "Hierarchical" => Self::Hierarchical,
            "Autonumbering" => Self::Autonumbering,
            "CheckUnique" => Self::CheckUnique,
            "LimitLevelCount" => Self::LimitLevelCount,
            "MultiLine" => Self::MultiLine,
            "PasswordMode" => Self::PasswordMode,
            "MarkNegatives" => Self::MarkNegatives,
            "FillFromFillingValue" => Self::FillFromFillingValue,
            "CodeLength" => Self::CodeLength,
            "DescriptionLength" => Self::DescriptionLength,
            "NumberLength" => Self::NumberLength,
            "LineNumberLength" => Self::LineNumberLength,
            "LevelCount" => Self::LevelCount,
            "CodeType" => Self::CodeType,
            "NumberType" => Self::NumberType,
            "CodeAllowedLength" => Self::CodeAllowedLength,
            "NumberAllowedLength" => Self::NumberAllowedLength,
            "InformationRegisterPeriodicity" => Self::InformationRegisterPeriodicity,
            "ReturnValuesReuse" => Self::ReturnValuesReuse,
            "FillChecking" => Self::FillChecking,
            "Indexing" => Self::Indexing,
            "Type" => Self::Type,
            _ => return None,
        })
    }

    /// Check a property's scalar domain only for owners accepted by the conversion matrix.
    pub(super) const fn value_kind(self, owner: MetadataKind) -> Option<ValueKind> {
        match self {
            Self::Name | Self::Comment | Self::Version => Some(ValueKind::Text),
            Self::Synonym => Some(ValueKind::Localized),
            Self::Type
                if matches!(
                    owner,
                    MetadataKind::Attribute
                        | MetadataKind::CommonAttribute
                        | MetadataKind::Constant
                        | MetadataKind::DefinedType
                        | MetadataKind::Dimension
                        | MetadataKind::Resource
                        | MetadataKind::ChartOfCharacteristicTypes
                        | MetadataKind::FilterCriterion
                        | MetadataKind::SessionParameter
                ) =>
            {
                Some(ValueKind::TypeDescription)
            }
            _ if boolean_property(self, owner) => Some(ValueKind::Boolean),
            _ if integer_property(self, owner) => Some(ValueKind::Integer),
            _ => enum_property(self, owner),
        }
    }
}

/// Property spelling alone does not prove a boolean domain in another metadata class.
const fn boolean_property(name: PropertyName, owner: MetadataKind) -> bool {
    use MetadataKind as M;
    use PropertyName as P;

    match name {
        P::Global
        | P::ClientManagedApplication
        | P::ClientOrdinaryApplication
        | P::Server
        | P::ExternalConnection
        | P::ServerCall
        | P::Privileged => {
            matches!(owner, M::CommonModule)
        }
        P::Hierarchical => matches!(owner, M::Catalog | M::ChartOfCharacteristicTypes),
        P::Autonumbering | P::CheckUnique => {
            matches!(
                owner,
                M::Catalog | M::ChartOfCharacteristicTypes | M::Document
            )
        }
        P::LimitLevelCount => matches!(owner, M::Catalog),
        P::MultiLine | P::PasswordMode | P::MarkNegatives => {
            matches!(
                owner,
                M::Attribute | M::CommonAttribute | M::Constant | M::Dimension | M::Resource
            )
        }
        P::FillFromFillingValue => {
            matches!(
                owner,
                M::Attribute | M::CommonAttribute | M::Dimension | M::Resource
            )
        }
        _ => false,
    }
}

/// Sizes are exact nonnegative integers; cross-property and platform size limits are separate.
const fn integer_property(name: PropertyName, owner: MetadataKind) -> bool {
    use MetadataKind as M;
    use PropertyName as P;

    match name {
        P::CodeLength | P::DescriptionLength => {
            matches!(
                owner,
                M::Catalog | M::ChartOfCharacteristicTypes | M::ExchangePlan
            )
        }
        P::NumberLength => matches!(owner, M::Document),
        P::LineNumberLength => matches!(owner, M::TabularSection),
        P::LevelCount => matches!(owner, M::Catalog),
        _ => false,
    }
}

/// Closed native enum vocabularies refuse unknown spellings before export or CST publication.
const fn enum_property(name: PropertyName, owner: MetadataKind) -> Option<ValueKind> {
    use MetadataKind as M;
    use PropertyName as P;

    let values: &'static [&'static str] = match name {
        P::CodeType if matches!(owner, M::Catalog) => &["String", "Number"],
        P::NumberType if matches!(owner, M::Document) => &["String", "Number"],
        P::CodeAllowedLength
            if matches!(
                owner,
                M::Catalog | M::ChartOfCharacteristicTypes | M::ExchangePlan
            ) =>
        {
            &["Variable", "Fixed"]
        }
        P::NumberAllowedLength if matches!(owner, M::Document) => &["Variable", "Fixed"],
        P::InformationRegisterPeriodicity if matches!(owner, M::InformationRegister) => &[
            "Nonperiodical",
            "Second",
            "RecorderPosition",
            "Day",
            "Month",
            "Quarter",
            "Year",
        ],
        P::ReturnValuesReuse if matches!(owner, M::CommonModule) => {
            &["DontUse", "DuringSession", "DuringRequest"]
        }
        P::FillChecking
            if matches!(
                owner,
                M::Attribute
                    | M::CommonAttribute
                    | M::Constant
                    | M::Dimension
                    | M::Resource
                    | M::TabularSection
            ) =>
        {
            &["DontCheck", "ShowError"]
        }
        P::Indexing
            if matches!(
                owner,
                M::Attribute | M::CommonAttribute | M::Dimension | M::Resource
            ) =>
        {
            &["DontIndex", "Index", "IndexWithAdditionalOrder"]
        }
        _ => return None,
    };
    Some(ValueKind::Enumeration(values))
}

impl ValueKind {
    /// Parse only exact supported native spellings; unknown values keep their original envelope.
    pub(super) fn parse(self, text: String) -> Option<PropertyValue> {
        match self {
            Self::Text => Some(PropertyValue::Text(text)),
            Self::Boolean => text.parse().ok().map(PropertyValue::Boolean),
            Self::Integer => ExactUnsigned::parse(&text).ok().map(PropertyValue::Integer),
            Self::Enumeration(values) if values.contains(&text.as_str()) => {
                Some(PropertyValue::Enumeration(text))
            }
            Self::Localized | Self::Enumeration(_) | Self::TypeDescription => None,
        }
    }

    /// Preserve absent/empty/nil states while refusing values from another scalar domain.
    pub(super) fn accepts(self, value: &PropertyValue) -> bool {
        match (self, value) {
            (_, PropertyValue::Empty | PropertyValue::Nil)
            | (Self::Text, PropertyValue::Text(_))
            | (Self::Localized, PropertyValue::Localized(_))
            | (Self::Boolean, PropertyValue::Boolean(_))
            | (Self::Integer, PropertyValue::Integer(_))
            | (Self::TypeDescription, PropertyValue::TypeDescription(_)) => true,
            (Self::Enumeration(values), PropertyValue::Enumeration(value)) => {
                values.contains(&value.as_str())
            }
            _ => false,
        }
    }
}
