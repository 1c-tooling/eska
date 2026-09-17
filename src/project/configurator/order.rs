//! One explicit order shared with semantic diff; no CLI dependency.
use crate::project::metadata_model::MetadataKind;

/// Flattened Configurator class order, retaining the existing T59 ranking contract.
pub const METADATA_ORDER: &[MetadataKind] = &[
    MetadataKind::Configuration,
    MetadataKind::Subsystem,
    MetadataKind::CommonModule,
    MetadataKind::SessionParameter,
    MetadataKind::Role,
    MetadataKind::CommonAttribute,
    MetadataKind::ExchangePlan,
    MetadataKind::FilterCriterion,
    MetadataKind::EventSubscription,
    MetadataKind::ScheduledJob,
    MetadataKind::FunctionalOption,
    MetadataKind::FunctionalOptionsParameter,
    MetadataKind::DefinedType,
    MetadataKind::SettingsStorage,
    MetadataKind::CommonForm,
    MetadataKind::CommonCommand,
    MetadataKind::CommandGroup,
    MetadataKind::CommonTemplate,
    MetadataKind::CommonPicture,
    MetadataKind::Style,
    MetadataKind::StyleItem,
    MetadataKind::Language,
    MetadataKind::XDTOPackage,
    MetadataKind::WebService,
    MetadataKind::HTTPService,
    MetadataKind::WSReference,
    MetadataKind::IntegrationService,
    MetadataKind::Bot,
    MetadataKind::Constant,
    MetadataKind::Catalog,
    MetadataKind::Document,
    MetadataKind::DocumentNumerator,
    MetadataKind::Sequence,
    MetadataKind::DocumentJournal,
    MetadataKind::Enum,
    MetadataKind::Report,
    MetadataKind::DataProcessor,
    MetadataKind::ChartOfCharacteristicTypes,
    MetadataKind::ChartOfAccounts,
    MetadataKind::ChartOfCalculationTypes,
    MetadataKind::InformationRegister,
    MetadataKind::AccumulationRegister,
    MetadataKind::AccountingRegister,
    MetadataKind::CalculationRegister,
    MetadataKind::BusinessProcess,
    MetadataKind::Task,
    MetadataKind::ExternalDataSource,
];

/// Rank a machine kind, keeping unknown future classes after known classes.
#[must_use]
pub fn metadata_group_rank(kind: &str) -> usize {
    METADATA_ORDER
        .iter()
        .position(|candidate| candidate.as_str() == kind)
        .unwrap_or(usize::MAX)
}
