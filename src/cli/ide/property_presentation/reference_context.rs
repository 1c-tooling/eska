//! Audited Designer selector contexts (EDT 8.3.27/8.5.1 serializer and real XML).

use super::{MD, READABLE, key_is};
use crate::project::metadata_model::{MetadataKind, PropertyKey};

/// Classify only schema-defined reference fields, never arbitrary dotted strings.
pub(super) fn is_reference(owner: MetadataKind, key: &PropertyKey, path: &[&PropertyKey]) -> bool {
    if path.is_empty() && key.namespace.as_deref() == Some(MD) {
        return top_level(owner, &key.name);
    }
    if key.namespace.as_deref() != Some(READABLE) {
        return false;
    }
    let Some(root) = path
        .first()
        .filter(|root| root.namespace.as_deref() == Some(MD))
    else {
        return false;
    };
    match (root.name.as_str(), key.name.as_str()) {
        ("Picture", "Ref") | ("InputByString" | "DataLockFields", "Field") => path.len() == 1,
        ("Content", "Metadata" | "ConditionalSeparation") => owner == MetadataKind::CommonAttribute,
        ("Content", "Object") => owner == MetadataKind::FunctionalOption,
        ("LinkByType" | "ChoiceParameterLinks" | "StandardAttributes", "DataPath") => {
            path.last().is_some_and(|parent| {
                key_is(parent, READABLE, "Link")
                    || key_is(parent, MD, "LinkByType")
                    || key_is(parent, READABLE, "LinkByType")
            })
        }
        (
            "Characteristics",
            "KeyField"
            | "TypesFilterField"
            | "ObjectField"
            | "TypeField"
            | "ValueField"
            | "DataPathField"
            | "MultipleValuesUseField"
            | "MultipleValuesKeyField"
            | "MultipleValuesOrderField",
        ) => path.len() == 3,
        ("XDTOPackages", "Value") => path.len() == 2,
        _ => false,
    }
}

/// Form selectors and unambiguous MD-object-valued properties share explicit path syntax.
fn top_level(owner: MetadataKind, name: &str) -> bool {
    use MetadataKind as K;
    matches!(
        name,
        "DefaultChoiceForm"
            | "DefaultFolderChoiceForm"
            | "DefaultFolderForm"
            | "DefaultForm"
            | "DefaultListForm"
            | "DefaultObjectForm"
            | "DefaultRecordForm"
            | "DefaultReportForm"
            | "DefaultReportSettingsForm"
            | "DefaultReportVariantForm"
            | "DefaultSettingsForm"
            | "DefaultVariantForm"
            | "DefaultConstantsForm"
            | "DefaultLoadForm"
            | "DefaultSaveForm"
            | "DefaultSearchForm"
            | "DefaultDynamicListSettingsForm"
            | "DefaultDataHistoryChangeHistoryForm"
            | "DefaultDataHistoryVersionDataForm"
            | "DefaultDataHistoryVersionDifferencesForm"
            | "DefaultCollaborationSystemUsersChoiceForm"
            | "ChoiceForm"
            | "MainDataCompositionSchema"
            | "AuxiliaryChoiceForm"
            | "AuxiliaryFolderChoiceForm"
            | "AuxiliaryFolderForm"
            | "AuxiliaryForm"
            | "AuxiliaryListForm"
            | "AuxiliaryObjectForm"
            | "AuxiliaryRecordForm"
            | "AuxiliaryReportForm"
            | "AuxiliaryReportVariantForm"
            | "AuxiliaryReportSettingsForm"
            | "AuxiliaryDynamicListSettingsForm"
            | "AuxiliaryDataHistoryChangeHistoryForm"
            | "AuxiliaryDataHistoryVersionDataForm"
            | "AuxiliaryDataHistoryVersionDifferencesForm"
            | "AuxiliaryCollaborationSystemUsersChoiceForm"
            | "AuxiliarySaveForm"
            | "AuxiliaryLoadForm"
            | "BinaryDataStorageLocationUseField"
    ) || matches!(
        (owner, name),
        (
            K::Configuration,
            "DefaultStyle"
                | "DefaultLanguage"
                | "DefaultRole"
                | "DefaultInterface"
                | "DefaultReportAppearanceTemplate"
                | "ReportsVariantsStorage"
                | "ReportsSettingsStorage"
                | "CommonSettingsStorage"
                | "FormDataSettingsStorage"
                | "DynamicListsSettingsStorage"
        ) | (K::Document, "Numerator")
            | (K::ChartOfCharacteristicTypes, "CharacteristicExtValues")
            | (K::ChartOfAccounts, "ExtDimensionTypes")
            | (K::AccountingRegister, "ChartOfAccounts")
            | (
                K::CalculationRegister,
                "ChartOfCalculationTypes" | "Schedule" | "ScheduleValue" | "ScheduleDate"
            )
            | (
                K::Dimension | K::Resource,
                "AccountingFlag" | "ExtDimensionAccountingFlag"
            )
            | (
                K::Dimension | K::Attribute,
                "ScheduleLink" | "RegisterDimension"
            )
            | (
                K::CommonAttribute,
                "DataSeparationValue" | "DataSeparationUse" | "ConditionalSeparation"
            )
            | (
                K::Task,
                "Addressing" | "MainAddressingAttribute" | "CurrentPerformer"
            )
            | (K::AddressingAttribute, "AddressingDimension")
            | (K::BusinessProcess, "Task")
            | (K::FunctionalOption, "Location")
            | (K::Report, "SettingsStorage" | "VariantsStorage")
            | (K::Command | K::CommonCommand, "Group")
    )
}
