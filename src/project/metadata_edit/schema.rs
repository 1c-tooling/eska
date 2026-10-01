//! Reviewed Designer writer mappings over the installed EDT property model.

use roxmltree::Node;
use serde::Serialize;

use super::EditError;
use crate::project::metadata_model::{ObjectId, PropertyKey};

pub(super) const MD: &str = "http://v8.1c.ru/8.3/MDClasses";
pub(super) const CORE: &str = "http://v8.1c.ru/8.1/data/core";
pub(super) const XS: &str = "http://www.w3.org/2001/XMLSchema";
pub(super) const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";
pub(super) const APP: &str = "http://v8.1c.ru/8.2/managed-application/core";
pub(super) const READABLE: &str = "http://v8.1c.ru/8.3/xcf/readable";

/// An editor is emitted only when the property's source shape and domain are known.
#[derive(Clone, Debug, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ScalarSchema {
    Text,
    Boolean,
    Integer {
        min: i64,
        max: i64,
    },
    Decimal {
        nullable: bool,
    },
    Enum {
        domain: String,
        values: Vec<String>,
    },
    DataType {
        key: PropertyKey,
        reference_only: bool,
    },
    Reference {
        domain: String,
        nullable: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        reference_type: Option<PropertyKey>,
    },
    Value {
        key: Option<PropertyKey>,
        types: Vec<super::EditableValueType>,
    },
}

impl ScalarSchema {
    /// Validate a submitted scalar independently of the UI controls.
    pub(super) fn validate(&self, value: &str) -> Result<(), EditError> {
        if value.len() > 1_048_576 {
            return Err(EditError::InvalidValue);
        }
        let valid = match self {
            Self::Text => true,
            Self::Boolean => matches!(value, "true" | "false" | "1" | "0"),
            Self::Integer { min, max } => value
                .parse::<i64>()
                .is_ok_and(|number| (*min..=*max).contains(&number)),
            Self::Decimal { nullable } => (*nullable && value.is_empty()) || decimal(value),
            Self::Enum { values, .. } => values.iter().any(|candidate| candidate == value),
            Self::DataType { .. } | Self::Value { .. } => false,
            Self::Reference {
                domain, nullable, ..
            } => {
                (*nullable && value.is_empty())
                    || super::references::parts(value).is_some_and(|parts| {
                        super::references::target(domain).is_some_and(|(kind, _)| {
                            parts.last().is_some_and(|(target, _)| *target == kind)
                        })
                    })
            }
        };
        valid.then_some(()).ok_or(EditError::InvalidValue)
    }
}

/// XML Schema decimal has neither exponent notation nor non-finite floating point values.
pub(super) fn decimal(value: &str) -> bool {
    let unsigned = value.strip_prefix(['+', '-']).unwrap_or(value);
    let mut digits = 0;
    let mut dots = 0;
    for byte in unsigned.bytes() {
        match byte {
            b'0'..=b'9' => digits += 1,
            b'.' => dots += 1,
            _ => return false,
        }
    }
    digits > 0 && dots <= 1
}

/// The XML tag of inline objects is less specific than their metadata class.
pub(super) fn object_class(id: &ObjectId) -> String {
    let kinds: Vec<_> = id
        .as_str()
        .split('/')
        .filter_map(|part| part.split_once(':').map(|pair| pair.0))
        .collect();
    let current = kinds.last().copied().unwrap_or_default();
    let parent = kinds.iter().rev().nth(1).copied().unwrap_or_default();
    let prefix = if parent == "tabular-section" && current == "attribute" {
        "tabular-section"
    } else if matches!(
        current,
        "attribute" | "dimension" | "resource" | "form" | "command" | "tabular-section"
    ) {
        parent
    } else {
        ""
    };
    [prefix, current]
        .into_iter()
        .filter(|part| !part.is_empty())
        .map(class_part)
        .collect()
}

/// Match acronym spellings used by the installed model instead of lowercasing XML names.
fn class_part(kind: &str) -> String {
    kind.split('-')
        .map(|part| match part {
            "http" => "HTTP".to_owned(),
            "ws" => "WS".to_owned(),
            "xdto" => "XDTO".to_owned(),
            other => {
                let mut chars = other.chars();
                chars.next().map_or_else(String::new, |first| {
                    first.to_uppercase().chain(chars).collect()
                })
            }
        })
        .collect()
}

/// Return the model's declared type only in a matching writer namespace and property position.
pub(super) fn field_type(
    class: &str,
    node: Node<'_, '_>,
    top: bool,
    modern: bool,
) -> Option<&'static str> {
    include_str!("fields.tsv").lines().skip(1).find_map(|line| {
        let mut columns = line.split('\t');
        let owner = columns.next()?;
        let namespace = columns.next()?;
        let name = columns.next()?;
        let kind = columns.next()?;
        let property = columns.next()?;
        let profile = columns.next()?;
        (owner == class
            && namespace == node.tag_name().namespace().unwrap_or_default()
            && name == node.tag_name().name()
            && (!top || property == "1")
            && (modern || profile == "8.3.27"))
            .then_some(kind)
    })
}

/// Read-only identities and payload format switches cannot be changed as scalar properties.
pub(super) fn protected(name: &str) -> bool {
    matches!(
        name,
        "Name"
            | "Id"
            | "LastId"
            | "TypeCode"
            | "LinkItem"
            | "Uuid"
            | "ObjectBelonging"
            | "ExtendedConfigurationObject"
            | "ConfigurationExtensionPurpose"
            | "TemplateType"
            | "FormType"
            | "LanguageCode"
    )
}

/// Use semantic annotations only from their actual namespace binding.
pub(super) fn scalar(
    node: Node<'_, '_>,
    model_type: Option<&str>,
    modern: bool,
) -> Option<ScalarSchema> {
    let model_type = model_type?;
    if model_type == "Value" && super::values::number_bound(node) {
        return Some(ScalarSchema::Decimal { nullable: true });
    }
    if model_type == "Value" && node.tag_name().name() == "FillValue" {
        return super::value_schema::schema(node);
    }
    if node.children().any(|child| child.is_element())
        || node
            .attribute((XSI, "nil"))
            .is_some_and(|value| matches!(value, "true" | "1"))
    {
        return None;
    }
    // These Role-valued fields serialize a list wrapper even when it is empty.
    if model_type == "Role"
        && node.tag_name().namespace() == Some(MD)
        && matches!(
            node.tag_name().name(),
            "DefaultRoles" | "StandaloneConfigurationRestrictionRoles"
        )
    {
        return None;
    }
    if super::references::target(model_type).is_some() {
        if node.attribute((XSI, "type")).is_some_and(|annotation| {
            let (prefix, name) = annotation
                .split_once(':')
                .map_or((None, annotation), |(prefix, name)| (Some(prefix), name));
            name != "MDObjectRef" || node.lookup_namespace_uri(prefix) != Some(READABLE)
        }) {
            return None;
        }
        return Some(ScalarSchema::Reference {
            domain: model_type.to_owned(),
            nullable: !node.has_tag_name((READABLE, "Item")),
            reference_type: if model_type == "BasicForm" && node.tag_name().name() == "ChoiceForm" {
                choice_form_type(node)
            } else {
                None
            },
        });
    }
    if let Some(annotation) = node.attribute((XSI, "type")) {
        if model_type != "Value" {
            return None;
        }
        let (prefix, name) = annotation
            .split_once(':')
            .map_or((None, annotation), |(prefix, name)| (Some(prefix), name));
        if node.lookup_namespace_uri(prefix) != Some(XS) {
            return None;
        }
        return match name {
            "string" => Some(ScalarSchema::Text),
            "boolean" => Some(ScalarSchema::Boolean),
            "int" => Some(ScalarSchema::Integer {
                min: i64::from(i32::MIN),
                max: i64::from(i32::MAX),
            }),
            "decimal" => Some(ScalarSchema::Decimal { nullable: false }),
            _ => None,
        };
    }
    let mut result = schema(model_type, modern)?;
    if let ScalarSchema::Integer { min, max } = &mut result {
        match (
            node.tag_name().namespace(),
            node.parent().map(|parent| parent.tag_name().name()),
            node.tag_name().name(),
        ) {
            (Some(CORE), Some("NumberQualifiers"), "Digits") => {
                *min = sibling_number(node, "FractionDigits").unwrap_or(0).max(1);
                *max = 38;
            }
            (Some(CORE), Some("NumberQualifiers"), "FractionDigits") => {
                *min = 0;
                *max = sibling_number(node, "Digits")?;
            }
            (Some(CORE), Some("StringQualifiers" | "BinaryDataQualifiers"), "Length") => {
                *min = 0;
            }
            (Some(MD), Some("Properties"), name) => {
                (*min, *max) = metadata_integer(node, name)?;
            }
            _ => return None,
        }
    }
    Some(result)
}

/// Installed EDT editors constrain these fields independently of other metadata properties.
fn metadata_integer(node: Node<'_, '_>, name: &str) -> Option<(i64, i64)> {
    let owner = node.parent()?.parent()?;
    match (owner.tag_name().namespace(), owner.tag_name().name(), name) {
        (Some(MD), "TabularSection", "LineNumberLength") => Some((5, 9)),
        (Some(MD), "ScheduledJob", "RestartCountOnFailure" | "RestartIntervalOnFailure") => {
            Some((0, 1_000_000))
        }
        _ => None,
    }
}

/// EDT's `ReferenceMdFormContentProvider` offers forms only for one concrete metadata reference type.
pub(super) fn choice_form_type(node: Node<'_, '_>) -> Option<PropertyKey> {
    if node.parent()?.has_tag_name((READABLE, "StandardAttribute")) {
        return super::standard::reference_type(node);
    }
    let description = node
        .parent()?
        .children()
        .find(|child| child.has_tag_name((MD, "Type")))?;
    if description
        .children()
        .any(|child| child.has_tag_name((CORE, "TypeSet")))
    {
        return None;
    }
    let mut types = description
        .children()
        .filter(|child| child.has_tag_name((CORE, "Type")));
    let key = super::types::key(types.next()?)?;
    (types.next().is_none()
        && key.namespace.as_deref() == Some(super::types::CFG)
        && super::types::supported(&key))
    .then_some(key)
}

/// Sibling constraints are reflected in the advertised schema as well as checked after patching.
fn sibling_number(node: Node<'_, '_>, name: &str) -> Option<i64> {
    node.parent()?
        .children()
        .find(|child| child.has_tag_name((CORE, name)))?
        .text()?
        .parse()
        .ok()
}

/// Domain tokens remain machine-readable; translations are added by the IDE adapter.
pub(super) fn schema(model_type: &str, modern: bool) -> Option<ScalarSchema> {
    match model_type {
        "EString" => Some(ScalarSchema::Text),
        "EBoolean" => Some(ScalarSchema::Boolean),
        "EInt" => Some(ScalarSchema::Integer {
            min: i64::from(i32::MIN),
            max: i64::from(i32::MAX),
        }),
        "ELong" => Some(ScalarSchema::Integer {
            min: i64::MIN,
            max: i64::MAX,
        }),
        "EBigDecimal" => Some(ScalarSchema::Decimal { nullable: false }),
        domain => {
            let values: Vec<_> = include_str!("enums.tsv")
                .lines()
                .skip(1)
                .filter_map(|line| line.split_once('\t'))
                .filter(|(kind, value)| {
                    *kind == domain
                        && (modern
                            || !matches!(
                                *value,
                                "Version8_5_1"
                                    | "Version8_5EnableTaxi"
                                    | "Version8_5"
                                    | "TaxiEnableVersion8_5"
                            ))
                })
                .map(|(_, value)| value.to_owned())
                .collect();
            (!values.is_empty()).then(|| ScalarSchema::Enum {
                domain: domain.to_owned(),
                values,
            })
        }
    }
}

/// Hand-written writers rename nested fields; these bindings are verified against Designer XML.
pub(super) fn nested_type(
    root: &str,
    parent: Node<'_, '_>,
    node: Node<'_, '_>,
) -> Option<&'static str> {
    let namespace = node.tag_name().namespace()?;
    let name = node.tag_name().name();
    match (namespace, parent.tag_name().name(), name) {
        (CORE, "item", "content") if parent.has_tag_name((CORE, "item")) => Some("EString"),
        (CORE, "StringQualifiers" | "BinaryDataQualifiers", "Length")
        | (CORE, "NumberQualifiers", "Digits" | "FractionDigits") => Some("EInt"),
        (CORE, "StringQualifiers" | "BinaryDataQualifiers", "AllowedLength") => {
            Some("AllowedLength")
        }
        (CORE, "NumberQualifiers", "AllowedSign") => Some("AllowedSign"),
        (CORE, "DateQualifiers", "DateFractions") => Some("DateFractions"),
        (APP, _, "use")
            if matches!(
                root,
                "UsedMobileApplicationFunctionalities"
                    | "RequiredMobileApplicationPermissions"
                    | "RequiredMobileApplicationPermissions8315"
            ) =>
        {
            Some("EBoolean")
        }
        _ => None,
    }
}
