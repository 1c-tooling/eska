//! Standard fields derive from the catalogue, while source properties control their presence/types.

use bsl_metadata::{AttributeType, MdoType, metadata_object::Attribute};
use bsl_platform::{
    AttrValueKind, MdoTemplateKind, ObjectView, PresenceCondition, StandardKind,
    standard_attributes_for,
};
use eska::project_core::metadata_parser::ParsedObject;

use crate::{AdapterError, AdapterIssue, properties::Properties};

struct Settings {
    code_length: Option<u32>,
    code_numeric: bool,
    number_length: Option<u32>,
    number_numeric: bool,
    description_length: Option<u32>,
    hierarchical: bool,
    calculation_dependency: CalculationDependency,
    owners: Vec<AttributeType>,
}

impl Settings {
    /// Parse explicit settings strictly even when the present catalogue does not use a setting.
    fn read(object: &ParsedObject) -> Result<Self, AdapterError> {
        let fields = Properties::new(object)?;
        let owners = fields
            .references("Owners", "Item")?
            .into_iter()
            .map(|value| {
                let (mdo_type, name) = reference(object, "Owners", value)?;
                Ok(AttributeType::Ref { mdo_type, name })
            })
            .collect::<Result<_, AdapterError>>()?;
        Ok(Self {
            code_length: fields.unsigned("CodeLength")?,
            code_numeric: numeric_kind(&fields, "CodeType")?,
            number_length: fields.unsigned("NumberLength")?,
            number_numeric: numeric_kind(&fields, "NumberType")?,
            description_length: fields.unsigned("DescriptionLength")?,
            hierarchical: fields.boolean("Hierarchical")?.unwrap_or(false),
            calculation_dependency: calculation_dependency(object)?,
            owners,
        })
    }

    /// Supported conditions mirror the catalogue; future conditions cause an explicit failure.
    fn present(&self, condition: PresenceCondition) -> Option<bool> {
        match condition {
            PresenceCondition::Always => Some(true),
            PresenceCondition::HasCode => Some(self.code_length.is_some_and(|length| length > 0)),
            PresenceCondition::HasDescription => {
                Some(self.description_length.is_some_and(|length| length > 0))
            }
            PresenceCondition::HasNumber => {
                Some(self.number_length.is_some_and(|length| length > 0))
            }
            PresenceCondition::Hierarchical => Some(self.hierarchical),
            PresenceCondition::HasOwners => Some(!self.owners.is_empty()),
            PresenceCondition::DependsOnCalculationTypes => {
                Some(self.calculation_dependency.is_dependent())
            }
            PresenceCondition::IsPeriodic => None,
        }
    }
}

/// Keep the three source modes distinct even though the current kernel only queries dependency presence.
pub(super) enum CalculationDependency {
    Independent,
    ActionPeriod,
    RegistrationPeriod,
}

impl CalculationDependency {
    /// Both period-based modes require the implicit dependency fields and sections.
    pub(super) const fn is_dependent(&self) -> bool {
        !matches!(self, Self::Independent)
    }
}

/// Only platform dependency modes can decide implicit fields/sections; unknown tokens are errors.
pub(super) fn calculation_dependency(
    object: &ParsedObject,
) -> Result<CalculationDependency, AdapterError> {
    let fields = Properties::new(object)?;
    let Some(field) = fields.field("DependenceOnCalculationTypes")? else {
        return Ok(CalculationDependency::Independent);
    };
    match fields.text("DependenceOnCalculationTypes")? {
        Some("DontUse") => Ok(CalculationDependency::Independent),
        Some("OnActionPeriod") => Ok(CalculationDependency::ActionPeriod),
        Some("OnRegistrationPeriod") => Ok(CalculationDependency::RegistrationPeriod),
        _ => Err(Properties::invalid(field)),
    }
}

/// Reconstruct catalogue-provided fields in the same order as the native kernel model.
pub(super) fn attributes(
    object: &ParsedObject,
    kind: MdoType,
    template: MdoTemplateKind,
) -> Result<Vec<Attribute>, AdapterError> {
    let settings = Settings::read(object)?;
    let mut result = Vec::new();
    for spec in standard_attributes_for(template, ObjectView::Object) {
        let unsupported = || AdapterError {
            issue: AdapterIssue::UnsupportedStandardAttribute {
                name: spec.kind.english_name().into(),
            },
            range: object.range.clone(),
        };
        if !settings.present(spec.condition).ok_or_else(unsupported)? {
            continue;
        }
        let attr_type = match spec.value {
            AttrValueKind::Boolean => AttributeType::Boolean,
            AttrValueKind::DateTime => AttributeType::DateTime,
            AttrValueKind::StringUnbounded => AttributeType::String { length: None },
            AttrValueKind::StringCodeOrDescription => {
                let (length, numeric) = match spec.kind {
                    StandardKind::Code => (settings.code_length, settings.code_numeric),
                    StandardKind::Description => (settings.description_length, false),
                    _ => return Err(unsupported()),
                };
                code_type(length, numeric)
            }
            AttrValueKind::StringNumber => {
                code_type(settings.number_length, settings.number_numeric)
            }
            AttrValueKind::SelfRef => AttributeType::Ref {
                mdo_type: kind,
                name: object.metadata.name().into(),
            },
            AttrValueKind::OwnerRef => match settings.owners.as_slice() {
                [] => AttributeType::Unknown,
                [owner] => owner.clone(),
                owners => AttributeType::Composite {
                    types: owners.to_vec(),
                },
            },
            AttrValueKind::AnyDocumentRef => AttributeType::AnyObjectRef {
                mdo_type: MdoType::Document,
            },
            AttrValueKind::NumberLineNumber => AttributeType::Number {
                precision: 10,
                scale: 0,
            },
            AttrValueKind::TypeDescription => AttributeType::PlatformNamed("ОписаниеТипов".into()),
            AttrValueKind::Unknown => AttributeType::Unknown,
        };
        result.push(Attribute {
            name: spec.kind.russian_name().into(),
            name_en: Some(spec.kind.english_name().into()),
            attr_type,
        });
    }
    if kind == MdoType::ChartOfAccounts {
        // These names are account-specific, not global standard names: other objects may declare a user field named «Вид».
        for (name, name_en, attr_type) in [
            (
                "Вид",
                "Type",
                AttributeType::PlatformNamed("ВидСчета".into()),
            ),
            ("Забалансовый", "OffBalance", AttributeType::Boolean),
        ] {
            result.push(Attribute {
                name: name.into(),
                name_en: Some(name_en.into()),
                attr_type,
            });
        }
    }
    Ok(result)
}

/// Numeric codes follow the kernel's bounded precision; source lengths themselves remain exact.
fn code_type(length: Option<u32>, numeric: bool) -> AttributeType {
    if numeric {
        AttributeType::Number {
            precision: u8::try_from(length.unwrap_or(0)).unwrap_or(u8::MAX),
            scale: 0,
        }
    } else {
        AttributeType::String { length }
    }
}

/// Missing code/number type means the platform default; invalid explicit tokens do not.
fn numeric_kind(fields: &Properties<'_>, name: &str) -> Result<bool, AdapterError> {
    let Some(field) = fields.field(name)? else {
        return Ok(false);
    };
    match fields.text(name)? {
        Some("String") => Ok(false),
        Some("Number") => Ok(true),
        _ => Err(Properties::invalid(field)),
    }
}

/// Validate every metadata reference rather than quietly skipping unknown or malformed items.
pub(super) fn reference(
    object: &ParsedObject,
    property: &str,
    value: &str,
) -> Result<(MdoType, String), AdapterError> {
    let invalid = || invalid_reference(object, property);
    let (prefix, name) = value.trim().split_once('.').ok_or_else(invalid)?;
    if name.is_empty() || name.contains('.') || name.contains(char::is_whitespace) {
        return Err(invalid());
    }
    let kind = prefix.parse().map_err(|_| invalid())?;
    Ok((kind, name.into()))
}

/// Report a list-item failure at the complete owning property in both XML and opaque YAML fields.
pub(super) fn invalid_reference(object: &ParsedObject, name: &str) -> AdapterError {
    let range = object
        .properties
        .as_deref()
        .unwrap_or_default()
        .iter()
        .find(|field| {
            field.property.key.namespace.as_deref()
                == Some(eska::project_core::source_format::MD_NAMESPACE)
                && field.property.key.name == name
        })
        .map_or_else(|| object.range.clone(), |field| field.range.clone());
    AdapterError {
        issue: AdapterIssue::InvalidProperty { name: name.into() },
        range,
    }
}
