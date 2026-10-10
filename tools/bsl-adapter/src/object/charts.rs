//! Implicit chart rows mirror native semantics without inventing source declarations or identities.

use bsl_metadata::{
    AttributeType, MdoType, Uuid,
    tabular_section::{TabularSection, TabularSectionAttribute},
};
use eska::project_core::{
    MetadataKind,
    metadata_parser::{ParsedDescriptor, ParsedObject},
};

use super::{attribute, children, standard};
use crate::{AdapterError, properties::Properties};

/// Append implicit sections after real declared sections, preserving native ordering and absence rules.
pub(super) fn sections(
    descriptor: &ParsedDescriptor,
    object: &ParsedObject,
) -> Result<Vec<TabularSection>, AdapterError> {
    match object.metadata.kind() {
        MetadataKind::ChartOfAccounts => Ok(vec![ext_dimension_types(descriptor, object)?]),
        MetadataKind::ChartOfCalculationTypes
            if standard::calculation_dependency(object)?.is_dependent() =>
        {
            Ok(calculation_dependencies(object.metadata.name()))
        }
        _ => Ok(Vec::new()),
    }
}

/// Real ext-dimension flags are columns of the implicit row; their physical kind/UUID remains in the descriptor.
fn ext_dimension_types(
    descriptor: &ParsedDescriptor,
    object: &ParsedObject,
) -> Result<TabularSection, AdapterError> {
    let fields = Properties::new(object)?;
    let attr_type = match fields.text("ExtDimensionTypes")? {
        None | Some("") => AttributeType::Unknown,
        Some(value) => {
            let (kind, name) = standard::reference(object, "ExtDimensionTypes", value)?;
            if kind != MdoType::ChartOfCharacteristicTypes {
                return Err(standard::invalid_reference(object, "ExtDimensionTypes"));
            }
            AttributeType::Ref {
                mdo_type: kind,
                name,
            }
        }
    };
    let mut section = TabularSection::new(Uuid::nil(), "ВидыСубконто");
    section.set_name_en(Some("ExtDimensionTypes".into()));
    let mut columns = Vec::new();
    for (name, name_en, kind) in [
        ("ВидСубконто", "ExtDimensionType", attr_type),
        ("Предопределенное", "Predefined", AttributeType::Boolean),
        ("ТолькоОбороты", "TurnoversOnly", AttributeType::Boolean),
    ] {
        let mut column = TabularSectionAttribute::new(Uuid::nil(), name, kind);
        column.set_name_en(Some(name_en.into()));
        columns.push(column);
    }
    for flag in children(descriptor, object)
        .filter(|child| child.metadata.kind() == MetadataKind::ExtDimensionAccountingFlag)
    {
        let field = attribute(flag)?;
        // The pinned kernel models this entire implicit row with nil IDs; navigation confirms the real source flag independently.
        columns.push(TabularSectionAttribute::new(
            Uuid::nil(),
            field.name,
            field.attr_type,
        ));
    }
    section.set_attributes(columns);
    Ok(section)
}

/// Dependent charts carry three implicit self-reference rows, each without a source section/column UUID.
fn calculation_dependencies(chart: &str) -> Vec<TabularSection> {
    [
        ("ВытесняющиеВидыРасчета", "DisplacingCalculationTypes"),
        ("ВедущиеВидыРасчета", "LeadingCalculationTypes"),
        ("БазовыеВидыРасчета", "BaseCalculationTypes"),
    ]
    .into_iter()
    .map(|(name, name_en)| {
        let mut section = TabularSection::new(Uuid::nil(), name);
        section.set_name_en(Some(name_en.into()));
        let mut column = TabularSectionAttribute::new(
            Uuid::nil(),
            "ВидРасчета",
            AttributeType::Ref {
                mdo_type: MdoType::ChartOfCalculationTypes,
                name: chart.into(),
            },
        );
        column.set_name_en(Some("CalculationType".into()));
        section.set_attributes(vec![column]);
        section
    })
    .collect()
}
