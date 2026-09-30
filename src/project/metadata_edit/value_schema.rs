//! The value editor uses the property's declared types and qualifiers, never arbitrary XML types.

use roxmltree::Node;
use serde::Serialize;

use super::{
    EditError, ScalarSchema,
    schema::{CORE, MD, READABLE, XS, XSI},
    types,
};
use crate::project::metadata_model::PropertyKey;

/// One allowed type of an existing value, including constraints required by its owning attribute.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditableValueType {
    pub key: PropertyKey,
    pub constraints: ValueConstraints,
}

/// Primitive constraints retain platform precision and date fractions without floating-point loss.
#[derive(Clone, Debug, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ValueConstraints {
    String {
        max_length: u32,
    },
    Number {
        digits: u32,
        fraction_digits: u32,
        nonnegative: bool,
    },
    Boolean,
    Date {
        fractions: String,
    },
    Reference,
}

impl EditableValueType {
    /// Reject values outside the advertised type before constructing XML replacements.
    pub(super) fn validate(&self, value: &str) -> Result<(), EditError> {
        if value.len() > 1_048_576 {
            return Err(EditError::InvalidValue);
        }
        let valid = match &self.constraints {
            ValueConstraints::String { max_length } => {
                *max_length == 0 || value.chars().count() <= *max_length as usize
            }
            ValueConstraints::Number {
                digits,
                fraction_digits,
                nonnegative,
            } => {
                let unsigned = value.trim_start_matches(['+', '-']);
                let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
                super::schema::decimal(value)
                    && (!nonnegative || !value.starts_with('-'))
                    && fraction.len() <= *fraction_digits as usize
                    && whole.trim_start_matches('0').len() <= (digits - fraction_digits) as usize
            }
            ValueConstraints::Boolean => matches!(value, "true" | "false" | "1" | "0"),
            ValueConstraints::Date { fractions } => date(value, fractions),
            ValueConstraints::Reference => !value.is_empty(), // ProjectSession validates membership.
        };
        valid.then_some(()).ok_or(EditError::InvalidValue)
    }
}

/// Changing an attribute type must not leave an incompatible filling value or choice form behind.
pub(super) fn validate_dependents(description: Node<'_, '_>) -> Result<(), EditError> {
    let properties = description.parent().ok_or(EditError::InvalidXml)?;
    for property in properties
        .children()
        .filter(|child| child.tag_name().namespace() == Some(MD))
    {
        let compatible = match property.tag_name().name() {
            "FillValue" => compatible(property),
            "ChoiceForm" if !property.text().unwrap_or_default().trim().is_empty() => {
                super::schema::choice_form_type(property)
                    .and_then(|key| reference_prefix(&key))
                    .is_some_and(|prefix| {
                        property
                            .text()
                            .unwrap_or_default()
                            .starts_with(&format!("{prefix}Form."))
                    })
            }
            _ => true,
        };
        if !compatible {
            return Err(EditError::IncompatibleProperty(PropertyKey {
                namespace: Some(MD.to_owned()),
                name: property.tag_name().name().to_owned(),
            }));
        }
    }
    Ok(())
}

/// Both declared and implicit types must still admit a saved filling value after its constraints change.
pub(super) fn compatible(property: Node<'_, '_>) -> bool {
    if property
        .attribute((XSI, "nil"))
        .is_some_and(|value| matches!(value, "true" | "1"))
    {
        return true;
    }
    let Some(ScalarSchema::Value {
        key: Some(key),
        types,
    }) = schema(property)
    else {
        return false;
    };
    types
        .iter()
        .find(|choice| choice.key == key)
        .is_some_and(|choice| choice.validate(property.text().unwrap_or_default()).is_ok())
}

/// Resolve an attribute's supported value types while preserving nil independently of empty string.
pub(super) fn schema(node: Node<'_, '_>) -> Option<ScalarSchema> {
    if !matches!(node.tag_name().namespace(), Some(MD | READABLE))
        || node.tag_name().name() != "FillValue"
        || node
            .children()
            .any(|child| child.is_element() || child.is_pi())
    {
        return None;
    }
    let parent = node.parent()?;
    let choices: Vec<_> = if parent.has_tag_name((READABLE, "StandardAttribute")) {
        super::standard::value_types(node)?
    } else if parent.has_tag_name((MD, "Properties")) {
        let description = parent
            .children()
            .find(|child| child.has_tag_name((MD, "Type")))?;
        description
            .children()
            .filter(|child| child.has_tag_name((CORE, "Type")))
            .filter_map(|child| value_type(types::key(child)?, description))
            .collect()
    } else {
        return None;
    };
    if choices.is_empty() {
        return None;
    }
    let key = if node
        .attribute((XSI, "nil"))
        .is_some_and(|value| matches!(value, "true" | "1"))
    {
        None
    } else {
        let annotation = annotation(node)?;
        if annotation.namespace.as_deref() == Some(READABLE) && annotation.name == "DesignTimeRef" {
            let value = node.text().unwrap_or_default();
            Some(
                choices
                    .iter()
                    .find(|choice| {
                        reference_prefix(&choice.key)
                            .is_some_and(|prefix| value.starts_with(&prefix))
                    })
                    .map_or(annotation, |choice| choice.key.clone()),
            )
        } else {
            Some(annotation)
        }
    };
    Some(ScalarSchema::Value {
        key,
        types: choices,
    })
}

/// The stored annotation may be an alias; all decisions use its expanded `QName`.
fn annotation(node: Node<'_, '_>) -> Option<PropertyKey> {
    let name = node.attribute((XSI, "type"))?;
    let (prefix, local) = name
        .split_once(':')
        .map_or((None, name), |(prefix, local)| (Some(prefix), local));
    Some(PropertyKey {
        namespace: node.lookup_namespace_uri(prefix).map(str::to_owned),
        name: local.to_owned(),
    })
}

/// Only reviewed primitive writers and declared generated reference types are value editors.
fn value_type(key: PropertyKey, description: Node<'_, '_>) -> Option<EditableValueType> {
    let child_text = |group, name| {
        description
            .children()
            .find(|child| child.has_tag_name((CORE, group)))?
            .children()
            .find(|child| child.has_tag_name((CORE, name)))?
            .text()
    };
    let number = |group, name, default| {
        child_text(group, name).map_or(Some(default), |text| text.parse::<u32>().ok())
    };
    let constraints = if key.namespace.as_deref() == Some(types::CFG) && types::supported(&key) {
        ValueConstraints::Reference
    } else if key.namespace.as_deref() == Some(XS) {
        match key.name.as_str() {
            "string" => ValueConstraints::String {
                max_length: number("StringQualifiers", "Length", 0)?,
            },
            "decimal" => {
                let digits = number("NumberQualifiers", "Digits", 10)?;
                let fraction_digits = number("NumberQualifiers", "FractionDigits", 0)?;
                if digits == 0 || digits > 38 || fraction_digits > digits {
                    return None;
                }
                ValueConstraints::Number {
                    digits,
                    fraction_digits,
                    nonnegative: child_text("NumberQualifiers", "AllowedSign")
                        == Some("Nonnegative"),
                }
            }
            "boolean" => ValueConstraints::Boolean,
            "dateTime" => {
                let fractions = child_text("DateQualifiers", "DateFractions").unwrap_or("DateTime");
                if !matches!(fractions, "DateTime" | "Date" | "Time") {
                    return None;
                }
                ValueConstraints::Date {
                    fractions: fractions.to_owned(),
                }
            }
            _ => return None,
        }
    } else {
        return None;
    };
    Some(EditableValueType { key, constraints })
}

/// Generated reference types serialize design-time values using the object's Designer tag.
pub fn reference_prefix(key: &PropertyKey) -> Option<String> {
    if key.namespace.as_deref() != Some(types::CFG) {
        return None;
    }
    let (family, name) = key.name.split_once('.')?;
    let tag = types::REFERENCE_TYPES
        .iter()
        .find_map(|(prefix, tag)| (*prefix == family).then_some(*tag))?;
    Some(format!("{tag}.{name}."))
}

/// 1C date values are timezone-free calendar values with second precision and years 1 through 9999.
fn date(value: &str, fractions: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 19
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
    {
        return false;
    }
    let number = |start, end| value.get(start..end)?.parse::<u32>().ok();
    if bytes
        .iter()
        .enumerate()
        .any(|(index, byte)| ![4, 7, 10, 13, 16].contains(&index) && !byte.is_ascii_digit())
    {
        return false;
    }
    let (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second)) = (
        number(0, 4),
        number(5, 7),
        number(8, 10),
        number(11, 13),
        number(14, 16),
        number(17, 19),
    ) else {
        return false;
    };
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let max_day = match month {
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => return false,
    };
    year > 0
        && (1..=max_day).contains(&day)
        && hour < 24
        && minute < 60
        && second < 60
        && (fractions != "Date" || (hour == 0 && minute == 0 && second == 0))
        && (fractions != "Time" || (year == 1 && month == 1 && day == 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Precision checks operate on decimal text, including all 38 supported digits.
    #[test]
    fn numbers_preserve_precision_and_reject_out_of_range_values() {
        let field = EditableValueType {
            key: PropertyKey {
                namespace: Some(XS.into()),
                name: "decimal".into(),
            },
            constraints: ValueConstraints::Number {
                digits: 38,
                fraction_digits: 2,
                nonnegative: false,
            },
        };
        for valid in [
            "999999999999999999999999999999999999.99",
            "-0.50",
            "+123.00",
            "0000.00",
        ] {
            assert!(field.validate(valid).is_ok(), "{valid}");
        }
        for invalid in [
            "9999999999999999999999999999999999999.99",
            "0.001",
            "1e5",
            "NaN",
            "--1",
            ".",
            "",
        ] {
            assert!(field.validate(invalid).is_err(), "{invalid}");
        }
    }

    /// A valid ISO-looking string must still represent an actual calendar value and fraction.
    #[test]
    fn dates_validate_calendar_and_declared_fractions() {
        for value in [
            "2024-02-29T12:00:00",
            "2000-02-29T00:00:00",
            "9999-12-31T23:59:59",
        ] {
            assert!(date(value, "DateTime"), "{value}");
        }
        for value in [
            "1900-02-29T00:00:00",
            "0000-01-01T00:00:00",
            "2024-01-32T00:00:00",
            "2024-02-29T24:00:00",
            "+024-02-29T00:00:00",
            "2024-+2-01T00:00:00",
        ] {
            assert!(!date(value, "DateTime"), "{value}");
        }
        assert!(date("2024-02-29T00:00:00", "Date"));
        assert!(!date("2024-02-29T01:00:00", "Date"));
        assert!(date("0001-01-01T23:59:59", "Time"));
        assert!(!date("2024-01-01T23:59:59", "Time"));
    }
}
