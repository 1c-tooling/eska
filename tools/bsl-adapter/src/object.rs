//! Object models combine source fields with the pinned platform's standard-attribute catalogue.

mod charts;
mod standard;
#[cfg(test)]
mod tests;

use bsl_metadata::{
    MdoType, MetadataObject,
    enums::CodeSeries,
    metadata_object::Attribute,
    tabular_section::{TabularSection, TabularSectionAttribute},
};
use bsl_platform::MdoTemplateKind;
use eska::project_core::{
    MetadataKind,
    metadata_parser::{ParsedDescriptor, ParsedObject},
};

use crate::{
    AdapterError, AdapterIssue, identity, properties::Properties, type_description::required_type,
};

/// Convert supported object descriptors, including typed attributes and ordered tabular sections.
///
/// # Errors
/// Returns unsupported kinds, incomplete descriptors, invalid identities or unsupported field values.
pub fn metadata_object(descriptor: &ParsedDescriptor) -> Result<MetadataObject, AdapterError> {
    let root = identity::descriptor_root(descriptor)?;
    let (kind, template) = model_kind(root)?;
    let fields = Properties::new(root)?;
    let (belonging, extends) = identity::ownership(&fields)?;
    let mut model = MetadataObject::new(kind, root.metadata.name());
    model.set_uuid(identity::uuid(root)?);
    model.set_object_belonging(belonging);
    if let Some(uuid) = extends {
        model.set_extends_uuid(uuid);
    }
    model.set_check_unique(fields.boolean("CheckUnique")?.unwrap_or(false));
    if let Some(field) = fields.field("CodeSeries")? {
        model.set_code_series(match fields.text("CodeSeries")? {
            Some("WholeCatalog" | "WholeCharacteristicKind" | "WholeChartOfAccounts") => {
                CodeSeries::WholeCatalog
            }
            Some("WithinSubordination") => CodeSeries::WithinSubordination,
            Some("WithinOwnerSubordination" | "WithinOwner") => {
                CodeSeries::WithinOwnerSubordination
            }
            _ => return Err(Properties::invalid(field)),
        });
    }
    if let Some(template) = template {
        model.attributes = standard::attributes(root, kind, template)?;
    }
    if kind == MdoType::Document {
        model.register_records = fields
            .references("RegisterRecords")?
            .into_iter()
            .map(|value| {
                let reference = standard::reference(root, "RegisterRecords", value)?;
                if matches!(
                    reference.0,
                    MdoType::InformationRegister
                        | MdoType::AccumulationRegister
                        | MdoType::AccountingRegister
                        | MdoType::CalculationRegister
                ) {
                    Ok(reference)
                } else {
                    Err(standard::invalid_reference(root, "RegisterRecords"))
                }
            })
            .collect::<Result<_, _>>()?;
    }
    for child in children(descriptor, root) {
        match child.metadata.kind() {
            MetadataKind::Attribute | MetadataKind::Dimension | MetadataKind::Resource => {
                model.add_attribute(attribute(child)?);
            }
            MetadataKind::AddressingAttribute if kind == MdoType::Task => {
                model.add_attribute(attribute(child)?);
            }
            MetadataKind::AccountingFlag if kind == MdoType::ChartOfAccounts => {
                model.add_attribute(attribute(child)?);
            }
            MetadataKind::TabularSection => {
                model.add_tabular_section(tabular_section(descriptor, child)?);
            }
            _ => {}
        }
    }
    model
        .tabular_sections
        .extend(charts::sections(descriptor, root)?);
    Ok(model)
}

/// Actual source kinds choose a catalogue; unsupported declarations never acquire another kind's defaults.
fn model_kind(object: &ParsedObject) -> Result<(MdoType, Option<MdoTemplateKind>), AdapterError> {
    Ok(match object.metadata.kind() {
        MetadataKind::Catalog => (MdoType::Catalog, Some(MdoTemplateKind::Catalog)),
        MetadataKind::Document => (MdoType::Document, Some(MdoTemplateKind::Document)),
        MetadataKind::BusinessProcess => (
            MdoType::BusinessProcess,
            Some(MdoTemplateKind::BusinessProcess),
        ),
        MetadataKind::Task => (MdoType::Task, Some(MdoTemplateKind::Task)),
        MetadataKind::ExchangePlan => (MdoType::ExchangePlan, Some(MdoTemplateKind::ExchangePlan)),
        MetadataKind::ChartOfCharacteristicTypes => (
            MdoType::ChartOfCharacteristicTypes,
            Some(MdoTemplateKind::ChartOfCharacteristicTypes),
        ),
        MetadataKind::ChartOfAccounts => (
            MdoType::ChartOfAccounts,
            Some(MdoTemplateKind::ChartOfAccounts),
        ),
        MetadataKind::ChartOfCalculationTypes => (
            MdoType::ChartOfCalculationTypes,
            Some(MdoTemplateKind::ChartOfCalculationTypes),
        ),
        MetadataKind::DataProcessor => (MdoType::DataProcessor, None),
        MetadataKind::Report => (MdoType::Report, None),
        actual => {
            return Err(AdapterError {
                issue: AdapterIssue::WrongKind { actual },
                range: object.range.clone(),
            });
        }
    })
}

/// Ordinary and task-addressing attributes share typed fields, while the source retains their distinct kinds/UUIDs.
fn attribute(object: &ParsedObject) -> Result<Attribute, AdapterError> {
    identity::uuid(object)?;
    Ok(Attribute {
        name: object.metadata.name().into(),
        name_en: None,
        attr_type: required_type(object)?,
    })
}

/// Direct parent identity distinguishes a tabular field from a same-named object attribute.
fn children<'a>(
    descriptor: &'a ParsedDescriptor,
    parent: &'a ParsedObject,
) -> impl Iterator<Item = &'a ParsedObject> {
    descriptor
        .objects
        .iter()
        .filter(|child| child.metadata.parent() == Some(parent.metadata.id()))
}

/// Retain source section/column UUIDs and use a real localized caption rather than XML whitespace.
fn tabular_section(
    descriptor: &ParsedDescriptor,
    object: &ParsedObject,
) -> Result<TabularSection, AdapterError> {
    let fields = Properties::new(object)?;
    let mut section = TabularSection::new(identity::uuid(object)?, object.metadata.name());
    // The kernel has one caption slot; the shared source model retains every language.
    let synonym = object
        .synonyms
        .iter()
        .find(|value| value.language == "ru")
        .or_else(|| object.synonyms.first())
        .map(|value| value.content.clone());
    section.set_synonym(synonym);
    section.set_use_mode(fields.text("Use")?.map(str::to_owned));
    let mut attributes = Vec::new();
    for child in children(descriptor, object) {
        if child.metadata.kind() != MetadataKind::Attribute {
            return Err(AdapterError {
                issue: AdapterIssue::WrongKind {
                    actual: child.metadata.kind(),
                },
                range: child.range.clone(),
            });
        }
        attributes.push(TabularSectionAttribute::new(
            identity::uuid(child)?,
            child.metadata.name(),
            required_type(child)?,
        ));
    }
    section.set_attributes(attributes);
    Ok(section)
}
