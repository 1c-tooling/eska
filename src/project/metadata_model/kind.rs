//! Typed Designer metadata kinds shared by the object model and IDE.

/// Define the supported vocabulary once, including external descriptor aliases.
macro_rules! metadata_kinds {
    ($( $variant:ident => ($key:literal, [$($tag:literal),+], $folder:literal) ),+ $(,)?) => {
        /// A supported logical metadata kind; independent of source paths and locale.
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub enum MetadataKind { $( $variant, )+ }

        impl MetadataKind {
            /// Every supported kind, without duplicate external descriptor aliases.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// Return the existing CLI machine key; this does not encode tree order.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $key),+ }
            }

            /// Return the top-level Designer collection, if this kind has one.
            #[must_use]
            pub fn collection_folder(self) -> Option<&'static str> {
                let folder = match self { $(Self::$variant => $folder),+ };
                (!folder.is_empty()).then_some(folder)
            }

            /// Resolve a top-level Designer directory using the same type registry.
            #[must_use]
            pub fn from_collection_folder(folder: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|kind| kind.collection_folder() == Some(folder))
            }

            /// Resolve a Designer tag, reporting an unsupported kind explicitly.
            ///
            /// # Errors
            /// Returns the unknown tag without guessing a replacement type.
            pub fn from_xml_tag(tag: &str) -> Result<Self, UnknownMetadataKind> {
                match tag {
                    $($($tag)|+ => Ok(Self::$variant),)+
                    _ => Err(UnknownMetadataKind { tag: tag.to_owned() }),
                }
            }
        }
    };
}

metadata_kinds! {
    Configuration => ("configuration", ["Configuration"], ""),
    DataProcessor => ("data-processor", ["ExternalDataProcessor", "DataProcessor"], "DataProcessors"),
    Report => ("report", ["ExternalReport", "Report"], "Reports"),
    AccountingRegister => ("accounting-register", ["AccountingRegister"], "AccountingRegisters"),
    AccumulationRegister => ("accumulation-register", ["AccumulationRegister"], "AccumulationRegisters"),
    Bot => ("bot", ["Bot"], "Bots"),
    BusinessProcess => ("business-process", ["BusinessProcess"], "BusinessProcesses"),
    CalculationRegister => ("calculation-register", ["CalculationRegister"], "CalculationRegisters"),
    Catalog => ("catalog", ["Catalog"], "Catalogs"),
    ChartOfAccounts => ("chart-of-accounts", ["ChartOfAccounts"], "ChartsOfAccounts"),
    ChartOfCalculationTypes => ("chart-of-calculation-types", ["ChartOfCalculationTypes"], "ChartsOfCalculationTypes"),
    ChartOfCharacteristicTypes => ("chart-of-characteristic-types", ["ChartOfCharacteristicTypes"], "ChartsOfCharacteristicTypes"),
    CommandGroup => ("command-group", ["CommandGroup"], "CommandGroups"),
    CommonAttribute => ("common-attribute", ["CommonAttribute"], "CommonAttributes"),
    CommonCommand => ("common-command", ["CommonCommand"], "CommonCommands"),
    CommonForm => ("common-form", ["CommonForm"], "CommonForms"),
    CommonModule => ("common-module", ["CommonModule"], "CommonModules"),
    CommonPicture => ("common-picture", ["CommonPicture"], "CommonPictures"),
    CommonTemplate => ("common-template", ["CommonTemplate"], "CommonTemplates"),
    Constant => ("constant", ["Constant"], "Constants"),
    DefinedType => ("defined-type", ["DefinedType"], "DefinedTypes"),
    Document => ("document", ["Document"], "Documents"),
    DocumentJournal => ("document-journal", ["DocumentJournal"], "DocumentJournals"),
    DocumentNumerator => ("document-numerator", ["DocumentNumerator"], "DocumentNumerators"),
    Enum => ("enum", ["Enum"], "Enums"),
    EventSubscription => ("event-subscription", ["EventSubscription"], "EventSubscriptions"),
    ExchangePlan => ("exchange-plan", ["ExchangePlan"], "ExchangePlans"),
    ExternalDataSource => ("external-data-source", ["ExternalDataSource"], "ExternalDataSources"),
    FilterCriterion => ("filter-criterion", ["FilterCriterion"], "FilterCriteria"),
    FunctionalOption => ("functional-option", ["FunctionalOption"], "FunctionalOptions"),
    FunctionalOptionsParameter => ("functional-option-parameter", ["FunctionalOptionsParameter"], "FunctionalOptionsParameters"),
    HTTPService => ("http-service", ["HTTPService"], "HTTPServices"),
    InformationRegister => ("information-register", ["InformationRegister"], "InformationRegisters"),
    IntegrationService => ("integration-service", ["IntegrationService"], "IntegrationServices"),
    Language => ("language", ["Language"], "Languages"),
    Role => ("role", ["Role"], "Roles"),
    ScheduledJob => ("scheduled-job", ["ScheduledJob"], "ScheduledJobs"),
    Sequence => ("sequence", ["Sequence"], "Sequences"),
    SessionParameter => ("session-parameter", ["SessionParameter"], "SessionParameters"),
    SettingsStorage => ("settings-storage", ["SettingsStorage"], "SettingsStorages"),
    Style => ("style", ["Style"], "Styles"),
    StyleItem => ("style-item", ["StyleItem"], "StyleItems"),
    Subsystem => ("subsystem", ["Subsystem"], "Subsystems"),
    Task => ("task", ["Task"], "Tasks"),
    WebService => ("web-service", ["WebService"], "WebServices"),
    WSReference => ("ws-reference", ["WSReference"], "WSReferences"),
    XDTOPackage => ("xdto-package", ["XDTOPackage"], "XDTOPackages"),
    Form => ("form", ["Form"], ""),
    Template => ("template", ["Template"], ""),
    Command => ("command", ["Command"], ""),
    AddressingAttribute => ("addressing-attribute", ["AddressingAttribute"], ""),
    Attribute => ("attribute", ["Attribute"], ""),
    TabularSection => ("tabular-section", ["TabularSection"], ""),
    Dimension => ("dimension", ["Dimension"], ""),
    Resource => ("resource", ["Resource"], ""),
    Requisite => ("requisite", ["Requisite"], ""),
    EnumValue => ("enum-value", ["EnumValue"], ""),
    AccountingFlag => ("accounting-flag", ["AccountingFlag"], ""),
    ExtDimensionAccountingFlag => ("ext-dimension-accounting-flag", ["ExtDimensionAccountingFlag"], ""),
    Recalculation => ("recalculation", ["Recalculation"], ""),
    Column => ("column", ["Column"], ""),
    URLTemplate => ("url-template", ["URLTemplate"], ""),
    Method => ("method", ["Method"], ""),
    Operation => ("operation", ["Operation"], ""),
    Parameter => ("parameter", ["Parameter"], ""),
    IntegrationServiceChannel => ("integration-service-channel", ["IntegrationServiceChannel"], ""),
}

/// An unsupported Designer metadata tag, retained for a localized diagnostic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnknownMetadataKind {
    pub tag: String,
}
