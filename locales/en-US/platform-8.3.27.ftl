## 1C platform captions. See docs/platform-catalog.md for sources and version scope.

## Metadata tree
tree-collection-configuration = Configuration
tree-collection-accounting-register = Accounting registers
tree-collection-accumulation-register = Accumulation registers
tree-collection-bot = Bots
tree-collection-business-process = Business processes
tree-collection-calculation-register = Calculation registers
tree-collection-catalog = Catalogs
tree-collection-chart-of-accounts = Charts of accounts
tree-collection-chart-of-calculation-types = Charts of calculation types
tree-collection-chart-of-characteristic-types = Charts of characteristic types
tree-collection-command-group = Command groups
tree-collection-common-attribute = Common attributes
tree-collection-common-command = Common commands
tree-collection-common-form = Common forms
tree-collection-common-module = Common modules
tree-collection-common-picture = Common pictures
tree-collection-common-template = Common templates
tree-collection-constant = Constants
tree-collection-data-processor = Data processors
tree-collection-defined-type = Defined types
tree-collection-document-journal = Document journals
tree-collection-document-numerator = Numerators
tree-collection-document = Documents
tree-collection-enum = Enums
tree-collection-event-subscription = Event subscriptions
tree-collection-exchange-plan = Exchange plans
tree-collection-external-data-source = External data sources
tree-collection-filter-criterion = Filter criteria
tree-collection-functional-option = Functional options
tree-collection-functional-option-parameter = Functional option parameters
tree-collection-http-service = HTTP services
tree-collection-information-register = Information registers
tree-collection-integration-service = Integration services
tree-collection-language = Languages
tree-collection-report = Reports
tree-collection-role = Roles
tree-collection-scheduled-job = Scheduled jobs
tree-collection-sequence = Sequences
tree-collection-session-parameter = Session parameters
tree-collection-settings-storage = Settings storages
tree-collection-style-item = Style items
tree-collection-style = Styles
tree-collection-subsystem = Subsystems
tree-collection-task = Tasks
tree-collection-web-service = Web services
tree-collection-ws-reference = WS references
tree-collection-xdto-package = XDTO packages
tree-collection-form = Forms
tree-collection-template = Templates
tree-collection-command = Commands
tree-collection-attribute = Attributes
tree-collection-tabular-section = Tabular sections
tree-collection-dimension = Dimensions
tree-collection-resource = Resources
tree-collection-requisite = Attributes
tree-collection-enum-value = Values
tree-collection-accounting-flag = Accounting flags
tree-collection-ext-dimension-accounting-flag = Extra dimension accounting flags
tree-collection-recalculation = Recalculations
tree-collection-url-template = URL templates
tree-collection-method = Methods
tree-collection-operation = Operations
tree-collection-parameter = Parameters
tree-collection-integration-service-channel = Channels
tree-collection-column = Columns
tree-collection-addressing-attribute = Addressing attributes
tree-collection-common = Common
tree-collection-modules = Modules
tree-collection-unsupported = Unsupported elements
tree-module-module = Module
tree-module-object = Object module
tree-module-manager = Manager module
tree-module-record-set = Record set module
tree-module-value-manager = Value manager module
tree-module-managed-application = Managed application module
tree-module-ordinary-application = Ordinary application module
tree-module-session = Session module
tree-module-external-connection = External connection module
tree-module-command = Command module
tree-collection-web-socket-client = WebSocket clients
tree-collection-predefined-item = Predefined data

## Properties and structured fields

# Reviewed Designer XML structural field
platform-property-app-functionality = Functionality

# Reviewed Designer XML structural field
platform-property-app-item = Item

# Reviewed Designer XML structural field
platform-property-app-use = Use

# Reviewed Designer XML structural field
platform-property-app-value = Value

# StringQualifiers|fixed
platform-property-core-AllowedLength = Allowed length

# Reviewed Designer XML structural field
platform-property-core-AllowedSign = Allowed sign

# TypeDescription|binaryQualifiers
platform-property-core-BinaryDataQualifiers = Binary qualifiers

# DateQualifiers|dateFractions
platform-property-core-DateFractions = Date fractions

# TypeDescription|dateQualifiers
platform-property-core-DateQualifiers = Date qualifiers

# NumberQualifiers|precision
platform-property-core-Digits = Precision

# NumberQualifiers|scale
platform-property-core-FractionDigits = Scale

# StringQualifiers|length
platform-property-core-Length = Length

# TypeDescription|numberQualifiers
platform-property-core-NumberQualifiers = Number qualifiers

# TypeDescription|stringQualifiers
platform-property-core-StringQualifiers = String qualifiers

# BasicFeature|type; ChartOfCharacteristicTypes|type; Constant|type; DefinedType|type; Dimension|type; Field|type; FilterCriterion|type; Function|type; Resource|type; SequenceDimension|type; SessionParameter|type
platform-property-core-Type = Type

# Reviewed Designer XML structural field
platform-property-core-TypeId = Type identifier

# Reviewed Designer XML structural field
platform-property-core-TypeSet = Type set

# Reviewed Designer XML structural field
platform-property-core-Value = Value

# Reviewed Designer XML structural field
platform-property-core-content = Text

# Reviewed Designer XML structural field
platform-property-core-item = Item

# Reviewed Designer XML structural field
platform-property-core-lang = Language

# AccountingRegisterDimension|accountingFlag; AccountingRegisterResource|accountingFlag
platform-property-md-AccountingFlag = Accounting flag

# CalculationRegister|actionPeriod
platform-property-md-ActionPeriod = Action period

# ChartOfCalculationTypes|actionPeriodUse
platform-property-md-ActionPeriodUse = Uses action period

# Configuration|additionalFullTextSearchDictionaries
platform-property-md-AdditionalFullTextSearchDictionaries = Additional full text search dictionaries

# Task|addressing
platform-property-md-Addressing = Addressing

# AddressingAttribute|addressingDimension
platform-property-md-AddressingDimension = Addressing dimension

# AccumulationRegister|aggregates
platform-property-md-Aggregates = Aggregates

# Field|allowNull
platform-property-md-AllowNull = NULL allowed

# Configuration|allowedIncomingShareRequestTypes
platform-property-md-AllowedIncomingShareRequestTypes = Allowed types of incoming "Share" requests

# CommonAttribute|authenticationSeparation
platform-property-md-AuthenticationSeparation = Authentication separation

# WebSocketClient|autoConnect
platform-property-md-AutoConnect = Connect automatically

# ChartOfAccounts|autoOrderByCode
platform-property-md-AutoOrderByCode = Autoorder by code

# CommonAttribute|autoUse
platform-property-md-AutoUse = Auto-use

# BusinessProcess|autonumbering; Catalog|autonumbering; ChartOfCharacteristicTypes|autonumbering; Document|autonumbering; Task|autonumbering
platform-property-md-Autonumbering = Autonumbering

# BusinessProcess|auxiliaryChoiceForm; Catalog|auxiliaryChoiceForm; ChartOfAccounts|auxiliaryChoiceForm; ChartOfCalculationTypes|auxiliaryChoiceForm; ChartOfCharacteristicTypes|auxiliaryChoiceForm; Document|auxiliaryChoiceForm; Enum|auxiliaryChoiceForm; ExchangePlan|auxiliaryChoiceForm; Task|auxiliaryChoiceForm
platform-property-md-AuxiliaryChoiceForm = Auxiliary choice form

# Catalog|auxiliaryFolderChoiceForm
platform-property-md-AuxiliaryFolderChoiceForm = Auxiliary folder choice form

# Catalog|auxiliaryFolderForm
platform-property-md-AuxiliaryFolderForm = Auxiliary folder form

# DataProcessor|auxiliaryForm; DocumentJournal|auxiliaryForm; ExternalDataProcessor|auxiliaryForm; ExternalReport|auxiliaryForm; FilterCriterion|auxiliaryForm; Report|auxiliaryForm
platform-property-md-AuxiliaryForm = Auxiliary form

# AccountingRegister|auxiliaryListForm; AccumulationRegister|auxiliaryListForm; BusinessProcess|auxiliaryListForm; CalculationRegister|auxiliaryListForm; Catalog|auxiliaryListForm; ChartOfAccounts|auxiliaryListForm; ChartOfCalculationTypes|auxiliaryListForm; ChartOfCharacteristicTypes|auxiliaryListForm; Document|auxiliaryListForm; Enum|auxiliaryListForm; ExchangePlan|auxiliaryListForm; InformationRegister|auxiliaryListForm; Task|auxiliaryListForm
platform-property-md-AuxiliaryListForm = Auxiliary list form

# SettingsStorage|auxiliaryLoadForm
platform-property-md-AuxiliaryLoadForm = Auxiliary load form

# BusinessProcess|auxiliaryObjectForm; Catalog|auxiliaryObjectForm; ChartOfAccounts|auxiliaryObjectForm; ChartOfCalculationTypes|auxiliaryObjectForm; ChartOfCharacteristicTypes|auxiliaryObjectForm; Document|auxiliaryObjectForm; ExchangePlan|auxiliaryObjectForm; Task|auxiliaryObjectForm
platform-property-md-AuxiliaryObjectForm = Auxiliary object form

# InformationRegister|auxiliaryRecordForm
platform-property-md-AuxiliaryRecordForm = Auxiliary record editing form

# SettingsStorage|auxiliarySaveForm
platform-property-md-AuxiliarySaveForm = Auxiliary save form

# ExternalReport|auxiliarySettingsForm; Report|auxiliarySettingsForm
platform-property-md-AuxiliarySettingsForm = Auxiliary settings form

# CommonPicture|availabilityForAppearance
platform-property-md-AvailabilityForAppearance = Available for appearance

# CommonPicture|availabilityForChoice
platform-property-md-AvailabilityForChoice = Available for selection

# AccountingRegisterDimension|balance; AccountingRegisterResource|balance
platform-property-md-Balance = Balance

# ChartOfCalculationTypes|baseCalculationTypes
platform-property-md-BaseCalculationTypes = Base charts of calculation types

# CalculationRegisterDimension|baseDimension
platform-property-md-BaseDimension = Base

# CalculationRegister|basePeriod
platform-property-md-BasePeriod = Base period

# BasicDbObject|basedOn
platform-property-md-BasedOn = Based on

# Configuration|binaryDataBlockStorageUseMode
platform-property-md-BinaryDataBlockStorageUseMode = Binary data block storage use mode

# DbObjectAttribute|binaryDataStorageLocationUse; InformationRegisterResource|binaryDataStorageLocationUse; RegisterAttribute|binaryDataStorageLocationUse
platform-property-md-BinaryDataStorageLocationUse = Use storage in binary data storage

# DbObjectAttribute|binaryDataStorageLocationUseField; InformationRegisterResource|binaryDataStorageLocationUseField; RegisterAttribute|binaryDataStorageLocationUseField
platform-property-md-BinaryDataStorageLocationUseField = Field of using storage in binary data storage

# Configuration|binaryDataStorageMode
platform-property-md-BinaryDataStorageMode = Binary data storage mode

# Configuration|briefInformation
platform-property-md-BriefInformation = Brief information

# CommandGroup|category
platform-property-md-Category = Category

# ChartOfCharacteristicTypes|characteristicExtValues
platform-property-md-CharacteristicExtValues = Additional characteristic values

# BasicDbObject|characteristics; Cube|characteristics; Enum|characteristics; Table|characteristics
platform-property-md-Characteristics = Characteristics

# AccountingRegister|chartOfAccounts
platform-property-md-ChartOfAccounts = Chart of accounts

# CalculationRegister|chartOfCalculationTypes
platform-property-md-ChartOfCalculationTypes = Chart of calculation types

# BusinessProcess|checkUnique; Catalog|checkUnique; ChartOfAccounts|checkUnique; ChartOfCharacteristicTypes|checkUnique; DocumentNumerator|checkUnique; Document|checkUnique; Task|checkUnique
platform-property-md-CheckUnique = Check for uniqueness

# BasicDbObject|choiceDataGetModeOnInputByString
platform-property-md-ChoiceDataGetModeOnInputByString = Search results display mode during the input by string

# BasicFeature|choiceFoldersAndItems; Constant|choiceFoldersAndItems; Dimension|choiceFoldersAndItems
platform-property-md-ChoiceFoldersAndItems = Choice folders and items

# BasicFeature|choiceForm; Constant|choiceForm; Dimension|choiceForm; Field|choiceForm; Resource|choiceForm
platform-property-md-ChoiceForm = Choice form

# AccountingFlag|choiceHistoryOnInput; AccountingRegisterAttribute|choiceHistoryOnInput; AccountingRegisterDimension|choiceHistoryOnInput; AccountingRegisterResource|choiceHistoryOnInput; AccumulationRegisterAttribute|choiceHistoryOnInput; AccumulationRegisterDimension|choiceHistoryOnInput; AccumulationRegisterResource|choiceHistoryOnInput; AddressingAttribute|choiceHistoryOnInput; BusinessProcessAttribute|choiceHistoryOnInput; BusinessProcess|choiceHistoryOnInput; CalculationRegisterAttribute|choiceHistoryOnInput; CalculationRegisterDimension|choiceHistoryOnInput; CalculationRegisterResource|choiceHistoryOnInput; CatalogAttribute|choiceHistoryOnInput; Catalog|choiceHistoryOnInput; ChartOfAccountsAttribute|choiceHistoryOnInput; ChartOfAccounts|choiceHistoryOnInput; ChartOfCalculationTypesAttribute|choiceHistoryOnInput; ChartOfCalculationTypes|choiceHistoryOnInput; ChartOfCharacteristicTypesAttribute|choiceHistoryOnInput; ChartOfCharacteristicTypes|choiceHistoryOnInput; CommonAttribute|choiceHistoryOnInput; Constant|choiceHistoryOnInput; DataProcessorAttribute|choiceHistoryOnInput; DataProcessorTabularSectionAttribute|choiceHistoryOnInput; Dimension|choiceHistoryOnInput; DocumentAttribute|choiceHistoryOnInput; Document|choiceHistoryOnInput; Enum|choiceHistoryOnInput; ExchangePlanAttribute|choiceHistoryOnInput; ExchangePlan|choiceHistoryOnInput; ExtDimensionAccountingFlag|choiceHistoryOnInput; InformationRegisterAttribute|choiceHistoryOnInput; InformationRegisterDimension|choiceHistoryOnInput; InformationRegisterResource|choiceHistoryOnInput; ReportAttribute|choiceHistoryOnInput; ReportTabularSectionAttribute|choiceHistoryOnInput; Table|choiceHistoryOnInput; TabularSectionAttribute|choiceHistoryOnInput; TaskAttribute|choiceHistoryOnInput; Task|choiceHistoryOnInput
platform-property-md-ChoiceHistoryOnInput = Choice history on input

# Catalog|choiceMode; ChartOfCalculationTypes|choiceMode; ChartOfCharacteristicTypes|choiceMode; Enum|choiceMode; ExchangePlan|choiceMode
platform-property-md-ChoiceMode = Choice mode

# BasicFeature|choiceParameterLinks; Constant|choiceParameterLinks; Dimension|choiceParameterLinks; Field|choiceParameterLinks; Resource|choiceParameterLinks
platform-property-md-ChoiceParameterLinks = Choice parameter links

# BasicFeature|choiceParameters; Constant|choiceParameters; Dimension|choiceParameters; Field|choiceParameters; Resource|choiceParameters
platform-property-md-ChoiceParameters = Choice parameters

# CommonModule|clientManagedApplication
platform-property-md-ClientManagedApplication = Client managed application

# CommonModule|clientOrdinaryApplication
platform-property-md-ClientOrdinaryApplication = Client ordinary application

# Catalog|codeAllowedLength; ChartOfCalculationTypes|codeAllowedLength; ChartOfCharacteristicTypes|codeAllowedLength; ExchangePlan|codeAllowedLength
platform-property-md-CodeAllowedLength = Allowed code length

# Catalog|codeLength; ChartOfAccounts|codeLength; ChartOfCalculationTypes|codeLength; ChartOfCharacteristicTypes|codeLength; ExchangePlan|codeLength
platform-property-md-CodeLength = Code length

# ChartOfAccounts|codeMask
platform-property-md-CodeMask = Code mask

# Catalog|codeSeries; ChartOfAccounts|codeSeries; ChartOfCharacteristicTypes|codeSeries
platform-property-md-CodeSeries = Code series

# Catalog|codeType; ChartOfCalculationTypes|codeType
platform-property-md-CodeType = Code type

# EnumValue|color; PaletteColor|color
platform-property-md-Color = Color

# BasicCommand|commandParameterType
platform-property-md-CommandParameterType = Command parameter type

# MdObject|comment
platform-property-md-Comment = Comment

# Configuration|commonSettingsStorage
platform-property-md-CommonSettingsStorage = Common settings storage

# Configuration|compatibilityMode
platform-property-md-CompatibilityMode = Compatibility mode

# CommonAttribute|conditionalSeparation
platform-property-md-ConditionalSeparation = Conditional separation

# Configuration|configurationExtensionCompatibilityMode
platform-property-md-ConfigurationExtensionCompatibilityMode = Configuration extension compatibility mode

# Configuration|configurationExtensionPurpose
platform-property-md-ConfigurationExtensionPurpose = Configuration extension purpose

# CommonAttribute|configurationExtensionsSeparation
platform-property-md-ConfigurationExtensionsSeparation = Configuration extension separation

# Configuration|configurationInformationAddress
platform-property-md-ConfigurationInformationAddress = Configuration information address

# CommonAttribute|content; FunctionalOption|content; Subsystem|content
platform-property-md-Content = Content

# Configuration|copyright
platform-property-md-Copyright = Copyright

# AccountingRegister|correspondence
platform-property-md-Correspondence = Correspondence

# BasicDbObject|createOnInput; BasicFeature|createOnInput; Dimension|createOnInput; Field|createOnInput; Table|createOnInput
platform-property-md-CreateOnInput = Create on input

# BusinessProcess|createTaskInPrivilegedMode
platform-property-md-CreateTaskInPrivilegedMode = Create task in privileged mode

# Task|currentPerformer
platform-property-md-CurrentPerformer = Current assignee

# DataHistorySupport|dataHistory
platform-property-md-DataHistory = Data history

# AccountingRegister|dataLockControlMode; AccumulationRegister|dataLockControlMode; BasicDbObject|dataLockControlMode; CalculationRegister|dataLockControlMode; Configuration|dataLockControlMode; Constant|dataLockControlMode; ExternalDataSource|dataLockControlMode; InformationRegister|dataLockControlMode; Operation|dataLockControlMode; Recalculation|dataLockControlMode; Sequence|dataLockControlMode; Table|dataLockControlMode
platform-property-md-DataLockControlMode = Data lock control mode

# BasicDbObject|dataLockFields; Table|dataLockFields
platform-property-md-DataLockFields = Data lock fields

# CommonAttribute|dataSeparation
platform-property-md-DataSeparation = Data separation

# CommonAttribute|dataSeparationUse
platform-property-md-DataSeparationUse = Data separation usage

# CommonAttribute|dataSeparationValue
platform-property-md-DataSeparationValue = Data separation value

# Table|dataVersionField
platform-property-md-DataVersionField = Data version field

# Configuration|databaseTablespacesUseMode
platform-property-md-DatabaseTablespacesUseMode = Tablespaces usage mode

# BusinessProcess|defaultChoiceForm; Catalog|defaultChoiceForm; ChartOfAccounts|defaultChoiceForm; ChartOfCalculationTypes|defaultChoiceForm; ChartOfCharacteristicTypes|defaultChoiceForm; DimensionTable|defaultChoiceForm; Document|defaultChoiceForm; Enum|defaultChoiceForm; ExchangePlan|defaultChoiceForm; Table|defaultChoiceForm; Task|defaultChoiceForm
platform-property-md-DefaultChoiceForm = Default choice form

# Configuration|defaultCollaborationSystemUsersChoiceForm
platform-property-md-DefaultCollaborationSystemUsersChoiceForm = Main form of selection of collaboration system users

# Configuration|defaultConstantsForm
platform-property-md-DefaultConstantsForm = Default constants form

# Configuration|defaultDataHistoryChangeHistoryForm
platform-property-md-DefaultDataHistoryChangeHistoryForm = Data history change history default form

# Configuration|defaultDataHistoryVersionDataForm
platform-property-md-DefaultDataHistoryVersionDataForm = Data history version default form

# Configuration|defaultDataHistoryVersionDifferencesForm
platform-property-md-DefaultDataHistoryVersionDifferencesForm = Data history version difference default form

# Configuration|defaultDynamicListSettingsForm
platform-property-md-DefaultDynamicListSettingsForm = Default dynamic list settings form

# Catalog|defaultFolderChoiceForm
platform-property-md-DefaultFolderChoiceForm = Default folder choice form

# Catalog|defaultFolderForm; ChartOfCharacteristicTypes|defaultFolderForm
platform-property-md-DefaultFolderForm = Default folder form

# Constant|defaultForm; DataProcessor|defaultForm; DocumentJournal|defaultForm; ExternalDataProcessor|defaultForm; ExternalReport|defaultForm; FilterCriterion|defaultForm; Report|defaultForm
platform-property-md-DefaultForm = Default form

# Configuration|defaultInterface
platform-property-md-DefaultInterface = Default interface

# Configuration|defaultLanguage
platform-property-md-DefaultLanguage = Default language

# AccountingRegister|defaultListForm; AccumulationRegister|defaultListForm; BusinessProcess|defaultListForm; CalculationRegister|defaultListForm; Catalog|defaultListForm; ChartOfAccounts|defaultListForm; ChartOfCalculationTypes|defaultListForm; ChartOfCharacteristicTypes|defaultListForm; Cube|defaultListForm; DimensionTable|defaultListForm; Document|defaultListForm; Enum|defaultListForm; ExchangePlan|defaultListForm; InformationRegister|defaultListForm; Table|defaultListForm; Task|defaultListForm
platform-property-md-DefaultListForm = Default list form

# SettingsStorage|defaultLoadForm
platform-property-md-DefaultLoadForm = Default load form

# BusinessProcess|defaultObjectForm; Catalog|defaultObjectForm; ChartOfAccounts|defaultObjectForm; ChartOfCalculationTypes|defaultObjectForm; ChartOfCharacteristicTypes|defaultObjectForm; DimensionTable|defaultObjectForm; Document|defaultObjectForm; ExchangePlan|defaultObjectForm; Table|defaultObjectForm; Task|defaultObjectForm
platform-property-md-DefaultObjectForm = Default object form

# Catalog|defaultPresentation; ChartOfAccounts|defaultPresentation; ChartOfCalculationTypes|defaultPresentation; ChartOfCharacteristicTypes|defaultPresentation
platform-property-md-DefaultPresentation = Default presentation

# Cube|defaultRecordForm; Table|defaultRecordForm
platform-property-md-DefaultRecordForm = Default record form

# Configuration|defaultReportAppearanceTemplate
platform-property-md-DefaultReportAppearanceTemplate = Main report appearance template

# Configuration|defaultReportForm
platform-property-md-DefaultReportForm = Default report form

# Configuration|defaultReportSettingsForm
platform-property-md-DefaultReportSettingsForm = Default report settings form

# Configuration|defaultReportVariantForm
platform-property-md-DefaultReportVariantForm = Default report option form

# Configuration|defaultRole
platform-property-md-DefaultRole = Default role

# Configuration|defaultRoles
platform-property-md-DefaultRoles = Default roles

# Configuration|defaultRunMode
platform-property-md-DefaultRunMode = Default run mode

# SettingsStorage|defaultSaveForm
platform-property-md-DefaultSaveForm = Default save form

# Configuration|defaultSearchForm
platform-property-md-DefaultSearchForm = Default search form

# ExternalReport|defaultSettingsForm; Report|defaultSettingsForm
platform-property-md-DefaultSettingsForm = Default settings form

# Configuration|defaultStyle
platform-property-md-DefaultStyle = Default style

# ExternalReport|defaultVariantForm; Report|defaultVariantForm
platform-property-md-DefaultVariantForm = Default option form

# RegisterDimension|denyIncompleteValues
platform-property-md-DenyIncompleteValues = Deny incomplete values

# ChartOfCalculationTypes|dependenceOnCalculationTypes
platform-property-md-DependenceOnCalculationTypes = Dependence on base

# ScheduledJob|description
platform-property-md-Description = Description

# Catalog|descriptionLength; ChartOfAccounts|descriptionLength; ChartOfCalculationTypes|descriptionLength; ChartOfCharacteristicTypes|descriptionLength; ExchangePlan|descriptionLength; Task|descriptionLength
platform-property-md-DescriptionLength = Description length

# WebService|descriptorFileName
platform-property-md-DescriptorFileName = Publication File Name

# Configuration|detailedInformation
platform-property-md-DetailedInformation = Detailed information

# ExchangePlan|distributedInfoBase
platform-property-md-DistributedInfoBase = Distributed infobase

# SequenceDimension|documentMap
platform-property-md-DocumentMap = Document map

# Sequence|documents
platform-property-md-Documents = Documents

# Configuration|dynamicListsUserSettingsStorage
platform-property-md-DynamicListsUserSettingsStorage = Dynamic lists user settings storage

# BasicFeature|editFormat; Constant|editFormat; Dimension|editFormat; Field|editFormat; Resource|editFormat
platform-property-md-EditFormat = Editing format

# BusinessProcess|editType; Catalog|editType; ChartOfCharacteristicTypes|editType; ExchangePlan|editType; InformationRegister|editType; Table|editType; Task|editType
platform-property-md-EditType = Edit type

# InformationRegister|enableTotalsSliceFirst
platform-property-md-EnableTotalsSliceFirst = Enable totals slice first

# InformationRegister|enableTotalsSliceLast
platform-property-md-EnableTotalsSliceLast = Enable totals slice last

# AccountingRegister|enableTotalsSplitting; AccumulationRegister|enableTotalsSplitting
platform-property-md-EnableTotalsSplitting = Enable totals splitting

# EventSubscription|event
platform-property-md-Event = Event

# BusinessProcess|executeAfterWriteDataHistoryVersionProcessing; Catalog|executeAfterWriteDataHistoryVersionProcessing; ChartOfAccounts|executeAfterWriteDataHistoryVersionProcessing; ChartOfCalculationTypes|executeAfterWriteDataHistoryVersionProcessing; ChartOfCharacteristicTypes|executeAfterWriteDataHistoryVersionProcessing; Constant|executeAfterWriteDataHistoryVersionProcessing; Document|executeAfterWriteDataHistoryVersionProcessing; ExchangePlan|executeAfterWriteDataHistoryVersionProcessing; InformationRegister|executeAfterWriteDataHistoryVersionProcessing; Task|executeAfterWriteDataHistoryVersionProcessing
platform-property-md-ExecuteAfterWriteDataHistoryVersionProcessing = Process data upon recording data history version

# AccountingRegister|explanation; AccumulationRegister|explanation; BasicDbObject|explanation; CalculationRegister|explanation; CommonForm|explanation; Constant|explanation; Cube|explanation; DataProcessor|explanation; DimensionTable|explanation; DocumentJournal|explanation; Enum|explanation; FilterCriterion|explanation; InformationRegister|explanation; Report|explanation; Subsystem|explanation; Table|explanation
platform-property-md-Explanation = Note

# Function|expressionInDataSource; Table|expressionInDataSource
platform-property-md-ExpressionInDataSource = Expression in the data source

# AccountingRegisterResource|extDimensionAccountingFlag
platform-property-md-ExtDimensionAccountingFlag = Extra dimension accounting flag

# ChartOfAccounts|extDimensionTypes
platform-property-md-ExtDimensionTypes = Extra dimension types

# MdObject|extendedConfigurationObject
platform-property-md-ExtendedConfigurationObject = Extended configuration object

# BasicFeature|extendedEdit; Constant|extendedEdit; Dimension|extendedEdit; Field|extendedEdit; Resource|extendedEdit
platform-property-md-ExtendedEdit = Extended edit

# AccountingRegister|extendedListPresentation; AccumulationRegister|extendedListPresentation; BasicDbObject|extendedListPresentation; CalculationRegister|extendedListPresentation; Cube|extendedListPresentation; DimensionTable|extendedListPresentation; DocumentJournal|extendedListPresentation; Enum|extendedListPresentation; FilterCriterion|extendedListPresentation; InformationRegister|extendedListPresentation; Table|extendedListPresentation
platform-property-md-ExtendedListPresentation = Extended list presentation

# BasicDbObject|extendedObjectPresentation
platform-property-md-ExtendedObjectPresentation = Extended object presentation

# CommonForm|extendedPresentation; Constant|extendedPresentation; DataProcessorForm|extendedPresentation; DataProcessor|extendedPresentation; ReportForm|extendedPresentation; Report|extendedPresentation
platform-property-md-ExtendedPresentation = Extended presentation

# Cube|extendedRecordPresentation; InformationRegister|extendedRecordPresentation; Table|extendedRecordPresentation
platform-property-md-ExtendedRecordPresentation = Extended record presentation

# CommonModule|externalConnection
platform-property-md-ExternalConnection = External connection

# IntegrationService|externalIntegrationServiceAddress
platform-property-md-ExternalIntegrationServiceAddress = External integration service address

# IntegrationServiceChannel|externalIntegrationServiceChannelName
platform-property-md-ExternalIntegrationServiceChannelName = Name of the canal of external integration service

# BasicFeature|fillChecking; BasicTabularSection|fillChecking; Constant|fillChecking; Dimension|fillChecking; Field|fillChecking
platform-property-md-FillChecking = Fill check

# AccountingFlag|fillFromFillingValue; AddressingAttribute|fillFromFillingValue; CommonAttribute|fillFromFillingValue; DataProcessorTabularSectionAttribute|fillFromFillingValue; DbObjectAttribute|fillFromFillingValue; ExtDimensionAccountingFlag|fillFromFillingValue; Field|fillFromFillingValue; InformationRegisterAttribute|fillFromFillingValue; InformationRegisterDimension|fillFromFillingValue; InformationRegisterResource|fillFromFillingValue; ReportTabularSectionAttribute|fillFromFillingValue
platform-property-md-FillFromFillingValue = Fill from filling data

# AccountingFlag|fillValue; AddressingAttribute|fillValue; CommonAttribute|fillValue; DataProcessorTabularSectionAttribute|fillValue; DbObjectAttribute|fillValue; ExtDimensionAccountingFlag|fillValue; Field|fillValue; InformationRegisterAttribute|fillValue; InformationRegisterDimension|fillValue; InformationRegisterResource|fillValue; ReportTabularSectionAttribute|fillValue
platform-property-md-FillValue = Fill value

# Catalog|foldersOnTop; ChartOfCharacteristicTypes|foldersOnTop
platform-property-md-FoldersOnTop = Folders on top

# Configuration|formDataSettingsStorage
platform-property-md-FormDataSettingsStorage = Form data settings storage

# BasicForm|formType
platform-property-md-FormType = Form type

# BasicFeature|format; Constant|format; Dimension|format; Field|format; Resource|format
platform-property-md-Format = Format

# AccountingRegister|fullTextSearch; AccumulationRegister|fullTextSearch; AddressingAttribute|fullTextSearch; BasicDbObject|fullTextSearch; CalculationRegister|fullTextSearch; DbObjectAttribute|fullTextSearch; InformationRegister|fullTextSearch; RegisterAttribute|fullTextSearch; RegisterDimension|fullTextSearch; RegisterResource|fullTextSearch; TabularSectionAttribute|fullTextSearch
platform-property-md-FullTextSearch = Full text search

# BasicDbObject|fullTextSearchOnInputByString
platform-property-md-FullTextSearchOnInputByString = Full text search on input by string

# CommonModule|global
platform-property-md-Global = Global

# BasicCommand|group
platform-property-md-Group = Group

# Method|httpMethod
platform-property-md-HTTPMethod = HTTP method

# EventSubscription|handler; Method|handler
platform-property-md-Handler = Handler

# WebSocketClient|headers
platform-property-md-Headers = Titles

# AccountingRegister|help; AccumulationRegister|help; BasicDbObject|help; BasicForm|help; CalculationRegister|help; CommonCommand|help; Configuration|help; Cube|help; DataProcessor|help; DimensionTable|help; DocumentJournal|help; ExternalDataProcessor|help; ExternalReport|help; InformationRegister|help; Report|help; Subsystem|help; Table|help
platform-property-md-Help = Help content

# Catalog|hierarchical; ChartOfCharacteristicTypes|hierarchical
platform-property-md-Hierarchical = Hierarchical

# DimensionTable|hierarchyNameInDataSource
platform-property-md-HierarchyNameInDataSource = Hierarchy name in the data source

# Catalog|hierarchyType
platform-property-md-HierarchyType = Hierarchy type

# ExchangePlan|includeConfigurationExtensions
platform-property-md-IncludeConfigurationExtensions = Include configuration extensions

# AccountingRegister|includeHelpInContents; AccumulationRegister|includeHelpInContents; BasicDbObject|includeHelpInContents; BasicForm|includeHelpInContents; CalculationRegister|includeHelpInContents; CommonCommand|includeHelpInContents; Configuration|includeHelpInContents; DataProcessor|includeHelpInContents; DocumentJournal|includeHelpInContents; InformationRegister|includeHelpInContents; Report|includeHelpInContents; Subsystem|includeHelpInContents
platform-property-md-IncludeHelpInContents = Include help in contents

# Subsystem|includeInCommandInterface
platform-property-md-IncludeInCommandInterface = Include in the command interface

# AddressingAttribute|indexing; Column|indexing; DbObjectAttribute|indexing; InformationRegisterResource|indexing; RegisterAttribute|indexing; RegisterDimension|indexing; TabularSectionAttribute|indexing
platform-property-md-Indexing = Indexing

# InformationRegister|informationRegisterPeriodicity
platform-property-md-InformationRegisterPeriodicity = Periodicity

# BasicDbObject|inputByString
platform-property-md-InputByString = Input by string

# Configuration|interfaceCompatibilityMode
platform-property-md-InterfaceCompatibilityMode = Interface compatibility mode

# Configuration|keepMappingToExtendedConfigurationObjectsByIDs
platform-property-md-KeepMappingToExtendedConfigurationObjectsByIDs = Support mapping to extended configuration objects by internal IDs

# ScheduledJob|key
platform-property-md-Key = Key

# Table|keyFields
platform-property-md-KeyFields = Fields of options

# Language|languageCode
platform-property-md-LanguageCode = Language code

# RecalculationDimension|leadingRegisterData
platform-property-md-LeadingRegisterData = Leading register data

# Catalog|levelCount
platform-property-md-LevelCount = Level count

# DimensionTable|levelNumber
platform-property-md-LevelNumber = Level number

# Catalog|limitLevelCount
platform-property-md-LimitLevelCount = Limit level count

# DbObjectTabularSection|lineNumberLength
platform-property-md-LineNumberLength = Line number length

# BasicFeature|linkByType; Constant|linkByType; Dimension|linkByType
platform-property-md-LinkByType = Link by type

# AccountingRegister|listPresentation; AccumulationRegister|listPresentation; BasicDbObject|listPresentation; CalculationRegister|listPresentation; Cube|listPresentation; DimensionTable|listPresentation; DocumentJournal|listPresentation; Enum|listPresentation; FilterCriterion|listPresentation; InformationRegister|listPresentation; Table|listPresentation
platform-property-md-ListPresentation = List presentation

# FunctionalOption|location
platform-property-md-Location = Location

# WSReference|locationURL
platform-property-md-LocationURL = Source URL

# Configuration|logo
platform-property-md-Logo = Logo

# Task|mainAddressingAttribute
platform-property-md-MainAddressingAttribute = Main addressing attribute

# Configuration|mainClientApplicationWindowMode
platform-property-md-MainClientApplicationWindowMode = Main client application window mode

# ExternalReport|mainDataCompositionSchema; Report|mainDataCompositionSchema
platform-property-md-MainDataCompositionSchema = Main data composition schema

# InformationRegisterDimension|mainFilter
platform-property-md-MainFilter = Main filter

# InformationRegister|mainFilterOnPeriod
platform-property-md-MainFilterOnPeriod = Main filter on period

# Configuration|mainSectionPicture
platform-property-md-MainSectionPicture = Main section picture

# BasicFeature|markNegatives; Constant|markNegatives; Dimension|markNegatives; Resource|markNegatives
platform-property-md-MarkNegatives = Mark negatives

# BasicFeature|mask; Constant|mask; Dimension|mask; Field|mask; Resource|mask
platform-property-md-Mask = Mask

# InformationRegisterDimension|master
platform-property-md-Master = Master

# ChartOfAccounts|maxExtDimensionCount
platform-property-md-MaxExtDimensionCount = Maximum extra dimension count

# BasicFeature|maxValue; Constant|maxValue; Dimension|maxValue; Field|maxValue
platform-property-md-MaxValue = Maximum value

# IntegrationServiceChannel|messageDirection
platform-property-md-MessageDirection = Message direction

# ScheduledJob|methodName
platform-property-md-MethodName = Method name

# BasicFeature|minValue; Constant|minValue; Dimension|minValue; Field|minValue
platform-property-md-MinValue = Minimum value

# Configuration|mobileApplicationUrls
platform-property-md-MobileApplicationURLs = Mobile application navigation links

# Configuration|modalityUseMode
platform-property-md-ModalityUseMode = Modality use mode

# BasicCommand|modifiesData
platform-property-md-ModifiesData = Modifies data

# Sequence|moveBoundaryOnPosting
platform-property-md-MoveBoundaryOnPosting = Move the boundary when posting

# BasicFeature|multiLine; Constant|multiLine
platform-property-md-MultiLine = Multi line

# MdObject|name
platform-property-md-Name = Name

# DimensionTable|nameInDataSource; Field|nameInDataSource; Resource|nameInDataSource
platform-property-md-NameInDataSource = Name in data source

# Configuration|namePrefix
platform-property-md-NamePrefix = Name prefix

# WebService|namespace; XDTOPackage|namespace
platform-property-md-Namespace = Namespace URI

# Operation|nillable; Parameter|nillable
platform-property-md-Nillable = Value can be blank

# BusinessProcess|numberAllowedLength; DocumentNumerator|numberAllowedLength; Document|numberAllowedLength; Task|numberAllowedLength
platform-property-md-NumberAllowedLength = Allowed number length

# BusinessProcess|numberLength; DocumentNumerator|numberLength; Document|numberLength; Task|numberLength
platform-property-md-NumberLength = Number length

# BusinessProcess|numberPeriodicity; DocumentNumerator|numberPeriodicity; Document|numberPeriodicity
platform-property-md-NumberPeriodicity = Periodicity

# BusinessProcess|numberType; DocumentNumerator|numberType; Document|numberType; Task|numberType
platform-property-md-NumberType = Number type

# Document|numerator
platform-property-md-Numerator = Numerator

# Configuration|objectAutonumerationMode
platform-property-md-ObjectAutonumerationMode = Object autonumeration mode

# MdObject|objectBelonging
platform-property-md-ObjectBelonging = Object belonging

# BasicDbObject|objectPresentation
platform-property-md-ObjectPresentation = Object presentation

# BasicCommand|onMainServerUnavalableBehavior
platform-property-md-OnMainServerUnavalableBehavior = Behavior in case the main server is not available

# ChartOfAccounts|orderLength
platform-property-md-OrderLength = Order length

# Catalog|owners
platform-property-md-Owners = Owners

# BasicCommand|parameterUseMode
platform-property-md-ParameterUseMode = Parameter use mode

# Table|parentField
platform-property-md-ParentField = Parent field

# WebSocketClient|password
platform-property-md-Password = Password

# BasicFeature|passwordMode; Constant|passwordMode; Dimension|passwordMode; Field|passwordMode; Resource|passwordMode
platform-property-md-PasswordMode = Password mode

# AccountingRegister|periodAdjustmentLength
platform-property-md-PeriodAdjustmentLength = Period adjustment length

# CalculationRegister|periodicity
platform-property-md-Periodicity = Periodicity

# BasicCommand|picture; Bot|picture; CommandGroup|picture; Subsystem|picture
platform-property-md-Picture = Picture

# Document|postInPrivilegedMode
platform-property-md-PostInPrivilegedMode = Post in privileged mode

# Document|posting
platform-property-md-Posting = Posting

# Catalog|predefined; ChartOfCalculationTypes|predefined; ChartOfCharacteristicTypes|predefined
platform-property-md-Predefined = Predefined

# Catalog|predefinedDataUpdate; ChartOfCalculationTypes|predefinedDataUpdate; ChartOfCharacteristicTypes|predefinedDataUpdate
platform-property-md-PredefinedDataUpdate = Predefined data update

# DimensionTable|presentationField; Table|presentationField
platform-property-md-PresentationField = Fields of representation

# CommonModule|privileged
platform-property-md-Privileged = Privileged

# FunctionalOption|privilegedGetMode
platform-property-md-PrivilegedGetMode = Privileged get mode

# Operation|procedureName
platform-property-md-ProcedureName = Procedure name

# BasicFeature|quickChoice; Catalog|quickChoice; ChartOfAccounts|quickChoice; ChartOfCalculationTypes|quickChoice; ChartOfCharacteristicTypes|quickChoice; Constant|quickChoice; DimensionTable|quickChoice; Dimension|quickChoice; Enum|quickChoice; ExchangePlan|quickChoice; Field|quickChoice; Resource|quickChoice; Table|quickChoice
platform-property-md-QuickChoice = Quick choice

# Table|readOnly
platform-property-md-ReadOnly = Read-only

# Document|realTimePosting
platform-property-md-RealTimePosting = Real time posting

# IntegrationServiceChannel|receiveMessageProcessing
platform-property-md-ReceiveMessageProcessing = Message receive handler

# Cube|recordPresentation; InformationRegister|recordPresentation; Table|recordPresentation
platform-property-md-RecordPresentation = Record presentation

# Column|references
platform-property-md-References = References

# RecalculationDimension|registerDimension
platform-property-md-RegisterDimension = Register dimension

# Document|registerRecords
platform-property-md-RegisterRecords = Register records

# Document|registerRecordsDeletion
platform-property-md-RegisterRecordsDeletion = Register records deletion

# SequenceDimension|registerRecordsMap
platform-property-md-RegisterRecordsMap = Register records map

# Document|registerRecordsWritingOnPost
platform-property-md-RegisterRecordsWritingOnPost = Register records writing on post

# AccumulationRegister|registerType
platform-property-md-RegisterType = Register type

# DocumentJournal|registeredDocuments
platform-property-md-RegisteredDocuments = Registered documents

# Configuration|reportsUserSettingsStorage
platform-property-md-ReportsUserSettingsStorage = Reports user settings storage

# Configuration|reportsVariantsStorage
platform-property-md-ReportsVariantsStorage = Reports variants storage

# BasicCommand|representation; CommandGroup|representation
platform-property-md-Representation = Representation

# Configuration|requiredMobileApplicationPermissions8315
platform-property-md-RequiredMobileApplicationPermissions = Required mobile application permissions

# Configuration|requiredMobileApplicationPermissions8315
platform-property-md-RequiredMobileApplicationPermissions8315 = Required mobile application permissions

# ScheduledJob|restartCountOnFailure
platform-property-md-RestartCountOnFailure = Restart count on failure

# ScheduledJob|restartIntervalOnFailure
platform-property-md-RestartIntervalOnFailure = Restart interval on failure

# Function|returnValue
platform-property-md-ReturnValue = Returns a value

# CommonModule|returnValuesReuse
platform-property-md-ReturnValuesReuse = Reuse return values

# HTTPService|reuseSessions; WebService|reuseSessions
platform-property-md-ReuseSessions = Session reuse

# HTTPService|rootURL
platform-property-md-RootURL = Root URL

# CalculationRegister|schedule
platform-property-md-Schedule = Schedule

# CalculationRegister|scheduleDate
platform-property-md-ScheduleDate = Schedule date

# CalculationRegisterAttribute|scheduleLink; CalculationRegisterDimension|scheduleLink
platform-property-md-ScheduleLink = Link to schedule

# CalculationRegister|scheduleValue
platform-property-md-ScheduleValue = Schedule value

# Configuration|scriptVariant
platform-property-md-ScriptVariant = Script variant

# BasicDbObject|searchStringModeOnInputByString
platform-property-md-SearchStringModeOnInputByString = Search method for input by string

# CommonAttribute|separatedDataUse
platform-property-md-SeparatedDataUse = Usage of split data

# Document|sequenceFilling
platform-property-md-SequenceFilling = Sequence filling

# CommonModule|server
platform-property-md-Server = Server

# CommonModule|serverCall
platform-property-md-ServerCall = Server call

# WebSocketClient|serverURL
platform-property-md-ServerURL = Server URL

# HTTPService|sessionMaxAge; WebService|sessionMaxAge
platform-property-md-SessionMaxAge = Session lifetime

# ExternalReport|settingsStorage; Report|settingsStorage
platform-property-md-SettingsStorage = Settings storage

# BasicCommand|shortcut
platform-property-md-Shortcut = Shortcut

# EventSubscription|source
platform-property-md-Source = Source

# Configuration|splash
platform-property-md-Splash = Splash

# Configuration|standaloneConfigurationRestrictionRoles
platform-property-md-StandaloneConfigurationRestrictionRoles = Limiting roles of the offline mobile application

# AccountingRegister|standardAttributes; AccumulationRegister|standardAttributes; BasicDbObject|standardAttributes; BasicTabularSection|standardAttributes; CalculationRegister|standardAttributes; DocumentJournal|standardAttributes; Enum|standardAttributes; InformationRegister|standardAttributes
platform-property-md-StandardAttributes = Standard attributes

# ChartOfAccounts|standardTabularSections; ChartOfCalculationTypes|standardTabularSections
platform-property-md-StandardTabularSections = Standard tabular sections

# Catalog|subordinationUse
platform-property-md-SubordinationUse = Subordination use

# Configuration|synchronousPlatformExtensionAndAddInCallUseMode
platform-property-md-SynchronousPlatformExtensionAndAddInCallUseMode = Synchronous call usage mode for platform extensions and add-ins

# MdObject|synonym
platform-property-md-Synonym = Synonym

# Table|tableDataType
platform-property-md-TableDataType = External data source table data type

# Table|tableType
platform-property-md-TableType = Table type

# BusinessProcess|task
platform-property-md-Task = Task

# Task|taskNumberAutoPrefix
platform-property-md-TaskNumberAutoPrefix = Task number auto prefix

# URLTemplate|template
platform-property-md-Template = Template

# BasicTemplate|templateType
platform-property-md-TemplateType = Template type

# WebSocketClient|timeout
platform-property-md-Timeout = Timeout (sec.)

# BasicCommand|toolTip; BasicFeature|toolTip; BasicTabularSection|toolTip; CommandGroup|toolTip; Constant|toolTip; Dimension|toolTip; Field|toolTip; Resource|toolTip
platform-property-md-ToolTip = Tooltip

# IntegrationServiceChannel|transactioned; Operation|transactioned
platform-property-md-Transactioned = In transaction

# Table|transactionsIsolationLevel
platform-property-md-TransactionsIsolationLevel = Transactions isolation level

# Parameter|transferDirection
platform-property-md-TransferDirection = Transfer direction

# BasicFeature|type; ChartOfCharacteristicTypes|type; Constant|type; DefinedType|type; Dimension|type; Field|type; FilterCriterion|type; Function|type; Resource|type; SequenceDimension|type; SessionParameter|type
platform-property-md-Type = Type

# InformationRegisterDimension|typeReductionMode
platform-property-md-TypeReductionMode = Type shrinking mode

# Configuration|urlExternalDataStorage
platform-property-md-URLExternalDataStorage = Storage of external URL data

# DimensionTable|unfilledParentValue; Table|unfilledParentValue
platform-property-md-UnfilledParentValue = Value of unfilled parent

# Document|unpostInPrivilegedMode
platform-property-md-UnpostInPrivilegedMode = Unpost in privileged mode

# Configuration|updateCatalogAddress
platform-property-md-UpdateCatalogAddress = Update catalog address

# BusinessProcess|updateDataHistoryImmediatelyAfterWrite; Catalog|updateDataHistoryImmediatelyAfterWrite; ChartOfAccounts|updateDataHistoryImmediatelyAfterWrite; ChartOfCalculationTypes|updateDataHistoryImmediatelyAfterWrite; ChartOfCharacteristicTypes|updateDataHistoryImmediatelyAfterWrite; Constant|updateDataHistoryImmediatelyAfterWrite; Document|updateDataHistoryImmediatelyAfterWrite; ExchangePlan|updateDataHistoryImmediatelyAfterWrite; InformationRegister|updateDataHistoryImmediatelyAfterWrite; Task|updateDataHistoryImmediatelyAfterWrite
platform-property-md-UpdateDataHistoryImmediatelyAfterWrite = Update data history immediately after writing

# FunctionalOptionsParameter|use; HierarchicalDbObjectAttribute|use; HierarchicalDbObjectTabularSection|use; ScheduledJob|use
platform-property-md-Use = Use

# AccumulationRegisterDimension|useInTotals
platform-property-md-UseInTotals = Use in totals

# Configuration|useManagedFormInOrdinaryApplication
platform-property-md-UseManagedFormInOrdinaryApplication = Use managed form in ordinary application

# WebSocketClient|useOSAuthentication
platform-property-md-UseOSAuthentication = Use OS authentication

# WebSocketClient|useOSProxy
platform-property-md-UseOSProxy = Use OS proxy

# Subsystem|useOneCommand
platform-property-md-UseOneCommand = Use one command

# Configuration|useOrdinaryFormInManagedApplication
platform-property-md-UseOrdinaryFormInManagedApplication = Use ordinary form in managed application

# BasicForm|usePurposes; Configuration|usePurposes
platform-property-md-UsePurposes = Use purposes

# AccountingRegister|useStandardCommands; AccumulationRegister|useStandardCommands; BasicDbObject|useStandardCommands; CalculationRegister|useStandardCommands; CommonForm|useStandardCommands; Constant|useStandardCommands; Cube|useStandardCommands; DataProcessor|useStandardCommands; DimensionTable|useStandardCommands; DocumentJournal|useStandardCommands; Enum|useStandardCommands; FilterCriterion|useStandardCommands; InformationRegister|useStandardCommands; Report|useStandardCommands; Table|useStandardCommands
platform-property-md-UseStandardCommands = Include in the command interface

# Configuration|usedMobileApplicationFunctionalities
platform-property-md-UsedMobileApplicationFunctionalities = Used mobile application functionality

# WebSocketClient|user
platform-property-md-User = User

# CommonAttribute|usersSeparation
platform-property-md-UsersSeparation = User separation

# StyleItem|value
platform-property-md-Value = Value

# ExternalReport|variantsStorage; Report|variantsStorage
platform-property-md-VariantsStorage = Variants storage

# Configuration|vendor
platform-property-md-Vendor = Vendor

# Configuration|vendorInformationAddress
platform-property-md-VendorInformationAddress = Vendor information address

# Configuration|version
platform-property-md-Version = Version

# InformationRegister|writeMode
platform-property-md-WriteMode = Write mode

# WebService|xdtoPackages
platform-property-md-XDTOPackages = XDTO Packages

# Operation|xdtoReturningValueType
platform-property-md-XDTOReturningValueType = Returning value type

# Parameter|xdtoValueType
platform-property-md-XDTOValueType = Value type

# ExchangePlan|defaultPresentation
platform-property-md-exchange-plan-DefaultPresentation = Main presentation of exchange plan

# InformationRegister|defaultRecordForm
platform-property-md-information-register-DefaultRecordForm = Default record editing form

# Sequence|registerRecords
platform-property-md-sequence-RegisterRecords = Register records

# StyleItem|type
platform-property-md-style-item-Type = Type

# Task|defaultPresentation
platform-property-md-task-DefaultPresentation = Default presentation

# ChartOfAccountsPredefinedItem|accountType
platform-property-predef-AccountType = Account type

# Reviewed Designer XML structural field
platform-property-predef-AccountingFlag = Accounting flag

# ChartOfAccountsPredefinedItem|accountingFlags
platform-property-predef-AccountingFlags = Accounting flags

# ChartOfCalculationTypesPredefinedItem|actionPeriodIsBase
platform-property-predef-ActionPeriodIsBase = Action period is base period

# ChartOfCalculationTypesPredefinedItem|base
platform-property-predef-Base = Base

# ChartOfCalculationTypesPredefinedItem|code
platform-property-predef-Code = Code

# PredefinedItem|description
platform-property-predef-Description = Description

# ChartOfCalculationTypesPredefinedItem|displaced
platform-property-predef-Displaced = Displaced

# Reviewed Designer XML structural field
platform-property-predef-ExtDimensionAccountingFlag = Extra dimension accounting flag

# Reviewed Designer XML structural field
platform-property-predef-ExtDimensionType = Extra dimension type

# ChartOfAccountsPredefinedItem|extDimensionTypes
platform-property-predef-ExtDimensionTypes = Extra dimension types

# ChartOfCharacteristicTypesPredefinedItem|isFolder
platform-property-predef-IsFolder = Is folder

# Reviewed Designer XML structural field
platform-property-predef-Item = Item

# ChartOfCalculationTypesPredefinedItem|leading
platform-property-predef-Leading = Leading

# PredefinedItem|name
platform-property-predef-Name = Name

# ChartOfAccountsPredefinedItem|offBalance
platform-property-predef-OffBalance = Off-balance

# ChartOfAccountsPredefinedItem|order
platform-property-predef-Order = Order

# BasicFeature|type; ChartOfCharacteristicTypes|type; Constant|type; DefinedType|type; Dimension|type; Field|type; FilterCriterion|type; Function|type; Resource|type; SequenceDimension|type; SessionParameter|type
platform-property-predef-Type = Type

# Reviewed Designer XML structural field
platform-property-readable-AutoRecord = Automatic registration

# Reviewed Designer XML structural field
platform-property-readable-Characteristic = Characteristic

# CharacteristicsDescription|characteristicTypes
platform-property-readable-CharacteristicTypes = Types of characteristics

# CharacteristicsDescription|characteristicValues
platform-property-readable-CharacteristicValues = Characteristic values

# Reviewed Designer XML structural field
platform-property-readable-CheckState = Check state

# BasicFeature|choiceForm; Constant|choiceForm; Dimension|choiceForm; Field|choiceForm; Resource|choiceForm
platform-property-readable-ChoiceForm = Choice form

# AccountingFlag|choiceHistoryOnInput; AccountingRegisterAttribute|choiceHistoryOnInput; AccountingRegisterDimension|choiceHistoryOnInput; AccountingRegisterResource|choiceHistoryOnInput; AccumulationRegisterAttribute|choiceHistoryOnInput; AccumulationRegisterDimension|choiceHistoryOnInput; AccumulationRegisterResource|choiceHistoryOnInput; AddressingAttribute|choiceHistoryOnInput; BusinessProcessAttribute|choiceHistoryOnInput; BusinessProcess|choiceHistoryOnInput; CalculationRegisterAttribute|choiceHistoryOnInput; CalculationRegisterDimension|choiceHistoryOnInput; CalculationRegisterResource|choiceHistoryOnInput; CatalogAttribute|choiceHistoryOnInput; Catalog|choiceHistoryOnInput; ChartOfAccountsAttribute|choiceHistoryOnInput; ChartOfAccounts|choiceHistoryOnInput; ChartOfCalculationTypesAttribute|choiceHistoryOnInput; ChartOfCalculationTypes|choiceHistoryOnInput; ChartOfCharacteristicTypesAttribute|choiceHistoryOnInput; ChartOfCharacteristicTypes|choiceHistoryOnInput; CommonAttribute|choiceHistoryOnInput; Constant|choiceHistoryOnInput; DataProcessorAttribute|choiceHistoryOnInput; DataProcessorTabularSectionAttribute|choiceHistoryOnInput; Dimension|choiceHistoryOnInput; DocumentAttribute|choiceHistoryOnInput; Document|choiceHistoryOnInput; Enum|choiceHistoryOnInput; ExchangePlanAttribute|choiceHistoryOnInput; ExchangePlan|choiceHistoryOnInput; ExtDimensionAccountingFlag|choiceHistoryOnInput; InformationRegisterAttribute|choiceHistoryOnInput; InformationRegisterDimension|choiceHistoryOnInput; InformationRegisterResource|choiceHistoryOnInput; ReportAttribute|choiceHistoryOnInput; ReportTabularSectionAttribute|choiceHistoryOnInput; Table|choiceHistoryOnInput; TabularSectionAttribute|choiceHistoryOnInput; TaskAttribute|choiceHistoryOnInput; Task|choiceHistoryOnInput
platform-property-readable-ChoiceHistoryOnInput = Choice history on input

# BasicFeature|choiceParameterLinks; Constant|choiceParameterLinks; Dimension|choiceParameterLinks; Field|choiceParameterLinks; Resource|choiceParameterLinks
platform-property-readable-ChoiceParameterLinks = Choice parameter links

# BasicFeature|choiceParameters; Constant|choiceParameters; Dimension|choiceParameters; Field|choiceParameters; Resource|choiceParameters
platform-property-readable-ChoiceParameters = Choice parameters

# MdObject|comment
platform-property-readable-Comment = Comment

# CommonAttribute|conditionalSeparation
platform-property-readable-ConditionalSeparation = Conditional separation

# BasicDbObject|createOnInput; BasicFeature|createOnInput; Dimension|createOnInput; Field|createOnInput; Table|createOnInput
platform-property-readable-CreateOnInput = Create on input

# DataHistorySupport|dataHistory
platform-property-readable-DataHistory = Data history

# Reviewed Designer XML structural field
platform-property-readable-DataPath = Data path

# CharacteristicsDescription|dataPathField
platform-property-readable-DataPathField = Data path field

# BasicFeature|editFormat; Constant|editFormat; Dimension|editFormat; Field|editFormat; Resource|editFormat
platform-property-readable-EditFormat = Editing format

# BasicFeature|extendedEdit; Constant|extendedEdit; Dimension|extendedEdit; Field|extendedEdit; Resource|extendedEdit
platform-property-readable-ExtendedEdit = Extended edit

# Reviewed Designer XML structural field
platform-property-readable-Field = Field

# BasicFeature|fillChecking; BasicTabularSection|fillChecking; Constant|fillChecking; Dimension|fillChecking; Field|fillChecking
platform-property-readable-FillChecking = Fill check

# AccountingFlag|fillFromFillingValue; AddressingAttribute|fillFromFillingValue; CommonAttribute|fillFromFillingValue; DataProcessorTabularSectionAttribute|fillFromFillingValue; DbObjectAttribute|fillFromFillingValue; ExtDimensionAccountingFlag|fillFromFillingValue; Field|fillFromFillingValue; InformationRegisterAttribute|fillFromFillingValue; InformationRegisterDimension|fillFromFillingValue; InformationRegisterResource|fillFromFillingValue; ReportTabularSectionAttribute|fillFromFillingValue
platform-property-readable-FillFromFillingValue = Fill from filling data

# AccountingFlag|fillValue; AddressingAttribute|fillValue; CommonAttribute|fillValue; DataProcessorTabularSectionAttribute|fillValue; DbObjectAttribute|fillValue; ExtDimensionAccountingFlag|fillValue; Field|fillValue; InformationRegisterAttribute|fillValue; InformationRegisterDimension|fillValue; InformationRegisterResource|fillValue; ReportTabularSectionAttribute|fillValue
platform-property-readable-FillValue = Fill value

# BasicFeature|format; Constant|format; Dimension|format; Field|format; Resource|format
platform-property-readable-Format = Format

# AccountingRegister|fullTextSearch; AccumulationRegister|fullTextSearch; AddressingAttribute|fullTextSearch; BasicDbObject|fullTextSearch; CalculationRegister|fullTextSearch; DbObjectAttribute|fullTextSearch; InformationRegister|fullTextSearch; RegisterAttribute|fullTextSearch; RegisterDimension|fullTextSearch; RegisterResource|fullTextSearch; TabularSectionAttribute|fullTextSearch
platform-property-readable-FullTextSearch = Full text search

# Reviewed Designer XML structural field
platform-property-readable-Header = Header

# Reviewed Designer XML structural field
platform-property-readable-Item = Item

# CharacteristicsDescription|keyField
platform-property-readable-KeyField = Field of option

# Reviewed Designer XML structural field
platform-property-readable-Link = Link

# BasicFeature|linkByType; Constant|linkByType; Dimension|linkByType
platform-property-readable-LinkByType = Link by type

# TypeLink|linkItem
platform-property-readable-LinkItem = Link item

# Reviewed Designer XML structural field
platform-property-readable-LoadTransparent = Load as transparent

# BasicFeature|markNegatives; Constant|markNegatives; Dimension|markNegatives; Resource|markNegatives
platform-property-readable-MarkNegatives = Mark negatives

# BasicFeature|mask; Constant|mask; Dimension|mask; Field|mask; Resource|mask
platform-property-readable-Mask = Mask

# BasicFeature|maxValue; Constant|maxValue; Dimension|maxValue; Field|maxValue
platform-property-readable-MaxValue = Maximum value

# Reviewed Designer XML structural field
platform-property-readable-Metadata = Metadata object

# BasicFeature|minValue; Constant|minValue; Dimension|minValue; Field|minValue
platform-property-readable-MinValue = Minimum value

# BasicFeature|multiLine; Constant|multiLine
platform-property-readable-MultiLine = Multi line

# CharacteristicsDescription|multipleValuesKeyField
platform-property-readable-MultipleValuesKeyField = Multiple values key field

# CharacteristicsDescription|multipleValuesOrderField
platform-property-readable-MultipleValuesOrderField = Multiple values order field

# CharacteristicsDescription|multipleValuesUseField
platform-property-readable-MultipleValuesUseField = Multiple value use field

# MdObject|name
platform-property-readable-Name = Name

# Reviewed Designer XML structural field
platform-property-readable-Object = Object

# CharacteristicsDescription|objectField
platform-property-readable-ObjectField = Object field

# BasicFeature|passwordMode; Constant|passwordMode; Dimension|passwordMode; Field|passwordMode; Resource|passwordMode
platform-property-readable-PasswordMode = Password mode

# Reviewed Designer XML structural field
platform-property-readable-Presentation = Presentation

# BasicFeature|quickChoice; Catalog|quickChoice; ChartOfAccounts|quickChoice; ChartOfCalculationTypes|quickChoice; ChartOfCharacteristicTypes|quickChoice; Constant|quickChoice; DimensionTable|quickChoice; Dimension|quickChoice; Enum|quickChoice; ExchangePlan|quickChoice; Field|quickChoice; Resource|quickChoice; Table|quickChoice
platform-property-readable-QuickChoice = Quick choice

# Reviewed Designer XML structural field
platform-property-readable-Ref = Reference

# Reviewed Designer XML structural field
platform-property-readable-StandardAttribute = Standard attribute

# StandardTabularSectionDescription|standardAttributes
platform-property-readable-StandardAttributes = Standard attributes

# Reviewed Designer XML structural field
platform-property-readable-StandardTabularSection = Standard tabular section

# MdObject|synonym
platform-property-readable-Synonym = Synonym

# BasicCommand|toolTip; BasicFeature|toolTip; BasicTabularSection|toolTip; CommandGroup|toolTip; Constant|toolTip; Dimension|toolTip; Field|toolTip; Resource|toolTip
platform-property-readable-ToolTip = Tooltip

# CharacteristicsDescription|typeField
platform-property-readable-TypeField = Field of type

# InformationRegisterDimension|typeReductionMode
platform-property-readable-TypeReductionMode = Type shrinking mode

# CharacteristicsDescription|typesFilterField
platform-property-readable-TypesFilterField = Field of type selection

# CharacteristicsDescription|typesFilterValue
platform-property-readable-TypesFilterValue = Value of type selection

# FunctionalOptionsParameter|use; HierarchicalDbObjectAttribute|use; HierarchicalDbObjectTabularSection|use; ScheduledJob|use
platform-property-readable-Use = Use

# StyleItem|value
platform-property-readable-Value = Value

# ChoiceParameterLink|changeMode
platform-property-readable-ValueChange = Change mode

# CharacteristicsDescription|valueField
platform-property-readable-ValueField = Field of value
