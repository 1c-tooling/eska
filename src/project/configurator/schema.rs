//! Configurator collections are explicit; XML serialization and directory order are unrelated.

use super::METADATA_ORDER;
use crate::project::{
    ProjectType,
    designer_source::DesignerSource,
    metadata_model::{MetadataKind, ModuleRole, ObjectId},
};

/// A schema belongs to a manifest-validated project, including its logical root identity.
#[derive(Clone, Debug)]
pub struct ConfiguratorSchema {
    project_type: ProjectType,
    root: ObjectId,
}

impl ConfiguratorSchema {
    /// Single-kind structural subtrees are displayed directly, without an extra virtual level.
    #[must_use]
    pub const fn has_direct_children(kind: MetadataKind) -> bool {
        matches!(
            kind,
            MetadataKind::TabularSection
                | MetadataKind::Subsystem
                | MetadataKind::HTTPService
                | MetadataKind::URLTemplate
                | MetadataKind::WebService
                | MetadataKind::Operation
                | MetadataKind::IntegrationService
                | MetadataKind::Sequence
                | MetadataKind::Recalculation
        )
    }

    /// Select the schema from a checked manifest, never from the source directory's name.
    #[must_use]
    pub fn for_source(source: &DesignerSource) -> Self {
        Self {
            project_type: source.project().configuration().project_type(),
            root: source.root().id().clone(),
        }
    }

    /// Expose the checked project type to consumers without inferring it from metadata aliases.
    #[must_use]
    pub const fn project_type(&self) -> ProjectType {
        self.project_type
    }

    /// Configuration and extension roots have sections; external roots are ordinary owners.
    #[must_use]
    pub fn has_root_sections(&self, id: &ObjectId) -> bool {
        &self.root == id
            && matches!(
                self.project_type,
                ProjectType::Configuration | ProjectType::Extension
            )
    }

    /// Classes nested under Common, in the same order used by semantic diff.
    pub fn common_collections() -> impl Iterator<Item = MetadataKind> {
        METADATA_ORDER
            .iter()
            .copied()
            .skip(1)
            .take_while(|kind| *kind != MetadataKind::Constant)
    }

    /// Applied root sections; numerators and sequences belong inside Documents.
    pub fn root_collections() -> impl Iterator<Item = MetadataKind> {
        METADATA_ORDER
            .iter()
            .copied()
            .skip_while(|kind| *kind != MetadataKind::Constant)
            .filter(|kind| {
                !matches!(
                    kind,
                    MetadataKind::DocumentNumerator | MetadataKind::Sequence
                )
            })
    }

    /// Apply manifest-specific differences only to the actual project root.
    #[must_use]
    pub fn owner_collections(
        &self,
        owner: &ObjectId,
        kind: MetadataKind,
    ) -> &'static [MetadataKind] {
        if &self.root == owner && self.project_type == ProjectType::Processing {
            return &[
                MetadataKind::Attribute,
                MetadataKind::TabularSection,
                MetadataKind::Form,
                MetadataKind::Template,
            ];
        }
        Self::collections(kind)
    }

    /// Ordered collections of each supported class; leaf cases are deliberately exhaustive.
    #[must_use]
    pub const fn collections(kind: MetadataKind) -> &'static [MetadataKind] {
        use MetadataKind::{
            Attribute, Command, Dimension, Form, Resource, TabularSection, Template,
        };
        match kind {
            MetadataKind::Catalog
            | MetadataKind::Document
            | MetadataKind::DataProcessor
            | MetadataKind::Report
            | MetadataKind::ExchangePlan
            | MetadataKind::ChartOfCharacteristicTypes
            | MetadataKind::ChartOfCalculationTypes
            | MetadataKind::BusinessProcess => {
                &[Attribute, TabularSection, Form, Command, Template]
            }
            MetadataKind::ChartOfAccounts => &[
                Attribute,
                MetadataKind::AccountingFlag,
                MetadataKind::ExtDimensionAccountingFlag,
                TabularSection,
                Form,
                Command,
                Template,
            ],
            MetadataKind::Task => &[
                MetadataKind::AddressingAttribute,
                Attribute,
                TabularSection,
                Form,
                Command,
                Template,
            ],
            MetadataKind::InformationRegister
            | MetadataKind::AccumulationRegister
            | MetadataKind::AccountingRegister => {
                &[Dimension, Resource, Attribute, Form, Command, Template]
            }
            MetadataKind::CalculationRegister => &[
                Dimension,
                Resource,
                Attribute,
                MetadataKind::Recalculation,
                Form,
                Command,
                Template,
            ],
            MetadataKind::Enum => &[MetadataKind::EnumValue, Form, Command, Template],
            MetadataKind::DocumentJournal => &[MetadataKind::Column, Form, Command, Template],
            MetadataKind::FilterCriterion => &[Form, Command],
            MetadataKind::SettingsStorage => &[Form],
            MetadataKind::Sequence | MetadataKind::Recalculation => &[Dimension],
            MetadataKind::TabularSection => &[Attribute],
            MetadataKind::Subsystem => &[MetadataKind::Subsystem],
            MetadataKind::HTTPService => &[MetadataKind::URLTemplate],
            MetadataKind::URLTemplate => &[MetadataKind::Method],
            MetadataKind::WebService => &[MetadataKind::Operation],
            MetadataKind::Operation => &[MetadataKind::Parameter],
            MetadataKind::IntegrationService => &[MetadataKind::IntegrationServiceChannel],
            // External-source table/cube/function tags are outside the current vocabulary;
            // their parser diagnostics remain visible in the unsupported-fragments branch.
            MetadataKind::Configuration
            | MetadataKind::ExternalDataSource
            | MetadataKind::Bot
            | MetadataKind::CommandGroup
            | MetadataKind::CommonAttribute
            | MetadataKind::CommonCommand
            | MetadataKind::CommonForm
            | MetadataKind::CommonModule
            | MetadataKind::CommonPicture
            | MetadataKind::CommonTemplate
            | MetadataKind::Constant
            | MetadataKind::DefinedType
            | MetadataKind::DocumentNumerator
            | MetadataKind::EventSubscription
            | MetadataKind::FunctionalOption
            | MetadataKind::FunctionalOptionsParameter
            | MetadataKind::Language
            | MetadataKind::Role
            | MetadataKind::ScheduledJob
            | MetadataKind::SessionParameter
            | MetadataKind::Style
            | MetadataKind::StyleItem
            | MetadataKind::WebSocketClient
            | MetadataKind::WSReference
            | MetadataKind::XDTOPackage
            | MetadataKind::Form
            | MetadataKind::Template
            | MetadataKind::Command
            | MetadataKind::Attribute
            | MetadataKind::Dimension
            | MetadataKind::Resource
            | MetadataKind::Requisite
            | MetadataKind::EnumValue
            | MetadataKind::AccountingFlag
            | MetadataKind::ExtDimensionAccountingFlag
            | MetadataKind::Column
            | MetadataKind::Method
            | MetadataKind::Parameter
            | MetadataKind::IntegrationServiceChannel
            | MetadataKind::AddressingAttribute => &[],
        }
    }

    /// Applicable modules in display order, not a claim that their BSL files exist.
    #[must_use]
    pub fn modules(&self, owner: &ObjectId, kind: MetadataKind) -> &'static [ModuleRole] {
        use ModuleRole::{Manager, Module, Object, RecordSet};
        if &self.root == owner
            && matches!(
                self.project_type,
                ProjectType::Report | ProjectType::Processing
            )
        {
            return &[Object];
        }
        match kind {
            MetadataKind::Configuration => &[
                ModuleRole::ManagedApplication,
                ModuleRole::OrdinaryApplication,
                ModuleRole::Session,
                ModuleRole::ExternalConnection,
            ],
            MetadataKind::Catalog
            | MetadataKind::Document
            | MetadataKind::DataProcessor
            | MetadataKind::Report
            | MetadataKind::ExchangePlan
            | MetadataKind::ChartOfAccounts
            | MetadataKind::ChartOfCharacteristicTypes
            | MetadataKind::ChartOfCalculationTypes
            | MetadataKind::BusinessProcess
            | MetadataKind::Task => &[Object, Manager],
            MetadataKind::InformationRegister
            | MetadataKind::AccumulationRegister
            | MetadataKind::AccountingRegister
            | MetadataKind::CalculationRegister => &[Manager, RecordSet],
            MetadataKind::Constant => &[ModuleRole::ValueManager, Manager],
            MetadataKind::Sequence => &[RecordSet],
            MetadataKind::Enum
            | MetadataKind::DocumentJournal
            | MetadataKind::FilterCriterion
            | MetadataKind::SettingsStorage => &[Manager],
            MetadataKind::CommonModule
            | MetadataKind::CommonForm
            | MetadataKind::Form
            | MetadataKind::WebService
            | MetadataKind::HTTPService
            | MetadataKind::IntegrationService
            | MetadataKind::Bot
            | MetadataKind::WebSocketClient => &[Module],
            MetadataKind::CommonCommand | MetadataKind::Command => &[ModuleRole::Command],
            _ => &[],
        }
    }
}
