//! Registers preserve real field identities and use the common type-description converter.

#[cfg(test)]
mod tests;

use bsl_metadata::{
    AttributeType, Dimension, MdoType, Register, Uuid,
    register::{
        AccumulationRegisterType, RegisterAttribute, RegisterPeriodicity, RegisterResource,
    },
};
use bsl_platform::{
    AttrValueKind, MdoTemplateKind, ObjectView, PresenceCondition, standard_attributes_for,
};
use eska::project_core::{
    MetadataKind,
    metadata_parser::{ParsedDescriptor, ParsedObject},
};

use crate::{AdapterError, AdapterIssue, attribute_type, identity, properties::Properties};

/// Convert one complete register descriptor, including direct fields in declaration order.
///
/// # Errors
/// Returns an incomplete/wrong descriptor or invalid field identity, flags, enums and types.
pub fn register(descriptor: &ParsedDescriptor) -> Result<Register, AdapterError> {
    if let Some(issue) = descriptor.diagnostics.first() {
        return Err(AdapterError {
            issue: AdapterIssue::IncompleteDescriptor,
            range: issue.range.clone(),
        });
    }
    let root = descriptor.objects.first().ok_or(AdapterError {
        issue: AdapterIssue::MissingRoot,
        range: 0..0,
    })?;
    let kind = match root.metadata.kind() {
        MetadataKind::InformationRegister => MdoType::InformationRegister,
        MetadataKind::AccumulationRegister => MdoType::AccumulationRegister,
        MetadataKind::AccountingRegister => MdoType::AccountingRegister,
        MetadataKind::CalculationRegister => MdoType::CalculationRegister,
        actual => {
            return Err(AdapterError {
                issue: AdapterIssue::WrongKind { actual },
                range: root.range.clone(),
            });
        }
    };
    let fields = Properties::new(root)?;
    let periodicity = periodicity(&fields, kind)?;
    let mut builder = Register::builder()
        .uuid(identity::uuid(root)?)
        .name(root.metadata.name())
        .mdo_type(kind)
        .periodicity(periodicity)
        .register_type(register_type(&fields, kind)?)
        .recorder_subordinate(recorder_subordinate(&fields, kind)?)
        .enable_totals_slice_first(fields.boolean("EnableTotalsSliceFirst")?.unwrap_or(false))
        .enable_totals_slice_last(fields.boolean("EnableTotalsSliceLast")?.unwrap_or(false))
        .attributes(standard_attributes(root, kind, periodicity)?);
    for child in descriptor
        .objects
        .iter()
        .filter(|child| child.metadata.parent() == Some(root.metadata.id()))
    {
        builder = match child.metadata.kind() {
            MetadataKind::Dimension => builder.add_dimension(dimension(child)?),
            MetadataKind::Resource => builder.add_resource(resource(child)?),
            MetadataKind::Attribute => builder.add_attribute(attribute(child)?),
            _ => builder,
        };
    }
    Ok(builder.build())
}

/// Periodicity affects standard fields; unknown explicit values cannot act as nonperiodical.
fn periodicity(
    fields: &Properties<'_>,
    kind: MdoType,
) -> Result<Option<RegisterPeriodicity>, AdapterError> {
    if kind != MdoType::InformationRegister {
        return Ok(None);
    }
    fields
        .field("InformationRegisterPeriodicity")?
        .map(
            |field| match fields.text("InformationRegisterPeriodicity")? {
                Some("Nonperiodical") => Ok(RegisterPeriodicity::Nonperiodical),
                Some("Second") => Ok(RegisterPeriodicity::Second),
                Some("Day") => Ok(RegisterPeriodicity::Day),
                Some("Month") => Ok(RegisterPeriodicity::Month),
                Some("RecorderPosition") => Ok(RegisterPeriodicity::RecorderPosition),
                Some(value @ ("Quarter" | "Year")) => Err(AdapterError {
                    issue: AdapterIssue::UnsupportedPropertyValue {
                        name: "InformationRegisterPeriodicity".into(),
                        value: value.into(),
                    },
                    range: field.range.clone(),
                }),
                _ => Err(Properties::invalid(field)),
            },
        )
        .transpose()
}

/// Register totals semantics are represented only for accumulation registers in this kernel.
fn register_type(
    fields: &Properties<'_>,
    kind: MdoType,
) -> Result<Option<AccumulationRegisterType>, AdapterError> {
    if kind != MdoType::AccumulationRegister {
        return Ok(None);
    }
    fields
        .field("RegisterType")?
        .map(|field| match fields.text("RegisterType")? {
            Some("Balance") => Ok(AccumulationRegisterType::Balance),
            Some("Turnovers") => Ok(AccumulationRegisterType::Turnovers),
            Some("BalanceAndTurnovers") => Ok(AccumulationRegisterType::BalanceAndTurnovers),
            _ => Err(Properties::invalid(field)),
        })
        .transpose()
}

/// Absence means independent in the selected model; invalid explicit mode is still a failure.
fn recorder_subordinate(fields: &Properties<'_>, kind: MdoType) -> Result<bool, AdapterError> {
    if kind != MdoType::InformationRegister {
        return Ok(false);
    }
    let Some(field) = fields.field("WriteMode")? else {
        return Ok(false);
    };
    match fields.text("WriteMode")? {
        Some("Independent") => Ok(false),
        Some("RecorderSubordinate") => Ok(true),
        _ => Err(Properties::invalid(field)),
    }
}

/// Use the shared platform specifications; nil identifies implicit fields, never source objects.
fn standard_attributes(
    root: &ParsedObject,
    kind: MdoType,
    periodicity: Option<RegisterPeriodicity>,
) -> Result<Vec<RegisterAttribute>, AdapterError> {
    let template = match kind {
        MdoType::InformationRegister => MdoTemplateKind::InformationRegister,
        MdoType::AccumulationRegister => MdoTemplateKind::AccumulationRegister,
        _ => return Ok(Vec::new()),
    };
    let mut attributes = Vec::new();
    for spec in standard_attributes_for(template, ObjectView::RecordSet) {
        let unsupported = || AdapterError {
            issue: AdapterIssue::UnsupportedStandardAttribute {
                name: spec.kind.english_name().into(),
            },
            range: root.range.clone(),
        };
        let present = match spec.condition {
            PresenceCondition::Always => true,
            PresenceCondition::IsPeriodic => {
                periodicity.is_some_and(|value| value != RegisterPeriodicity::Nonperiodical)
            }
            _ => return Err(unsupported()),
        };
        if !present {
            continue;
        }
        let attr_type = match spec.value {
            AttrValueKind::Boolean => AttributeType::Boolean,
            AttrValueKind::NumberLineNumber => AttributeType::Number {
                precision: 10,
                scale: 0,
            },
            AttrValueKind::AnyDocumentRef => AttributeType::AnyObjectRef {
                mdo_type: MdoType::Document,
            },
            AttrValueKind::DateTime => AttributeType::DateTime,
            AttrValueKind::Unknown => AttributeType::Unknown,
            _ => return Err(unsupported()),
        };
        let mut field = RegisterAttribute::new(Uuid::nil(), spec.kind.russian_name());
        field.set_name_en(spec.kind.english_name());
        field.set_attr_type(attr_type);
        attributes.push(field);
    }
    Ok(attributes)
}

/// A dimension's Type is optional in the kernel; explicit types use the shared converter.
fn dimension(object: &ParsedObject) -> Result<Dimension, AdapterError> {
    let fields = Properties::new(object)?;
    let mut dimension = Dimension::builder()
        .uuid(identity::uuid(object)?)
        .name(object.metadata.name())
        .deny_incomplete_values(fields.boolean("DenyIncompleteValues")?.unwrap_or(false))
        .master(fields.boolean("Master")?.unwrap_or(false))
        .indexing(fields.text("Indexing")?.unwrap_or_default())
        .build();
    if let Some(field) = fields.field("Type")? {
        let value = attribute_type(field)?;
        dimension.set_type_str(value.to_string());
        dimension.set_attr_type(value);
    }
    Ok(dimension)
}

/// Resource identities and types are required, with no random identity or guessed default.
fn resource(object: &ParsedObject) -> Result<RegisterResource, AdapterError> {
    let mut resource = RegisterResource::new(identity::uuid(object)?, object.metadata.name());
    let value = required_type(object)?;
    resource.set_type_str(value.to_string());
    resource.set_attr_type(value);
    Ok(resource)
}

/// User attributes retain actual UUIDs, unlike implicit standard fields.
fn attribute(object: &ParsedObject) -> Result<RegisterAttribute, AdapterError> {
    let mut attribute = RegisterAttribute::new(identity::uuid(object)?, object.metadata.name());
    let value = required_type(object)?;
    attribute.set_type_str(value.to_string());
    attribute.set_attr_type(value);
    Ok(attribute)
}

/// Both resource and requisite types are mandatory and address missing data at their declaration.
fn required_type(object: &ParsedObject) -> Result<AttributeType, AdapterError> {
    let fields = Properties::new(object)?;
    let field = fields.field("Type")?.ok_or_else(|| AdapterError {
        issue: AdapterIssue::MissingProperty {
            name: "Type".into(),
        },
        range: object.range.clone(),
    })?;
    attribute_type(field)
}
