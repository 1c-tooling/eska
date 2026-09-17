//! Typed Designer metadata kinds shared by the object model and IDE.

/// Define the supported vocabulary once, including external descriptor aliases.
macro_rules! metadata_kinds {
    ($( $variant:ident => ($key:literal, [$($tag:literal),+]) ),+ $(,)?) => {
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
    Configuration => ("configuration", ["Configuration"]),
    DataProcessor => ("data-processor", ["ExternalDataProcessor", "DataProcessor"]),
    Report => ("report", ["ExternalReport", "Report"]),
    AccountingRegister => ("accounting-register", ["AccountingRegister"]),
    AccumulationRegister => ("accumulation-register", ["AccumulationRegister"]),
    Bot => ("bot", ["Bot"]),
    BusinessProcess => ("business-process", ["BusinessProcess"]),
    CalculationRegister => ("calculation-register", ["CalculationRegister"]),
    Catalog => ("catalog", ["Catalog"]),
    ChartOfAccounts => ("chart-of-accounts", ["ChartOfAccounts"]),
    ChartOfCalculationTypes => ("chart-of-calculation-types", ["ChartOfCalculationTypes"]),
    ChartOfCharacteristicTypes => ("chart-of-characteristic-types", ["ChartOfCharacteristicTypes"]),
    CommandGroup => ("command-group", ["CommandGroup"]),
    CommonAttribute => ("common-attribute", ["CommonAttribute"]),
    CommonCommand => ("common-command", ["CommonCommand"]),
    CommonForm => ("common-form", ["CommonForm"]),
    CommonModule => ("common-module", ["CommonModule"]),
    CommonPicture => ("common-picture", ["CommonPicture"]),
    CommonTemplate => ("common-template", ["CommonTemplate"]),
    Constant => ("constant", ["Constant"]),
    DefinedType => ("defined-type", ["DefinedType"]),
    Document => ("document", ["Document"]),
    DocumentJournal => ("document-journal", ["DocumentJournal"]),
    DocumentNumerator => ("document-numerator", ["DocumentNumerator"]),
    Enum => ("enum", ["Enum"]),
    EventSubscription => ("event-subscription", ["EventSubscription"]),
    ExchangePlan => ("exchange-plan", ["ExchangePlan"]),
    ExternalDataSource => ("external-data-source", ["ExternalDataSource"]),
    FilterCriterion => ("filter-criterion", ["FilterCriterion"]),
    FunctionalOption => ("functional-option", ["FunctionalOption"]),
    FunctionalOptionsParameter => ("functional-option-parameter", ["FunctionalOptionsParameter"]),
    HTTPService => ("http-service", ["HTTPService"]),
    InformationRegister => ("information-register", ["InformationRegister"]),
    IntegrationService => ("integration-service", ["IntegrationService"]),
    Language => ("language", ["Language"]),
    Role => ("role", ["Role"]),
    ScheduledJob => ("scheduled-job", ["ScheduledJob"]),
    Sequence => ("sequence", ["Sequence"]),
    SessionParameter => ("session-parameter", ["SessionParameter"]),
    SettingsStorage => ("settings-storage", ["SettingsStorage"]),
    Style => ("style", ["Style"]),
    StyleItem => ("style-item", ["StyleItem"]),
    Subsystem => ("subsystem", ["Subsystem"]),
    Task => ("task", ["Task"]),
    WebService => ("web-service", ["WebService"]),
    WSReference => ("ws-reference", ["WSReference"]),
    XDTOPackage => ("xdto-package", ["XDTOPackage"]),
    Form => ("form", ["Form"]),
    Template => ("template", ["Template"]),
    Command => ("command", ["Command"]),
    Attribute => ("attribute", ["Attribute"]),
    TabularSection => ("tabular-section", ["TabularSection"]),
    Dimension => ("dimension", ["Dimension"]),
    Resource => ("resource", ["Resource"]),
    Requisite => ("requisite", ["Requisite"]),
    EnumValue => ("enum-value", ["EnumValue"]),
    AccountingFlag => ("accounting-flag", ["AccountingFlag"]),
    ExtDimensionAccountingFlag => ("ext-dimension-accounting-flag", ["ExtDimensionAccountingFlag"]),
    Recalculation => ("recalculation", ["Recalculation"]),
    Column => ("column", ["Column"]),
    URLTemplate => ("url-template", ["URLTemplate"]),
    Method => ("method", ["Method"]),
    Operation => ("operation", ["Operation"]),
    Parameter => ("parameter", ["Parameter"]),
    IntegrationServiceChannel => ("integration-service-channel", ["IntegrationServiceChannel"]),
}

/// An unsupported Designer metadata tag, retained for a localized diagnostic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnknownMetadataKind {
    pub tag: String,
}
