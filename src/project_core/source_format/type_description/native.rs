//! Only complete inverse rules promote a native type; unknown content stays opaque as a whole.

use roxmltree::Node;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use super::{
    CORE_NAMESPACE, DateQualifiers, NumberQualifiers, StringQualifiers, TypeAlternative,
    TypeAlternativeKind, TypeData, TypeDateField, TypeDescription, TypeLayout, TypeNumberField,
    TypePart, TypeStringField,
};
use crate::project_core::source_format::{
    Namespace,
    xml::{plain_element_name, typed_elements},
};

/// Decode only fields whose namespace, order, absence and lexical value have a known inverse.
pub(in crate::project_core::source_format) fn decode(
    node: Node<'_, '_>,
    input: &str,
    context: &[Namespace],
) -> Option<TypeDescription> {
    plain_element_name(node, input)?;
    let children = typed_elements(node)?;
    let first = *children.first()?;
    let prefix = plain_element_name(first, input)?
        .split_once(':')?
        .0
        .to_owned();
    let mut result = TypeDescription {
        data: TypeData {
            alternatives: Vec::new(),
            string: None,
            number: None,
            date: None,
        },
        native: TypeLayout {
            prefix,
            parts: Vec::new(),
            string_fields: Vec::new(),
            number_fields: Vec::new(),
            date_fields: Vec::new(),
        },
    };
    for child in children {
        let tag = core_name(child, input, &result.native.prefix)?;
        match tag {
            "Type" | "TypeSet" => {
                let name = leaf(child)?;
                let index = result.data.alternatives.len();
                result.data.alternatives.push(TypeAlternative {
                    kind: if tag == "Type" {
                        TypeAlternativeKind::Type
                    } else {
                        TypeAlternativeKind::TypeSet
                    },
                    name,
                });
                result.native.parts.push(TypePart::Alternative(index));
            }
            "StringQualifiers" if result.data.string.is_none() => {
                let (value, fields) = qualifiers::<StringQualifiers>(
                    child,
                    input,
                    &result.native.prefix,
                    &[("Length", "length"), ("AllowedLength", "allowed_length")],
                )?;
                result.data.string = Some(value);
                result.native.string_fields = fields
                    .into_iter()
                    .map(|field| {
                        if field == "length" {
                            TypeStringField::Length
                        } else {
                            TypeStringField::AllowedLength
                        }
                    })
                    .collect();
                result.native.parts.push(TypePart::String);
            }
            "NumberQualifiers" if result.data.number.is_none() => {
                let (value, fields) = qualifiers::<NumberQualifiers>(
                    child,
                    input,
                    &result.native.prefix,
                    &[
                        ("Digits", "digits"),
                        ("FractionDigits", "fraction_digits"),
                        ("AllowedSign", "allowed_sign"),
                    ],
                )?;
                result.data.number = Some(value);
                result.native.number_fields = fields
                    .into_iter()
                    .map(|field| match field {
                        "digits" => TypeNumberField::Digits,
                        "fraction_digits" => TypeNumberField::FractionDigits,
                        _ => TypeNumberField::AllowedSign,
                    })
                    .collect();
                result.native.parts.push(TypePart::Number);
            }
            "DateQualifiers" if result.data.date.is_none() => {
                let (value, _) = qualifiers::<DateQualifiers>(
                    child,
                    input,
                    &result.native.prefix,
                    &[("DateFractions", "fractions")],
                )?;
                result.data.date = Some(value);
                result.native.date_fields = vec![TypeDateField::Fractions];
                result.native.parts.push(TypePart::Date);
            }
            _ => return None,
        }
    }
    result.validate(context).ok()?;
    Some(result)
}

/// Declared qualifier vocabularies map exact text to typed fields; no unknown field is discarded.
fn qualifiers<T: DeserializeOwned>(
    node: Node<'_, '_>,
    input: &str,
    prefix: &str,
    names: &[(&'static str, &'static str)],
) -> Option<(T, Vec<&'static str>)> {
    let mut values = Map::new();
    let mut order = Vec::new();
    for field in typed_elements(node)? {
        let name = core_name(field, input, prefix)?;
        let (_, key) = names.iter().find(|(tag, _)| *tag == name)?;
        if values
            .insert((*key).into(), Value::String(leaf(field)?))
            .is_some()
        {
            return None;
        }
        order.push(*key);
    }
    Some((serde_json::from_value(Value::Object(values)).ok()?, order))
}

/// URI identity and exact native prefix spelling are both required by the stored layout.
fn core_name<'a>(node: Node<'a, '_>, input: &str, prefix: &str) -> Option<&'a str> {
    let (actual, _) = plain_element_name(node, input)?.split_once(':')?;
    (node.tag_name().namespace() == Some(CORE_NAMESPACE) && actual == prefix)
        .then(|| node.tag_name().name())
}

/// Qualifier and `QName` leaves are exact nonempty text without nested annotations or attributes.
fn leaf(node: Node<'_, '_>) -> Option<String> {
    if !node.children().all(|child| child.is_text()) {
        return None;
    }
    let text: String = node.children().filter_map(|child| child.text()).collect();
    (!text.is_empty()).then_some(text)
}
