//! Editable type values retain native ordering separately, without a second value authority.

use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize};

use super::{Error, ExactUnsigned, Namespace};

pub(super) const CORE_NAMESPACE: &str = "http://v8.1c.ru/8.1/data/core";

mod native;
mod semantic;
#[cfg(test)]
mod tests;

pub(super) use native::decode;

/// Alternatives and qualifiers are edited once; native layout contains only reconstruction slots.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeDescription {
    pub data: TypeData,
    pub native: TypeLayout,
}

/// Missing qualifier fields remain absent instead of acquiring platform defaults.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeData {
    pub alternatives: Vec<TypeAlternative>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_option"
    )]
    pub string: Option<StringQualifiers>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_option"
    )]
    pub number: Option<NumberQualifiers>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_option"
    )]
    pub date: Option<DateQualifiers>,
}

/// Type and `TypeSet` have different native meanings even when their qualified spelling agrees.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeAlternative {
    pub kind: TypeAlternativeKind,
    pub name: String,
}

/// A type alternative's semantic category is independent of its namespace prefix spelling.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeAlternativeKind {
    Type,
    TypeSet,
}

impl TypeAlternativeKind {
    /// Native element names retain the distinction between one type and a declared type set.
    pub(super) const fn tag(self) -> &'static str {
        match self {
            Self::Type => "Type",
            Self::TypeSet => "TypeSet",
        }
    }
}

/// Exact string limits do not conflate missing, zero or a fixed versus variable length.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StringQualifiers {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_option"
    )]
    pub length: Option<ExactUnsigned>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_option"
    )]
    pub allowed_length: Option<TypeAllowedLength>,
}

/// Precision and scale keep native digit spellings without narrowing them to a machine integer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NumberQualifiers {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_option"
    )]
    pub digits: Option<ExactUnsigned>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_option"
    )]
    pub fraction_digits: Option<ExactUnsigned>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_option"
    )]
    pub allowed_sign: Option<TypeAllowedSign>,
}

/// An omitted fraction mode is distinct from an explicit date, time or combined value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DateQualifiers {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_option"
    )]
    pub fractions: Option<TypeDateFractions>,
}

/// Native enum spellings remain locale-independent.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TypeAllowedLength {
    Variable,
    Fixed,
}

impl TypeAllowedLength {
    /// Use the exact platform enum spelling in both native and semantic output.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Variable => "Variable",
            Self::Fixed => "Fixed",
        }
    }
}

/// Native sign restrictions do not infer a default for an absent field.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TypeAllowedSign {
    Any,
    Nonnegative,
}

impl TypeAllowedSign {
    /// Use the exact platform enum spelling without case folding.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Any => "Any",
            Self::Nonnegative => "Nonnegative",
        }
    }
}

/// Date fractions are a closed native vocabulary rather than case-folded free text.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TypeDateFractions {
    Date,
    Time,
    DateTime,
}

impl TypeDateFractions {
    /// Use the exact date fraction spelling without inferring an absent value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Date => "Date",
            Self::Time => "Time",
            Self::DateTime => "DateTime",
        }
    }
}

/// Slots contain no values; their positions reproduce native order even for repeated alternatives.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeLayout {
    pub prefix: String,
    pub parts: Vec<TypePart>,
    pub string_fields: Vec<TypeStringField>,
    pub number_fields: Vec<TypeNumberField>,
    pub date_fields: Vec<TypeDateField>,
}

/// Exactly one slot binds each alternative and each present qualifier group.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "index",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum TypePart {
    Alternative(usize),
    String,
    Number,
    Date,
}

/// Qualifier order has one binding per present value and is kept out of ordinary YAML data.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeStringField {
    Length,
    AllowedLength,
}

/// Native numeric fields may occur in any accepted order without duplicating their values.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeNumberField {
    Digits,
    FractionDigits,
    AllowedSign,
}

/// A date group retains its single native field's explicit presence.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeDateField {
    Fractions,
}

impl TypeDescription {
    /// Read native string fields only after validation has bound each slot to one present value.
    pub(super) fn string_field(&self, field: TypeStringField) -> Option<(&'static str, &str)> {
        let value = self.data.string.as_ref()?;
        Some(match field {
            TypeStringField::Length => ("Length", value.length.as_ref()?.as_str()),
            TypeStringField::AllowedLength => ("AllowedLength", value.allowed_length?.as_str()),
        })
    }

    /// Read native number fields without narrowing exact digit values to machine integers.
    pub(super) fn number_field(&self, field: TypeNumberField) -> Option<(&'static str, &str)> {
        let value = self.data.number.as_ref()?;
        Some(match field {
            TypeNumberField::Digits => ("Digits", value.digits.as_ref()?.as_str()),
            TypeNumberField::FractionDigits => {
                ("FractionDigits", value.fraction_digits.as_ref()?.as_str())
            }
            TypeNumberField::AllowedSign => ("AllowedSign", value.allowed_sign?.as_str()),
        })
    }

    /// Date fields retain their explicit presence and native spelling.
    pub(super) fn date_field(&self, field: TypeDateField) -> Option<(&'static str, &str)> {
        match field {
            TypeDateField::Fractions => Some((
                "DateFractions",
                self.data.date.as_ref()?.fractions?.as_str(),
            )),
        }
    }

    /// Validate complete value/slot bindings and resolve qualified alternatives in the real owner.
    ///
    /// # Errors
    /// Returns an invalid native layout, missing namespace or incompatible type value.
    pub fn validate(&self, context: &[Namespace]) -> Result<(), Error> {
        if self.native.parts.is_empty() {
            return Err(layout_invalid());
        }
        if self.native.prefix.is_empty()
            || !context.iter().any(|binding| {
                binding.prefix.as_deref() == Some(&self.native.prefix)
                    && binding.uri == CORE_NAMESPACE
            })
        {
            return Err(Error::root("source_type_namespace_invalid"));
        }
        let mut alternatives = BTreeSet::new();
        let mut groups = BTreeSet::new();
        for part in &self.native.parts {
            match part {
                TypePart::Alternative(index) => {
                    let alternative = self
                        .data
                        .alternatives
                        .get(*index)
                        .ok_or_else(layout_invalid)?;
                    if !alternatives.insert(*index) {
                        return Err(layout_invalid());
                    }
                    qualified_name(&alternative.name, context)?;
                }
                TypePart::String | TypePart::Number | TypePart::Date => {
                    let name = match part {
                        TypePart::String => "string",
                        TypePart::Number => "number",
                        _ => "date",
                    };
                    if !groups.insert(name) {
                        return Err(layout_invalid());
                    }
                }
            }
        }
        if alternatives.len() != self.data.alternatives.len()
            || groups.contains("string") != self.data.string.is_some()
            || groups.contains("number") != self.data.number.is_some()
            || groups.contains("date") != self.data.date.is_some()
        {
            return Err(layout_invalid());
        }
        self.validate_qualifier_fields()
    }

    /// Slot equality checks exact field presence without changing missing values into defaults.
    fn validate_qualifier_fields(&self) -> Result<(), Error> {
        let string = self.data.string.as_ref().map_or_else(Vec::new, |value| {
            let mut fields = Vec::new();
            if value.length.is_some() {
                fields.push(TypeStringField::Length);
            }
            if value.allowed_length.is_some() {
                fields.push(TypeStringField::AllowedLength);
            }
            fields
        });
        let number = self.data.number.as_ref().map_or_else(Vec::new, |value| {
            let mut fields = Vec::new();
            if value.digits.is_some() {
                fields.push(TypeNumberField::Digits);
            }
            if value.fraction_digits.is_some() {
                fields.push(TypeNumberField::FractionDigits);
            }
            if value.allowed_sign.is_some() {
                fields.push(TypeNumberField::AllowedSign);
            }
            fields
        });
        let date = self
            .data
            .date
            .as_ref()
            .and_then(|value| value.fractions)
            .map_or_else(Vec::new, |_| vec![TypeDateField::Fractions]);
        if !same_fields(&string, &self.native.string_fields)
            || !same_fields(&number, &self.native.number_fields)
            || !same_fields(&date, &self.native.date_fields)
            || self.data.string.is_some() && string.is_empty()
            || self.data.number.is_some() && number.is_empty()
            || self.data.date.is_some() && date.is_empty()
        {
            return Err(layout_invalid());
        }
        Ok(())
    }
}

/// `QName` resolution uses namespace identity, preserving exact spelling for native text output.
pub(super) fn qualified_name<'a>(
    value: &'a str,
    context: &'a [Namespace],
) -> Result<(&'a str, &'a str), Error> {
    let text = value.trim();
    let (prefix, local) = text
        .split_once(':')
        .map_or((None, text), |(prefix, local)| (Some(prefix), local));
    if !ncname(local) || prefix.is_some_and(|prefix| !ncname(prefix)) || value != text {
        return Err(Error::root("source_type_name_invalid"));
    }
    let namespace = context
        .iter()
        .find(|binding| binding.prefix.as_deref() == prefix)
        .map(|binding| binding.uri.as_str())
        .ok_or_else(|| Error::root("source_type_namespace_invalid"))?;
    Ok((namespace, local))
}

/// XML 1.0 Fifth Edition names admit native Unicode identifiers, excluding namespace separators.
fn ncname(value: &str) -> bool {
    let mut characters = value.chars();
    characters.next().is_some_and(name_start) && characters.all(|ch| name_start(ch)
        || matches!(ch, '-' | '.' | '0'..='9' | '\u{b7}' | '\u{300}'..='\u{36f}' | '\u{203f}'..='\u{2040}'))
}

/// Keep the normative `NameStartChar` ranges independent from Unicode letter classifications.
const fn name_start(ch: char) -> bool {
    matches!(ch, 'A'..='Z' | '_' | 'a'..='z' | '\u{c0}'..='\u{d6}' | '\u{d8}'..='\u{f6}'
        | '\u{f8}'..='\u{2ff}' | '\u{370}'..='\u{37d}' | '\u{37f}'..='\u{1fff}'
        | '\u{200c}'..='\u{200d}' | '\u{2070}'..='\u{218f}' | '\u{2c00}'..='\u{2fef}'
        | '\u{3001}'..='\u{d7ff}' | '\u{f900}'..='\u{fdcf}' | '\u{fdf0}'..='\u{fffd}'
        | '\u{10000}'..='\u{effff}')
}

/// A present null is not an absent qualifier and must not acquire platform defaults.
fn present_option<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

/// Exact set membership rejects duplicate slots while allowing the observed native field order.
fn same_fields<T: Ord>(expected: &[T], slots: &[T]) -> bool {
    let expected: BTreeSet<_> = expected.iter().collect();
    let actual: BTreeSet<_> = slots.iter().collect();
    expected == actual && slots.len() == actual.len()
}

/// Missing, duplicated or unbound layout fields are not inferred from the editable data.
fn layout_invalid() -> Error {
    Error::root("source_type_layout_invalid")
}
