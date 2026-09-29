## 1C platform captions. See docs/platform-catalog.md for sources and version scope.

## Metadata tree
tree-collection-configuration = Конфигурация
tree-collection-accounting-register = Регистры бухгалтерии
tree-collection-accumulation-register = Регистры накопления
tree-collection-bot = Боты
tree-collection-business-process = Бизнес-процессы
tree-collection-calculation-register = Регистры расчёта
tree-collection-catalog = Справочники
tree-collection-chart-of-accounts = Планы счетов
tree-collection-chart-of-calculation-types = Планы видов расчёта
tree-collection-chart-of-characteristic-types = Планы видов характеристик
tree-collection-command-group = Группы команд
tree-collection-common-attribute = Общие реквизиты
tree-collection-common-command = Общие команды
tree-collection-common-form = Общие формы
tree-collection-common-module = Общие модули
tree-collection-common-picture = Общие картинки
tree-collection-common-template = Общие макеты
tree-collection-constant = Константы
tree-collection-data-processor = Обработки
tree-collection-defined-type = Определяемые типы
tree-collection-document-journal = Журналы документов
tree-collection-document-numerator = Нумераторы
tree-collection-document = Документы
tree-collection-enum = Перечисления
tree-collection-event-subscription = Подписки на события
tree-collection-exchange-plan = Планы обмена
tree-collection-external-data-source = Внешние источники данных
tree-collection-filter-criterion = Критерии отбора
tree-collection-functional-option = Функциональные опции
tree-collection-functional-option-parameter = Параметры функциональных опций
tree-collection-http-service = HTTP-сервисы
tree-collection-information-register = Регистры сведений
tree-collection-integration-service = Сервисы интеграции
tree-collection-language = Языки
tree-collection-report = Отчёты
tree-collection-role = Роли
tree-collection-scheduled-job = Регламентные задания
tree-collection-sequence = Последовательности
tree-collection-session-parameter = Параметры сеанса
tree-collection-settings-storage = Хранилища настроек
tree-collection-style-item = Элементы стиля
tree-collection-style = Стили
tree-collection-subsystem = Подсистемы
tree-collection-task = Задачи
tree-collection-web-service = Web-сервисы
tree-collection-ws-reference = WS-ссылки
tree-collection-xdto-package = Пакеты XDTO
tree-collection-form = Формы
tree-collection-template = Макеты
tree-collection-command = Команды
tree-collection-attribute = Реквизиты
tree-collection-tabular-section = Табличные части
tree-collection-dimension = Измерения
tree-collection-resource = Ресурсы
tree-collection-requisite = Реквизиты
tree-collection-enum-value = Значения
tree-collection-accounting-flag = Признаки учёта
tree-collection-ext-dimension-accounting-flag = Признаки учёта субконто
tree-collection-recalculation = Перерасчёты
tree-collection-url-template = Шаблоны URL
tree-collection-method = Методы
tree-collection-operation = Операции
tree-collection-parameter = Параметры
tree-collection-integration-service-channel = Каналы
tree-collection-column = Графы
tree-collection-addressing-attribute = Реквизиты адресации
tree-collection-common = Общие
tree-collection-modules = Модули
tree-collection-unsupported = Неподдержанные элементы
tree-module-module = Модуль
tree-module-object = Модуль объекта
tree-module-manager = Модуль менеджера
tree-module-record-set = Модуль набора записей
tree-module-value-manager = Модуль менеджера значения
tree-module-managed-application = Модуль управляемого приложения
tree-module-ordinary-application = Модуль обычного приложения
tree-module-session = Модуль сеанса
tree-module-external-connection = Модуль внешнего соединения
tree-module-command = Модуль команды
tree-collection-web-socket-client = WebSocket-клиенты
tree-collection-predefined-item = Предопределённые данные

## Properties and structured fields

# Reviewed Designer XML structural field
platform-property-app-functionality = Функциональность

# Reviewed Designer XML structural field
platform-property-app-item = Элемент

# Reviewed Designer XML structural field
platform-property-app-use = Использование

# Reviewed Designer XML structural field
platform-property-app-value = Значение

# StringQualifiers|fixed
platform-property-core-AllowedLength = Допустимая длина

# Reviewed Designer XML structural field
platform-property-core-AllowedSign = Допустимый знак

# TypeDescription|binaryQualifiers
platform-property-core-BinaryDataQualifiers = Квалификаторы двоичных данных

# DateQualifiers|dateFractions
platform-property-core-DateFractions = Состав даты

# TypeDescription|dateQualifiers
platform-property-core-DateQualifiers = Квалификаторы даты

# NumberQualifiers|precision
platform-property-core-Digits = Длина

# NumberQualifiers|scale
platform-property-core-FractionDigits = Точность

# StringQualifiers|length
platform-property-core-Length = Длина

# TypeDescription|numberQualifiers
platform-property-core-NumberQualifiers = Квалификаторы числа

# TypeDescription|stringQualifiers
platform-property-core-StringQualifiers = Квалификаторы строки

# BasicFeature|type; ChartOfCharacteristicTypes|type; Constant|type; DefinedType|type; Dimension|type; Field|type; FilterCriterion|type; Function|type; Resource|type; SequenceDimension|type; SessionParameter|type
platform-property-core-Type = Тип

# Reviewed Designer XML structural field
platform-property-core-TypeId = Идентификатор типа

# Reviewed Designer XML structural field
platform-property-core-TypeSet = Набор типов

# Reviewed Designer XML structural field
platform-property-core-Value = Значение

# Reviewed Designer XML structural field
platform-property-core-content = Текст

# Reviewed Designer XML structural field
platform-property-core-item = Элемент

# Reviewed Designer XML structural field
platform-property-core-lang = Язык

# AccountingRegisterDimension|accountingFlag; AccountingRegisterResource|accountingFlag
platform-property-md-AccountingFlag = Признак учета

# CalculationRegister|actionPeriod
platform-property-md-ActionPeriod = Период действия

# ChartOfCalculationTypes|actionPeriodUse
platform-property-md-ActionPeriodUse = Использует период действия

# Configuration|additionalFullTextSearchDictionaries
platform-property-md-AdditionalFullTextSearchDictionaries = Дополнительные словари полнотекстового поиска

# Task|addressing
platform-property-md-Addressing = Адресация

# AddressingAttribute|addressingDimension
platform-property-md-AddressingDimension = Измерение адресации

# AccumulationRegister|aggregates
platform-property-md-Aggregates = Агрегаты

# Field|allowNull
platform-property-md-AllowNull = Разрешить Null

# Configuration|allowedIncomingShareRequestTypes
platform-property-md-AllowedIncomingShareRequestTypes = Допустимые типы входящих запросов "Поделиться"

# CommonAttribute|authenticationSeparation
platform-property-md-AuthenticationSeparation = Разделение аутентификации

# WebSocketClient|autoConnect
platform-property-md-AutoConnect = Подключать автоматически

# ChartOfAccounts|autoOrderByCode
platform-property-md-AutoOrderByCode = Автопорядок по коду

# CommonAttribute|autoUse
platform-property-md-AutoUse = Автоиспользование

# BusinessProcess|autonumbering; Catalog|autonumbering; ChartOfCharacteristicTypes|autonumbering; Document|autonumbering; Task|autonumbering
platform-property-md-Autonumbering = Автонумерация

# BusinessProcess|auxiliaryChoiceForm; Catalog|auxiliaryChoiceForm; ChartOfAccounts|auxiliaryChoiceForm; ChartOfCalculationTypes|auxiliaryChoiceForm; ChartOfCharacteristicTypes|auxiliaryChoiceForm; Document|auxiliaryChoiceForm; Enum|auxiliaryChoiceForm; ExchangePlan|auxiliaryChoiceForm; Task|auxiliaryChoiceForm
platform-property-md-AuxiliaryChoiceForm = Дополнительная форма выбора

# Catalog|auxiliaryFolderChoiceForm
platform-property-md-AuxiliaryFolderChoiceForm = Дополнительная форма выбора группы

# Catalog|auxiliaryFolderForm
platform-property-md-AuxiliaryFolderForm = Дополнительная форма группы

# DataProcessor|auxiliaryForm; DocumentJournal|auxiliaryForm; ExternalDataProcessor|auxiliaryForm; ExternalReport|auxiliaryForm; FilterCriterion|auxiliaryForm; Report|auxiliaryForm
platform-property-md-AuxiliaryForm = Дополнительная форма

# AccountingRegister|auxiliaryListForm; AccumulationRegister|auxiliaryListForm; BusinessProcess|auxiliaryListForm; CalculationRegister|auxiliaryListForm; Catalog|auxiliaryListForm; ChartOfAccounts|auxiliaryListForm; ChartOfCalculationTypes|auxiliaryListForm; ChartOfCharacteristicTypes|auxiliaryListForm; Document|auxiliaryListForm; Enum|auxiliaryListForm; ExchangePlan|auxiliaryListForm; InformationRegister|auxiliaryListForm; Task|auxiliaryListForm
platform-property-md-AuxiliaryListForm = Дополнительная форма списка

# SettingsStorage|auxiliaryLoadForm
platform-property-md-AuxiliaryLoadForm = Дополнительная форма загрузки

# BusinessProcess|auxiliaryObjectForm; Catalog|auxiliaryObjectForm; ChartOfAccounts|auxiliaryObjectForm; ChartOfCalculationTypes|auxiliaryObjectForm; ChartOfCharacteristicTypes|auxiliaryObjectForm; Document|auxiliaryObjectForm; ExchangePlan|auxiliaryObjectForm; Task|auxiliaryObjectForm
platform-property-md-AuxiliaryObjectForm = Дополнительная форма объекта

# InformationRegister|auxiliaryRecordForm
platform-property-md-AuxiliaryRecordForm = Дополнительная форма редактирования записи

# SettingsStorage|auxiliarySaveForm
platform-property-md-AuxiliarySaveForm = Дополнительная форма сохранения

# ExternalReport|auxiliarySettingsForm; Report|auxiliarySettingsForm
platform-property-md-AuxiliarySettingsForm = Дополнительная форма настроек

# CommonPicture|availabilityForAppearance
platform-property-md-AvailabilityForAppearance = Доступность для оформления

# CommonPicture|availabilityForChoice
platform-property-md-AvailabilityForChoice = Доступность для выбора

# AccountingRegisterDimension|balance; AccountingRegisterResource|balance
platform-property-md-Balance = Балансовый

# ChartOfCalculationTypes|baseCalculationTypes
platform-property-md-BaseCalculationTypes = Базовые планы видов расчета

# CalculationRegisterDimension|baseDimension
platform-property-md-BaseDimension = Базовое

# CalculationRegister|basePeriod
platform-property-md-BasePeriod = Базовый период

# BasicDbObject|basedOn
platform-property-md-BasedOn = Вводится на основании

# Configuration|binaryDataBlockStorageUseMode
platform-property-md-BinaryDataBlockStorageUseMode = Режим использования блочного хранения двоичных данных

# DbObjectAttribute|binaryDataStorageLocationUse; InformationRegisterResource|binaryDataStorageLocationUse; RegisterAttribute|binaryDataStorageLocationUse
platform-property-md-BinaryDataStorageLocationUse = Использование хранения в хранилище двоичных данных

# DbObjectAttribute|binaryDataStorageLocationUseField; InformationRegisterResource|binaryDataStorageLocationUseField; RegisterAttribute|binaryDataStorageLocationUseField
platform-property-md-BinaryDataStorageLocationUseField = Поле использования хранения в хранилище двоичных данных

# Configuration|binaryDataStorageMode
platform-property-md-BinaryDataStorageMode = Режим хранилища двоичных данных

# Configuration|briefInformation
platform-property-md-BriefInformation = Краткая информация

# CommandGroup|category
platform-property-md-Category = Категория

# ChartOfCharacteristicTypes|characteristicExtValues
platform-property-md-CharacteristicExtValues = Дополнительные значения характеристик

# BasicDbObject|characteristics; Cube|characteristics; Enum|characteristics; Table|characteristics
platform-property-md-Characteristics = Характеристики

# AccountingRegister|chartOfAccounts
platform-property-md-ChartOfAccounts = План счетов

# CalculationRegister|chartOfCalculationTypes
platform-property-md-ChartOfCalculationTypes = План видов расчета

# BusinessProcess|checkUnique; Catalog|checkUnique; ChartOfAccounts|checkUnique; ChartOfCharacteristicTypes|checkUnique; DocumentNumerator|checkUnique; Document|checkUnique; Task|checkUnique
platform-property-md-CheckUnique = Контроль уникальности

# BasicDbObject|choiceDataGetModeOnInputByString
platform-property-md-ChoiceDataGetModeOnInputByString = Режим получения данных выбора при вводе по строке

# BasicFeature|choiceFoldersAndItems; Constant|choiceFoldersAndItems; Dimension|choiceFoldersAndItems
platform-property-md-ChoiceFoldersAndItems = Выбор групп и элементов

# BasicFeature|choiceForm; Constant|choiceForm; Dimension|choiceForm; Field|choiceForm; Resource|choiceForm
platform-property-md-ChoiceForm = Форма выбора

# AccountingFlag|choiceHistoryOnInput; AccountingRegisterAttribute|choiceHistoryOnInput; AccountingRegisterDimension|choiceHistoryOnInput; AccountingRegisterResource|choiceHistoryOnInput; AccumulationRegisterAttribute|choiceHistoryOnInput; AccumulationRegisterDimension|choiceHistoryOnInput; AccumulationRegisterResource|choiceHistoryOnInput; AddressingAttribute|choiceHistoryOnInput; BusinessProcessAttribute|choiceHistoryOnInput; BusinessProcess|choiceHistoryOnInput; CalculationRegisterAttribute|choiceHistoryOnInput; CalculationRegisterDimension|choiceHistoryOnInput; CalculationRegisterResource|choiceHistoryOnInput; CatalogAttribute|choiceHistoryOnInput; Catalog|choiceHistoryOnInput; ChartOfAccountsAttribute|choiceHistoryOnInput; ChartOfAccounts|choiceHistoryOnInput; ChartOfCalculationTypesAttribute|choiceHistoryOnInput; ChartOfCalculationTypes|choiceHistoryOnInput; ChartOfCharacteristicTypesAttribute|choiceHistoryOnInput; ChartOfCharacteristicTypes|choiceHistoryOnInput; CommonAttribute|choiceHistoryOnInput; Constant|choiceHistoryOnInput; DataProcessorAttribute|choiceHistoryOnInput; DataProcessorTabularSectionAttribute|choiceHistoryOnInput; Dimension|choiceHistoryOnInput; DocumentAttribute|choiceHistoryOnInput; Document|choiceHistoryOnInput; Enum|choiceHistoryOnInput; ExchangePlanAttribute|choiceHistoryOnInput; ExchangePlan|choiceHistoryOnInput; ExtDimensionAccountingFlag|choiceHistoryOnInput; InformationRegisterAttribute|choiceHistoryOnInput; InformationRegisterDimension|choiceHistoryOnInput; InformationRegisterResource|choiceHistoryOnInput; ReportAttribute|choiceHistoryOnInput; ReportTabularSectionAttribute|choiceHistoryOnInput; Table|choiceHistoryOnInput; TabularSectionAttribute|choiceHistoryOnInput; TaskAttribute|choiceHistoryOnInput; Task|choiceHistoryOnInput
platform-property-md-ChoiceHistoryOnInput = История выбора при вводе

# Catalog|choiceMode; ChartOfCalculationTypes|choiceMode; ChartOfCharacteristicTypes|choiceMode; Enum|choiceMode; ExchangePlan|choiceMode
platform-property-md-ChoiceMode = Способ выбора

# BasicFeature|choiceParameterLinks; Constant|choiceParameterLinks; Dimension|choiceParameterLinks; Field|choiceParameterLinks; Resource|choiceParameterLinks
platform-property-md-ChoiceParameterLinks = Связи параметров выбора

# BasicFeature|choiceParameters; Constant|choiceParameters; Dimension|choiceParameters; Field|choiceParameters; Resource|choiceParameters
platform-property-md-ChoiceParameters = Параметры выбора

# CommonModule|clientManagedApplication
platform-property-md-ClientManagedApplication = Клиент (управляемое приложение)

# CommonModule|clientOrdinaryApplication
platform-property-md-ClientOrdinaryApplication = Клиент (обычное приложение)

# Catalog|codeAllowedLength; ChartOfCalculationTypes|codeAllowedLength; ChartOfCharacteristicTypes|codeAllowedLength; ExchangePlan|codeAllowedLength
platform-property-md-CodeAllowedLength = Допустимая длина кода

# Catalog|codeLength; ChartOfAccounts|codeLength; ChartOfCalculationTypes|codeLength; ChartOfCharacteristicTypes|codeLength; ExchangePlan|codeLength
platform-property-md-CodeLength = Длина кода

# ChartOfAccounts|codeMask
platform-property-md-CodeMask = Маска кода

# Catalog|codeSeries; ChartOfAccounts|codeSeries; ChartOfCharacteristicTypes|codeSeries
platform-property-md-CodeSeries = Серии кодов

# Catalog|codeType; ChartOfCalculationTypes|codeType
platform-property-md-CodeType = Тип кода

# EnumValue|color; PaletteColor|color
platform-property-md-Color = Цвет

# BasicCommand|commandParameterType
platform-property-md-CommandParameterType = Тип параметра команды

# MdObject|comment
platform-property-md-Comment = Комментарий

# Configuration|commonSettingsStorage
platform-property-md-CommonSettingsStorage = Хранилище общих настроек

# Configuration|compatibilityMode
platform-property-md-CompatibilityMode = Режим совместимости

# CommonAttribute|conditionalSeparation
platform-property-md-ConditionalSeparation = Условное разделение

# Configuration|configurationExtensionCompatibilityMode
platform-property-md-ConfigurationExtensionCompatibilityMode = Режим совместимости расширения конфигурации

# Configuration|configurationExtensionPurpose
platform-property-md-ConfigurationExtensionPurpose = Назначение расширения конфигурации

# CommonAttribute|configurationExtensionsSeparation
platform-property-md-ConfigurationExtensionsSeparation = Разделение расширений конфигураций

# Configuration|configurationInformationAddress
platform-property-md-ConfigurationInformationAddress = Адрес информации о конфигурации

# CommonAttribute|content; FunctionalOption|content; Subsystem|content
platform-property-md-Content = Состав

# Configuration|copyright
platform-property-md-Copyright = Авторские права

# AccountingRegister|correspondence
platform-property-md-Correspondence = Корреспонденция

# BasicDbObject|createOnInput; BasicFeature|createOnInput; Dimension|createOnInput; Field|createOnInput; Table|createOnInput
platform-property-md-CreateOnInput = Создание при вводе

# BusinessProcess|createTaskInPrivilegedMode
platform-property-md-CreateTaskInPrivilegedMode = Привилегированный режим при создании задачи

# Task|currentPerformer
platform-property-md-CurrentPerformer = Текущий исполнитель

# DataHistorySupport|dataHistory
platform-property-md-DataHistory = История данных

# AccountingRegister|dataLockControlMode; AccumulationRegister|dataLockControlMode; BasicDbObject|dataLockControlMode; CalculationRegister|dataLockControlMode; Configuration|dataLockControlMode; Constant|dataLockControlMode; ExternalDataSource|dataLockControlMode; InformationRegister|dataLockControlMode; Operation|dataLockControlMode; Recalculation|dataLockControlMode; Sequence|dataLockControlMode; Table|dataLockControlMode
platform-property-md-DataLockControlMode = Режим управления блокировкой данных

# BasicDbObject|dataLockFields; Table|dataLockFields
platform-property-md-DataLockFields = Поля блокировки данных

# CommonAttribute|dataSeparation
platform-property-md-DataSeparation = Разделение данных

# CommonAttribute|dataSeparationUse
platform-property-md-DataSeparationUse = Использование разделения данных

# CommonAttribute|dataSeparationValue
platform-property-md-DataSeparationValue = Значение разделения данных

# Table|dataVersionField
platform-property-md-DataVersionField = Поле версии данных

# Configuration|databaseTablespacesUseMode
platform-property-md-DatabaseTablespacesUseMode = Режим использования табличных пространств

# BusinessProcess|defaultChoiceForm; Catalog|defaultChoiceForm; ChartOfAccounts|defaultChoiceForm; ChartOfCalculationTypes|defaultChoiceForm; ChartOfCharacteristicTypes|defaultChoiceForm; DimensionTable|defaultChoiceForm; Document|defaultChoiceForm; Enum|defaultChoiceForm; ExchangePlan|defaultChoiceForm; Table|defaultChoiceForm; Task|defaultChoiceForm
platform-property-md-DefaultChoiceForm = Основная форма выбора

# Configuration|defaultCollaborationSystemUsersChoiceForm
platform-property-md-DefaultCollaborationSystemUsersChoiceForm = Основная форма выбора пользователей системы взаимодействия

# Configuration|defaultConstantsForm
platform-property-md-DefaultConstantsForm = Основная форма констант

# Configuration|defaultDataHistoryChangeHistoryForm
platform-property-md-DefaultDataHistoryChangeHistoryForm = Основная форма истории изменений истории данных

# Configuration|defaultDataHistoryVersionDataForm
platform-property-md-DefaultDataHistoryVersionDataForm = Основная форма данных версии истории данных

# Configuration|defaultDataHistoryVersionDifferencesForm
platform-property-md-DefaultDataHistoryVersionDifferencesForm = Основная форма различий версий истории данных

# Configuration|defaultDynamicListSettingsForm
platform-property-md-DefaultDynamicListSettingsForm = Основная форма настроек динамического списка

# Catalog|defaultFolderChoiceForm
platform-property-md-DefaultFolderChoiceForm = Основная форма выбора группы

# Catalog|defaultFolderForm; ChartOfCharacteristicTypes|defaultFolderForm
platform-property-md-DefaultFolderForm = Основная форма группы

# Constant|defaultForm; DataProcessor|defaultForm; DocumentJournal|defaultForm; ExternalDataProcessor|defaultForm; ExternalReport|defaultForm; FilterCriterion|defaultForm; Report|defaultForm
platform-property-md-DefaultForm = Основная форма

# Configuration|defaultInterface
platform-property-md-DefaultInterface = Основной интерфейс

# Configuration|defaultLanguage
platform-property-md-DefaultLanguage = Основной язык

# AccountingRegister|defaultListForm; AccumulationRegister|defaultListForm; BusinessProcess|defaultListForm; CalculationRegister|defaultListForm; Catalog|defaultListForm; ChartOfAccounts|defaultListForm; ChartOfCalculationTypes|defaultListForm; ChartOfCharacteristicTypes|defaultListForm; Cube|defaultListForm; DimensionTable|defaultListForm; Document|defaultListForm; Enum|defaultListForm; ExchangePlan|defaultListForm; InformationRegister|defaultListForm; Table|defaultListForm; Task|defaultListForm
platform-property-md-DefaultListForm = Основная форма списка

# SettingsStorage|defaultLoadForm
platform-property-md-DefaultLoadForm = Основная форма загрузки

# BusinessProcess|defaultObjectForm; Catalog|defaultObjectForm; ChartOfAccounts|defaultObjectForm; ChartOfCalculationTypes|defaultObjectForm; ChartOfCharacteristicTypes|defaultObjectForm; DimensionTable|defaultObjectForm; Document|defaultObjectForm; ExchangePlan|defaultObjectForm; Table|defaultObjectForm; Task|defaultObjectForm
platform-property-md-DefaultObjectForm = Основная форма объекта

# Catalog|defaultPresentation; ChartOfAccounts|defaultPresentation; ChartOfCalculationTypes|defaultPresentation; ChartOfCharacteristicTypes|defaultPresentation
platform-property-md-DefaultPresentation = Основное представление

# Cube|defaultRecordForm; Table|defaultRecordForm
platform-property-md-DefaultRecordForm = Основная форма записи

# Configuration|defaultReportAppearanceTemplate
platform-property-md-DefaultReportAppearanceTemplate = Основной макет оформления отчета

# Configuration|defaultReportForm
platform-property-md-DefaultReportForm = Основная форма отчета

# Configuration|defaultReportSettingsForm
platform-property-md-DefaultReportSettingsForm = Основная форма настроек отчета

# Configuration|defaultReportVariantForm
platform-property-md-DefaultReportVariantForm = Основная форма варианта отчета

# Configuration|defaultRole
platform-property-md-DefaultRole = Основная роль

# Configuration|defaultRoles
platform-property-md-DefaultRoles = Основные роли

# Configuration|defaultRunMode
platform-property-md-DefaultRunMode = Основной режим запуска

# SettingsStorage|defaultSaveForm
platform-property-md-DefaultSaveForm = Основная форма сохранения

# Configuration|defaultSearchForm
platform-property-md-DefaultSearchForm = Основная форма поиска

# ExternalReport|defaultSettingsForm; Report|defaultSettingsForm
platform-property-md-DefaultSettingsForm = Основная форма настроек

# Configuration|defaultStyle
platform-property-md-DefaultStyle = Основной стиль

# ExternalReport|defaultVariantForm; Report|defaultVariantForm
platform-property-md-DefaultVariantForm = Основная форма варианта

# RegisterDimension|denyIncompleteValues
platform-property-md-DenyIncompleteValues = Запрет незаполненных значений

# ChartOfCalculationTypes|dependenceOnCalculationTypes
platform-property-md-DependenceOnCalculationTypes = Зависимость от базы

# ScheduledJob|description
platform-property-md-Description = Наименование

# Catalog|descriptionLength; ChartOfAccounts|descriptionLength; ChartOfCalculationTypes|descriptionLength; ChartOfCharacteristicTypes|descriptionLength; ExchangePlan|descriptionLength; Task|descriptionLength
platform-property-md-DescriptionLength = Длина наименования

# WebService|descriptorFileName
platform-property-md-DescriptorFileName = Имя файла публикации

# Configuration|detailedInformation
platform-property-md-DetailedInformation = Подробная информация

# ExchangePlan|distributedInfoBase
platform-property-md-DistributedInfoBase = Распределенная информационная база

# SequenceDimension|documentMap
platform-property-md-DocumentMap = Соответствие реквизитам документов

# Sequence|documents
platform-property-md-Documents = Документы, входящие в последовательность

# Configuration|dynamicListsUserSettingsStorage
platform-property-md-DynamicListsUserSettingsStorage = Хранилище пользовательских настроек динамических списков

# BasicFeature|editFormat; Constant|editFormat; Dimension|editFormat; Field|editFormat; Resource|editFormat
platform-property-md-EditFormat = Формат редактирования

# BusinessProcess|editType; Catalog|editType; ChartOfCharacteristicTypes|editType; ExchangePlan|editType; InformationRegister|editType; Table|editType; Task|editType
platform-property-md-EditType = Способ редактирования

# InformationRegister|enableTotalsSliceFirst
platform-property-md-EnableTotalsSliceFirst = Разрешить итоги: срез первых

# InformationRegister|enableTotalsSliceLast
platform-property-md-EnableTotalsSliceLast = Разрешить итоги: срез последних

# AccountingRegister|enableTotalsSplitting; AccumulationRegister|enableTotalsSplitting
platform-property-md-EnableTotalsSplitting = Разрешить разделение итогов

# EventSubscription|event
platform-property-md-Event = Событие

# BusinessProcess|executeAfterWriteDataHistoryVersionProcessing; Catalog|executeAfterWriteDataHistoryVersionProcessing; ChartOfAccounts|executeAfterWriteDataHistoryVersionProcessing; ChartOfCalculationTypes|executeAfterWriteDataHistoryVersionProcessing; ChartOfCharacteristicTypes|executeAfterWriteDataHistoryVersionProcessing; Constant|executeAfterWriteDataHistoryVersionProcessing; Document|executeAfterWriteDataHistoryVersionProcessing; ExchangePlan|executeAfterWriteDataHistoryVersionProcessing; InformationRegister|executeAfterWriteDataHistoryVersionProcessing; Task|executeAfterWriteDataHistoryVersionProcessing
platform-property-md-ExecuteAfterWriteDataHistoryVersionProcessing = Выполнять обработку после записи версии истории данных

# AccountingRegister|explanation; AccumulationRegister|explanation; BasicDbObject|explanation; CalculationRegister|explanation; CommonForm|explanation; Constant|explanation; Cube|explanation; DataProcessor|explanation; DimensionTable|explanation; DocumentJournal|explanation; Enum|explanation; FilterCriterion|explanation; InformationRegister|explanation; Report|explanation; Subsystem|explanation; Table|explanation
platform-property-md-Explanation = Пояснение

# Function|expressionInDataSource; Table|expressionInDataSource
platform-property-md-ExpressionInDataSource = Выражение в источнике данных

# AccountingRegisterResource|extDimensionAccountingFlag
platform-property-md-ExtDimensionAccountingFlag = Признак учета субконто

# ChartOfAccounts|extDimensionTypes
platform-property-md-ExtDimensionTypes = Виды субконто

# MdObject|extendedConfigurationObject
platform-property-md-ExtendedConfigurationObject = Объект расширяемой конфигурации

# BasicFeature|extendedEdit; Constant|extendedEdit; Dimension|extendedEdit; Field|extendedEdit; Resource|extendedEdit
platform-property-md-ExtendedEdit = Расширенное редактирование

# AccountingRegister|extendedListPresentation; AccumulationRegister|extendedListPresentation; BasicDbObject|extendedListPresentation; CalculationRegister|extendedListPresentation; Cube|extendedListPresentation; DimensionTable|extendedListPresentation; DocumentJournal|extendedListPresentation; Enum|extendedListPresentation; FilterCriterion|extendedListPresentation; InformationRegister|extendedListPresentation; Table|extendedListPresentation
platform-property-md-ExtendedListPresentation = Расширенное представление списка

# BasicDbObject|extendedObjectPresentation
platform-property-md-ExtendedObjectPresentation = Расширенное представление объекта

# CommonForm|extendedPresentation; Constant|extendedPresentation; DataProcessorForm|extendedPresentation; DataProcessor|extendedPresentation; ReportForm|extendedPresentation; Report|extendedPresentation
platform-property-md-ExtendedPresentation = Расширенное представление

# Cube|extendedRecordPresentation; InformationRegister|extendedRecordPresentation; Table|extendedRecordPresentation
platform-property-md-ExtendedRecordPresentation = Расширенное представление записи

# CommonModule|externalConnection
platform-property-md-ExternalConnection = Внешнее соединение

# IntegrationService|externalIntegrationServiceAddress
platform-property-md-ExternalIntegrationServiceAddress = Адрес внешнего сервиса интеграции

# IntegrationServiceChannel|externalIntegrationServiceChannelName
platform-property-md-ExternalIntegrationServiceChannelName = Имя канала внешнего сервиса интеграции

# BasicFeature|fillChecking; BasicTabularSection|fillChecking; Constant|fillChecking; Dimension|fillChecking; Field|fillChecking
platform-property-md-FillChecking = Проверка заполнения

# AccountingFlag|fillFromFillingValue; AddressingAttribute|fillFromFillingValue; CommonAttribute|fillFromFillingValue; DataProcessorTabularSectionAttribute|fillFromFillingValue; DbObjectAttribute|fillFromFillingValue; ExtDimensionAccountingFlag|fillFromFillingValue; Field|fillFromFillingValue; InformationRegisterAttribute|fillFromFillingValue; InformationRegisterDimension|fillFromFillingValue; InformationRegisterResource|fillFromFillingValue; ReportTabularSectionAttribute|fillFromFillingValue
platform-property-md-FillFromFillingValue = Заполнять из данных заполнения

# AccountingFlag|fillValue; AddressingAttribute|fillValue; CommonAttribute|fillValue; DataProcessorTabularSectionAttribute|fillValue; DbObjectAttribute|fillValue; ExtDimensionAccountingFlag|fillValue; Field|fillValue; InformationRegisterAttribute|fillValue; InformationRegisterDimension|fillValue; InformationRegisterResource|fillValue; ReportTabularSectionAttribute|fillValue
platform-property-md-FillValue = Значение заполнения

# Catalog|foldersOnTop; ChartOfCharacteristicTypes|foldersOnTop
platform-property-md-FoldersOnTop = Размещать группы сверху

# Configuration|formDataSettingsStorage
platform-property-md-FormDataSettingsStorage = Хранилище настроек данных форм

# BasicForm|formType
platform-property-md-FormType = Тип формы

# BasicFeature|format; Constant|format; Dimension|format; Field|format; Resource|format
platform-property-md-Format = Формат

# AccountingRegister|fullTextSearch; AccumulationRegister|fullTextSearch; AddressingAttribute|fullTextSearch; BasicDbObject|fullTextSearch; CalculationRegister|fullTextSearch; DbObjectAttribute|fullTextSearch; InformationRegister|fullTextSearch; RegisterAttribute|fullTextSearch; RegisterDimension|fullTextSearch; RegisterResource|fullTextSearch; TabularSectionAttribute|fullTextSearch
platform-property-md-FullTextSearch = Полнотекстовый поиск

# BasicDbObject|fullTextSearchOnInputByString
platform-property-md-FullTextSearchOnInputByString = Полнотекстовый поиск при вводе по строке

# CommonModule|global
platform-property-md-Global = Глобальный

# BasicCommand|group
platform-property-md-Group = Группа

# Method|httpMethod
platform-property-md-HTTPMethod = HTTP-метод

# EventSubscription|handler; Method|handler
platform-property-md-Handler = Обработчик

# WebSocketClient|headers
platform-property-md-Headers = Заголовки

# AccountingRegister|help; AccumulationRegister|help; BasicDbObject|help; BasicForm|help; CalculationRegister|help; CommonCommand|help; Configuration|help; Cube|help; DataProcessor|help; DimensionTable|help; DocumentJournal|help; ExternalDataProcessor|help; ExternalReport|help; InformationRegister|help; Report|help; Subsystem|help; Table|help
platform-property-md-Help = Справочная информация

# Catalog|hierarchical; ChartOfCharacteristicTypes|hierarchical
platform-property-md-Hierarchical = Иерархический

# DimensionTable|hierarchyNameInDataSource
platform-property-md-HierarchyNameInDataSource = Имя иерархии в источнике данных

# Catalog|hierarchyType
platform-property-md-HierarchyType = Вид иерархии

# ExchangePlan|includeConfigurationExtensions
platform-property-md-IncludeConfigurationExtensions = Включать расширения конфигурации

# AccountingRegister|includeHelpInContents; AccumulationRegister|includeHelpInContents; BasicDbObject|includeHelpInContents; BasicForm|includeHelpInContents; CalculationRegister|includeHelpInContents; CommonCommand|includeHelpInContents; Configuration|includeHelpInContents; DataProcessor|includeHelpInContents; DocumentJournal|includeHelpInContents; InformationRegister|includeHelpInContents; Report|includeHelpInContents; Subsystem|includeHelpInContents
platform-property-md-IncludeHelpInContents = Включать в содержание справки

# Subsystem|includeInCommandInterface
platform-property-md-IncludeInCommandInterface = Включать в командный интерфейс

# AddressingAttribute|indexing; Column|indexing; DbObjectAttribute|indexing; InformationRegisterResource|indexing; RegisterAttribute|indexing; RegisterDimension|indexing; TabularSectionAttribute|indexing
platform-property-md-Indexing = Индексировать

# InformationRegister|informationRegisterPeriodicity
platform-property-md-InformationRegisterPeriodicity = Периодичность

# BasicDbObject|inputByString
platform-property-md-InputByString = Ввод по строке

# Configuration|interfaceCompatibilityMode
platform-property-md-InterfaceCompatibilityMode = Режим совместимости интерфейса

# Configuration|keepMappingToExtendedConfigurationObjectsByIDs
platform-property-md-KeepMappingToExtendedConfigurationObjectsByIDs = Поддерживать соответствие объектам расширяемой конфигурации по внутренним идентификаторам

# ScheduledJob|key
platform-property-md-Key = Ключ

# Table|keyFields
platform-property-md-KeyFields = Поля ключа

# Language|languageCode
platform-property-md-LanguageCode = Код языка

# RecalculationDimension|leadingRegisterData
platform-property-md-LeadingRegisterData = Данные ведущих регистров

# Catalog|levelCount
platform-property-md-LevelCount = Количество уровней

# DimensionTable|levelNumber
platform-property-md-LevelNumber = Номер уровня

# Catalog|limitLevelCount
platform-property-md-LimitLevelCount = Ограничивать кол-во уровней

# DbObjectTabularSection|lineNumberLength
platform-property-md-LineNumberLength = Длина номера строки

# BasicFeature|linkByType; Constant|linkByType; Dimension|linkByType
platform-property-md-LinkByType = Связь по типу

# AccountingRegister|listPresentation; AccumulationRegister|listPresentation; BasicDbObject|listPresentation; CalculationRegister|listPresentation; Cube|listPresentation; DimensionTable|listPresentation; DocumentJournal|listPresentation; Enum|listPresentation; FilterCriterion|listPresentation; InformationRegister|listPresentation; Table|listPresentation
platform-property-md-ListPresentation = Представление списка

# FunctionalOption|location
platform-property-md-Location = Хранение

# WSReference|locationURL
platform-property-md-LocationURL = URL источника

# Configuration|logo
platform-property-md-Logo = Логотип

# Task|mainAddressingAttribute
platform-property-md-MainAddressingAttribute = Основной реквизит адресации

# Configuration|mainClientApplicationWindowMode
platform-property-md-MainClientApplicationWindowMode = Режим основного окна клиентского приложения

# ExternalReport|mainDataCompositionSchema; Report|mainDataCompositionSchema
platform-property-md-MainDataCompositionSchema = Основная схема компоновки данных

# InformationRegisterDimension|mainFilter
platform-property-md-MainFilter = Основной отбор

# InformationRegister|mainFilterOnPeriod
platform-property-md-MainFilterOnPeriod = Основной отбор по периоду

# Configuration|mainSectionPicture
platform-property-md-MainSectionPicture = Картинка основного раздела

# BasicFeature|markNegatives; Constant|markNegatives; Dimension|markNegatives; Resource|markNegatives
platform-property-md-MarkNegatives = Выделять отрицательные

# BasicFeature|mask; Constant|mask; Dimension|mask; Field|mask; Resource|mask
platform-property-md-Mask = Маска

# InformationRegisterDimension|master
platform-property-md-Master = Ведущее

# ChartOfAccounts|maxExtDimensionCount
platform-property-md-MaxExtDimensionCount = Максимальное количество субконто

# BasicFeature|maxValue; Constant|maxValue; Dimension|maxValue; Field|maxValue
platform-property-md-MaxValue = Максимальное значение

# IntegrationServiceChannel|messageDirection
platform-property-md-MessageDirection = Направление сообщения

# ScheduledJob|methodName
platform-property-md-MethodName = Имя метода

# BasicFeature|minValue; Constant|minValue; Dimension|minValue; Field|minValue
platform-property-md-MinValue = Минимальное значение

# Configuration|mobileApplicationUrls
platform-property-md-MobileApplicationURLs = Навигационные ссылки мобильного приложения

# Configuration|modalityUseMode
platform-property-md-ModalityUseMode = Режим использования модальности

# BasicCommand|modifiesData
platform-property-md-ModifiesData = Изменяет данные

# Sequence|moveBoundaryOnPosting
platform-property-md-MoveBoundaryOnPosting = Перемещение границы при проведении

# BasicFeature|multiLine; Constant|multiLine
platform-property-md-MultiLine = Многострочный режим

# MdObject|name
platform-property-md-Name = Имя

# DimensionTable|nameInDataSource; Field|nameInDataSource; Resource|nameInDataSource
platform-property-md-NameInDataSource = Имя в источнике данных

# Configuration|namePrefix
platform-property-md-NamePrefix = Префикс

# WebService|namespace; XDTOPackage|namespace
platform-property-md-Namespace = URI пространства имен

# Operation|nillable; Parameter|nillable
platform-property-md-Nillable = Возможно пустое значение

# BusinessProcess|numberAllowedLength; DocumentNumerator|numberAllowedLength; Document|numberAllowedLength; Task|numberAllowedLength
platform-property-md-NumberAllowedLength = Допустимая длина номера

# BusinessProcess|numberLength; DocumentNumerator|numberLength; Document|numberLength; Task|numberLength
platform-property-md-NumberLength = Длина номера

# BusinessProcess|numberPeriodicity; DocumentNumerator|numberPeriodicity; Document|numberPeriodicity
platform-property-md-NumberPeriodicity = Периодичность

# BusinessProcess|numberType; DocumentNumerator|numberType; Document|numberType; Task|numberType
platform-property-md-NumberType = Тип номера

# Document|numerator
platform-property-md-Numerator = Нумератор

# Configuration|objectAutonumerationMode
platform-property-md-ObjectAutonumerationMode = Режим автонумерации объектов

# MdObject|objectBelonging
platform-property-md-ObjectBelonging = Принадлежность объекта

# BasicDbObject|objectPresentation
platform-property-md-ObjectPresentation = Представление объекта

# BasicCommand|onMainServerUnavalableBehavior
platform-property-md-OnMainServerUnavalableBehavior = Поведение при недоступности основного сервера

# ChartOfAccounts|orderLength
platform-property-md-OrderLength = Длина порядка

# Catalog|owners
platform-property-md-Owners = Владельцы

# BasicCommand|parameterUseMode
platform-property-md-ParameterUseMode = Режим использования параметра

# Table|parentField
platform-property-md-ParentField = Поле родителя

# WebSocketClient|password
platform-property-md-Password = Пароль

# BasicFeature|passwordMode; Constant|passwordMode; Dimension|passwordMode; Field|passwordMode; Resource|passwordMode
platform-property-md-PasswordMode = Режим пароля

# AccountingRegister|periodAdjustmentLength
platform-property-md-PeriodAdjustmentLength = Длина уточнения периода

# CalculationRegister|periodicity
platform-property-md-Periodicity = Периодичность

# BasicCommand|picture; Bot|picture; CommandGroup|picture; Subsystem|picture
platform-property-md-Picture = Картинка

# Document|postInPrivilegedMode
platform-property-md-PostInPrivilegedMode = Привилегированный режим при проведении

# Document|posting
platform-property-md-Posting = Проведение

# Catalog|predefined; ChartOfCalculationTypes|predefined; ChartOfCharacteristicTypes|predefined
platform-property-md-Predefined = Предопределенные

# Catalog|predefinedDataUpdate; ChartOfCalculationTypes|predefinedDataUpdate; ChartOfCharacteristicTypes|predefinedDataUpdate
platform-property-md-PredefinedDataUpdate = Обновление предопределенных данных

# DimensionTable|presentationField; Table|presentationField
platform-property-md-PresentationField = Поле представления

# CommonModule|privileged
platform-property-md-Privileged = Привилегированный

# FunctionalOption|privilegedGetMode
platform-property-md-PrivilegedGetMode = Привилегированный режим при получении

# Operation|procedureName
platform-property-md-ProcedureName = Имя процедуры

# BasicFeature|quickChoice; Catalog|quickChoice; ChartOfAccounts|quickChoice; ChartOfCalculationTypes|quickChoice; ChartOfCharacteristicTypes|quickChoice; Constant|quickChoice; DimensionTable|quickChoice; Dimension|quickChoice; Enum|quickChoice; ExchangePlan|quickChoice; Field|quickChoice; Resource|quickChoice; Table|quickChoice
platform-property-md-QuickChoice = Быстрый выбор

# Table|readOnly
platform-property-md-ReadOnly = Только чтение

# Document|realTimePosting
platform-property-md-RealTimePosting = Оперативное проведение

# IntegrationServiceChannel|receiveMessageProcessing
platform-property-md-ReceiveMessageProcessing = Обработчик получения сообщения

# Cube|recordPresentation; InformationRegister|recordPresentation; Table|recordPresentation
platform-property-md-RecordPresentation = Представление записи

# Column|references
platform-property-md-References = Ссылки

# RecalculationDimension|registerDimension
platform-property-md-RegisterDimension = Измерения регистра

# Document|registerRecords
platform-property-md-RegisterRecords = Движения

# Document|registerRecordsDeletion
platform-property-md-RegisterRecordsDeletion = Удаление движений

# SequenceDimension|registerRecordsMap
platform-property-md-RegisterRecordsMap = Соответствие реквизитам движений

# Document|registerRecordsWritingOnPost
platform-property-md-RegisterRecordsWritingOnPost = Запись движений при проведении

# AccumulationRegister|registerType
platform-property-md-RegisterType = Вид регистра

# DocumentJournal|registeredDocuments
platform-property-md-RegisteredDocuments = Регистрируемые документы

# Configuration|reportsUserSettingsStorage
platform-property-md-ReportsUserSettingsStorage = Хранилище пользовательских настроек отчетов

# Configuration|reportsVariantsStorage
platform-property-md-ReportsVariantsStorage = Хранилище вариантов отчетов

# BasicCommand|representation; CommandGroup|representation
platform-property-md-Representation = Отображение

# Configuration|requiredMobileApplicationPermissions8315
platform-property-md-RequiredMobileApplicationPermissions = Требуемые разрешения мобильного приложения

# Configuration|requiredMobileApplicationPermissions8315
platform-property-md-RequiredMobileApplicationPermissions8315 = Требуемые разрешения мобильного приложения

# ScheduledJob|restartCountOnFailure
platform-property-md-RestartCountOnFailure = Количество повторов при аварийном завершении

# ScheduledJob|restartIntervalOnFailure
platform-property-md-RestartIntervalOnFailure = Интервал повтора при аварийном завершении

# Function|returnValue
platform-property-md-ReturnValue = Возвращает значение

# CommonModule|returnValuesReuse
platform-property-md-ReturnValuesReuse = Повторное использование возвращаемых значений

# HTTPService|reuseSessions; WebService|reuseSessions
platform-property-md-ReuseSessions = Повторное использование сеансов

# HTTPService|rootURL
platform-property-md-RootURL = Корневой URL

# CalculationRegister|schedule
platform-property-md-Schedule = График

# CalculationRegister|scheduleDate
platform-property-md-ScheduleDate = Дата графика

# CalculationRegisterAttribute|scheduleLink; CalculationRegisterDimension|scheduleLink
platform-property-md-ScheduleLink = Связь с графиком

# CalculationRegister|scheduleValue
platform-property-md-ScheduleValue = Значение графика

# Configuration|scriptVariant
platform-property-md-ScriptVariant = Вариант встроенного языка

# BasicDbObject|searchStringModeOnInputByString
platform-property-md-SearchStringModeOnInputByString = Способ поиска строки при вводе по строке

# CommonAttribute|separatedDataUse
platform-property-md-SeparatedDataUse = Использование разделяемых данных

# Document|sequenceFilling
platform-property-md-SequenceFilling = Заполнение последовательностей

# CommonModule|server
platform-property-md-Server = Сервер

# CommonModule|serverCall
platform-property-md-ServerCall = Вызов сервера

# WebSocketClient|serverURL
platform-property-md-ServerURL = URL сервера

# HTTPService|sessionMaxAge; WebService|sessionMaxAge
platform-property-md-SessionMaxAge = Время жизни сеанса

# ExternalReport|settingsStorage; Report|settingsStorage
platform-property-md-SettingsStorage = Хранилище настроек

# BasicCommand|shortcut
platform-property-md-Shortcut = Сочетание клавиш

# EventSubscription|source
platform-property-md-Source = Источник

# Configuration|splash
platform-property-md-Splash = Заставка

# Configuration|standaloneConfigurationRestrictionRoles
platform-property-md-StandaloneConfigurationRestrictionRoles = Роли ограничения автономного мобильного приложения

# AccountingRegister|standardAttributes; AccumulationRegister|standardAttributes; BasicDbObject|standardAttributes; BasicTabularSection|standardAttributes; CalculationRegister|standardAttributes; DocumentJournal|standardAttributes; Enum|standardAttributes; InformationRegister|standardAttributes
platform-property-md-StandardAttributes = Стандартные реквизиты

# ChartOfAccounts|standardTabularSections; ChartOfCalculationTypes|standardTabularSections
platform-property-md-StandardTabularSections = Стандартные табличные части

# Catalog|subordinationUse
platform-property-md-SubordinationUse = Использование подчинения

# Configuration|synchronousPlatformExtensionAndAddInCallUseMode
platform-property-md-SynchronousPlatformExtensionAndAddInCallUseMode = Режим использования синхронных вызовов расширений платформы и внешних компонент

# MdObject|synonym
platform-property-md-Synonym = Синоним

# Table|tableDataType
platform-property-md-TableDataType = Тип данных таблицы внешнего источника данных

# Table|tableType
platform-property-md-TableType = Вид таблицы

# BusinessProcess|task
platform-property-md-Task = Задачи

# Task|taskNumberAutoPrefix
platform-property-md-TaskNumberAutoPrefix = Авто префикс

# URLTemplate|template
platform-property-md-Template = Шаблон

# BasicTemplate|templateType
platform-property-md-TemplateType = Тип макета

# WebSocketClient|timeout
platform-property-md-Timeout = Таймаут (секунд)

# BasicCommand|toolTip; BasicFeature|toolTip; BasicTabularSection|toolTip; CommandGroup|toolTip; Constant|toolTip; Dimension|toolTip; Field|toolTip; Resource|toolTip
platform-property-md-ToolTip = Подсказка

# IntegrationServiceChannel|transactioned; Operation|transactioned
platform-property-md-Transactioned = В транзакции

# Table|transactionsIsolationLevel
platform-property-md-TransactionsIsolationLevel = Уровень изоляции транзакций

# Parameter|transferDirection
platform-property-md-TransferDirection = Направление передачи

# BasicFeature|type; ChartOfCharacteristicTypes|type; Constant|type; DefinedType|type; Dimension|type; Field|type; FilterCriterion|type; Function|type; Resource|type; SequenceDimension|type; SessionParameter|type
platform-property-md-Type = Тип

# InformationRegisterDimension|typeReductionMode
platform-property-md-TypeReductionMode = Режим сокращения типа

# Configuration|urlExternalDataStorage
platform-property-md-URLExternalDataStorage = Хранилище внешних данных навигационных ссылок

# DimensionTable|unfilledParentValue; Table|unfilledParentValue
platform-property-md-UnfilledParentValue = Значение незаполненного родителя

# Document|unpostInPrivilegedMode
platform-property-md-UnpostInPrivilegedMode = Привилегированный режим при отмене проведения

# Configuration|updateCatalogAddress
platform-property-md-UpdateCatalogAddress = Адрес каталога обновлений

# BusinessProcess|updateDataHistoryImmediatelyAfterWrite; Catalog|updateDataHistoryImmediatelyAfterWrite; ChartOfAccounts|updateDataHistoryImmediatelyAfterWrite; ChartOfCalculationTypes|updateDataHistoryImmediatelyAfterWrite; ChartOfCharacteristicTypes|updateDataHistoryImmediatelyAfterWrite; Constant|updateDataHistoryImmediatelyAfterWrite; Document|updateDataHistoryImmediatelyAfterWrite; ExchangePlan|updateDataHistoryImmediatelyAfterWrite; InformationRegister|updateDataHistoryImmediatelyAfterWrite; Task|updateDataHistoryImmediatelyAfterWrite
platform-property-md-UpdateDataHistoryImmediatelyAfterWrite = Обновлять историю данных сразу после записи

# FunctionalOptionsParameter|use; HierarchicalDbObjectAttribute|use; HierarchicalDbObjectTabularSection|use; ScheduledJob|use
platform-property-md-Use = Использование

# AccumulationRegisterDimension|useInTotals
platform-property-md-UseInTotals = Использование в итогах

# Configuration|useManagedFormInOrdinaryApplication
platform-property-md-UseManagedFormInOrdinaryApplication = Использовать управляемые формы в обычном приложении

# WebSocketClient|useOSAuthentication
platform-property-md-UseOSAuthentication = Использовать аутентификацию ОС

# WebSocketClient|useOSProxy
platform-property-md-UseOSProxy = Использовать прокси ОС

# Subsystem|useOneCommand
platform-property-md-UseOneCommand = Подсистема с одной командой

# Configuration|useOrdinaryFormInManagedApplication
platform-property-md-UseOrdinaryFormInManagedApplication = Использовать обычные формы в управляемом приложении

# BasicForm|usePurposes; Configuration|usePurposes
platform-property-md-UsePurposes = Назначения использования

# AccountingRegister|useStandardCommands; AccumulationRegister|useStandardCommands; BasicDbObject|useStandardCommands; CalculationRegister|useStandardCommands; CommonForm|useStandardCommands; Constant|useStandardCommands; Cube|useStandardCommands; DataProcessor|useStandardCommands; DimensionTable|useStandardCommands; DocumentJournal|useStandardCommands; Enum|useStandardCommands; FilterCriterion|useStandardCommands; InformationRegister|useStandardCommands; Report|useStandardCommands; Table|useStandardCommands
platform-property-md-UseStandardCommands = Использовать стандартные команды

# Configuration|usedMobileApplicationFunctionalities
platform-property-md-UsedMobileApplicationFunctionalities = Используемая функциональность мобильного приложения

# WebSocketClient|user
platform-property-md-User = Пользователь

# CommonAttribute|usersSeparation
platform-property-md-UsersSeparation = Разделение пользователей

# StyleItem|value
platform-property-md-Value = Значение

# ExternalReport|variantsStorage; Report|variantsStorage
platform-property-md-VariantsStorage = Хранилище вариантов

# Configuration|vendor
platform-property-md-Vendor = Поставщик

# Configuration|vendorInformationAddress
platform-property-md-VendorInformationAddress = Адрес информации о поставщике

# Configuration|version
platform-property-md-Version = Версия

# InformationRegister|writeMode
platform-property-md-WriteMode = Режим записи

# WebService|xdtoPackages
platform-property-md-XDTOPackages = Пакеты XDTO

# Operation|xdtoReturningValueType
platform-property-md-XDTOReturningValueType = Тип возвращаемого значения

# Parameter|xdtoValueType
platform-property-md-XDTOValueType = Тип значения

# ExchangePlan|defaultPresentation
platform-property-md-exchange-plan-DefaultPresentation = Основное представление плана обмена

# InformationRegister|defaultRecordForm
platform-property-md-information-register-DefaultRecordForm = Основная форма редактирования записи

# Sequence|registerRecords
platform-property-md-sequence-RegisterRecords = Движения, влияющие на последовательность

# StyleItem|type
platform-property-md-style-item-Type = Вид

# Task|defaultPresentation
platform-property-md-task-DefaultPresentation = Представление по умолчанию

# ChartOfAccountsPredefinedItem|accountType
platform-property-predef-AccountType = Вид

# Reviewed Designer XML structural field
platform-property-predef-AccountingFlag = Признак учета

# ChartOfAccountsPredefinedItem|accountingFlags
platform-property-predef-AccountingFlags = Признаки учета

# ChartOfCalculationTypesPredefinedItem|actionPeriodIsBase
platform-property-predef-ActionPeriodIsBase = Период действия является базовым периодом

# ChartOfCalculationTypesPredefinedItem|base
platform-property-predef-Base = Базовые

# ChartOfCalculationTypesPredefinedItem|code
platform-property-predef-Code = Код

# PredefinedItem|description
platform-property-predef-Description = Наименование

# ChartOfCalculationTypesPredefinedItem|displaced
platform-property-predef-Displaced = Вытесняющие

# Reviewed Designer XML structural field
platform-property-predef-ExtDimensionAccountingFlag = Признак учета субконто

# Reviewed Designer XML structural field
platform-property-predef-ExtDimensionType = Вид субконто

# ChartOfAccountsPredefinedItem|extDimensionTypes
platform-property-predef-ExtDimensionTypes = Виды субконто

# ChartOfCharacteristicTypesPredefinedItem|isFolder
platform-property-predef-IsFolder = Это группа

# Reviewed Designer XML structural field
platform-property-predef-Item = Элемент

# ChartOfCalculationTypesPredefinedItem|leading
platform-property-predef-Leading = Ведущие

# PredefinedItem|name
platform-property-predef-Name = Имя

# ChartOfAccountsPredefinedItem|offBalance
platform-property-predef-OffBalance = Забалансовый

# ChartOfAccountsPredefinedItem|order
platform-property-predef-Order = Порядок

# BasicFeature|type; ChartOfCharacteristicTypes|type; Constant|type; DefinedType|type; Dimension|type; Field|type; FilterCriterion|type; Function|type; Resource|type; SequenceDimension|type; SessionParameter|type
platform-property-predef-Type = Тип

# Reviewed Designer XML structural field
platform-property-readable-AutoRecord = Авторегистрация

# Reviewed Designer XML structural field
platform-property-readable-Characteristic = Характеристика

# CharacteristicsDescription|characteristicTypes
platform-property-readable-CharacteristicTypes = Виды характеристик

# CharacteristicsDescription|characteristicValues
platform-property-readable-CharacteristicValues = Значения характеристик

# Reviewed Designer XML structural field
platform-property-readable-CheckState = Состояние выбора

# BasicFeature|choiceForm; Constant|choiceForm; Dimension|choiceForm; Field|choiceForm; Resource|choiceForm
platform-property-readable-ChoiceForm = Форма выбора

# AccountingFlag|choiceHistoryOnInput; AccountingRegisterAttribute|choiceHistoryOnInput; AccountingRegisterDimension|choiceHistoryOnInput; AccountingRegisterResource|choiceHistoryOnInput; AccumulationRegisterAttribute|choiceHistoryOnInput; AccumulationRegisterDimension|choiceHistoryOnInput; AccumulationRegisterResource|choiceHistoryOnInput; AddressingAttribute|choiceHistoryOnInput; BusinessProcessAttribute|choiceHistoryOnInput; BusinessProcess|choiceHistoryOnInput; CalculationRegisterAttribute|choiceHistoryOnInput; CalculationRegisterDimension|choiceHistoryOnInput; CalculationRegisterResource|choiceHistoryOnInput; CatalogAttribute|choiceHistoryOnInput; Catalog|choiceHistoryOnInput; ChartOfAccountsAttribute|choiceHistoryOnInput; ChartOfAccounts|choiceHistoryOnInput; ChartOfCalculationTypesAttribute|choiceHistoryOnInput; ChartOfCalculationTypes|choiceHistoryOnInput; ChartOfCharacteristicTypesAttribute|choiceHistoryOnInput; ChartOfCharacteristicTypes|choiceHistoryOnInput; CommonAttribute|choiceHistoryOnInput; Constant|choiceHistoryOnInput; DataProcessorAttribute|choiceHistoryOnInput; DataProcessorTabularSectionAttribute|choiceHistoryOnInput; Dimension|choiceHistoryOnInput; DocumentAttribute|choiceHistoryOnInput; Document|choiceHistoryOnInput; Enum|choiceHistoryOnInput; ExchangePlanAttribute|choiceHistoryOnInput; ExchangePlan|choiceHistoryOnInput; ExtDimensionAccountingFlag|choiceHistoryOnInput; InformationRegisterAttribute|choiceHistoryOnInput; InformationRegisterDimension|choiceHistoryOnInput; InformationRegisterResource|choiceHistoryOnInput; ReportAttribute|choiceHistoryOnInput; ReportTabularSectionAttribute|choiceHistoryOnInput; Table|choiceHistoryOnInput; TabularSectionAttribute|choiceHistoryOnInput; TaskAttribute|choiceHistoryOnInput; Task|choiceHistoryOnInput
platform-property-readable-ChoiceHistoryOnInput = История выбора при вводе

# BasicFeature|choiceParameterLinks; Constant|choiceParameterLinks; Dimension|choiceParameterLinks; Field|choiceParameterLinks; Resource|choiceParameterLinks
platform-property-readable-ChoiceParameterLinks = Связи параметров выбора

# BasicFeature|choiceParameters; Constant|choiceParameters; Dimension|choiceParameters; Field|choiceParameters; Resource|choiceParameters
platform-property-readable-ChoiceParameters = Параметры выбора

# MdObject|comment
platform-property-readable-Comment = Комментарий

# CommonAttribute|conditionalSeparation
platform-property-readable-ConditionalSeparation = Условное разделение

# BasicDbObject|createOnInput; BasicFeature|createOnInput; Dimension|createOnInput; Field|createOnInput; Table|createOnInput
platform-property-readable-CreateOnInput = Создание при вводе

# DataHistorySupport|dataHistory
platform-property-readable-DataHistory = История данных

# Reviewed Designer XML structural field
platform-property-readable-DataPath = Путь к данным

# CharacteristicsDescription|dataPathField
platform-property-readable-DataPathField = Поле пути к данным

# BasicFeature|editFormat; Constant|editFormat; Dimension|editFormat; Field|editFormat; Resource|editFormat
platform-property-readable-EditFormat = Формат редактирования

# BasicFeature|extendedEdit; Constant|extendedEdit; Dimension|extendedEdit; Field|extendedEdit; Resource|extendedEdit
platform-property-readable-ExtendedEdit = Расширенное редактирование

# Reviewed Designer XML structural field
platform-property-readable-Field = Поле

# BasicFeature|fillChecking; BasicTabularSection|fillChecking; Constant|fillChecking; Dimension|fillChecking; Field|fillChecking
platform-property-readable-FillChecking = Проверка заполнения

# AccountingFlag|fillFromFillingValue; AddressingAttribute|fillFromFillingValue; CommonAttribute|fillFromFillingValue; DataProcessorTabularSectionAttribute|fillFromFillingValue; DbObjectAttribute|fillFromFillingValue; ExtDimensionAccountingFlag|fillFromFillingValue; Field|fillFromFillingValue; InformationRegisterAttribute|fillFromFillingValue; InformationRegisterDimension|fillFromFillingValue; InformationRegisterResource|fillFromFillingValue; ReportTabularSectionAttribute|fillFromFillingValue
platform-property-readable-FillFromFillingValue = Заполнять из данных заполнения

# AccountingFlag|fillValue; AddressingAttribute|fillValue; CommonAttribute|fillValue; DataProcessorTabularSectionAttribute|fillValue; DbObjectAttribute|fillValue; ExtDimensionAccountingFlag|fillValue; Field|fillValue; InformationRegisterAttribute|fillValue; InformationRegisterDimension|fillValue; InformationRegisterResource|fillValue; ReportTabularSectionAttribute|fillValue
platform-property-readable-FillValue = Значение заполнения

# BasicFeature|format; Constant|format; Dimension|format; Field|format; Resource|format
platform-property-readable-Format = Формат

# AccountingRegister|fullTextSearch; AccumulationRegister|fullTextSearch; AddressingAttribute|fullTextSearch; BasicDbObject|fullTextSearch; CalculationRegister|fullTextSearch; DbObjectAttribute|fullTextSearch; InformationRegister|fullTextSearch; RegisterAttribute|fullTextSearch; RegisterDimension|fullTextSearch; RegisterResource|fullTextSearch; TabularSectionAttribute|fullTextSearch
platform-property-readable-FullTextSearch = Полнотекстовый поиск

# Reviewed Designer XML structural field
platform-property-readable-Header = Заголовок

# Reviewed Designer XML structural field
platform-property-readable-Item = Элемент

# CharacteristicsDescription|keyField
platform-property-readable-KeyField = Поле ключа

# Reviewed Designer XML structural field
platform-property-readable-Link = Связь

# BasicFeature|linkByType; Constant|linkByType; Dimension|linkByType
platform-property-readable-LinkByType = Связь по типу

# TypeLink|linkItem
platform-property-readable-LinkItem = Элемент связи по типу

# Reviewed Designer XML structural field
platform-property-readable-LoadTransparent = Загружать прозрачной

# BasicFeature|markNegatives; Constant|markNegatives; Dimension|markNegatives; Resource|markNegatives
platform-property-readable-MarkNegatives = Выделять отрицательные

# BasicFeature|mask; Constant|mask; Dimension|mask; Field|mask; Resource|mask
platform-property-readable-Mask = Маска

# BasicFeature|maxValue; Constant|maxValue; Dimension|maxValue; Field|maxValue
platform-property-readable-MaxValue = Максимальное значение

# Reviewed Designer XML structural field
platform-property-readable-Metadata = Объект метаданных

# BasicFeature|minValue; Constant|minValue; Dimension|minValue; Field|minValue
platform-property-readable-MinValue = Минимальное значение

# BasicFeature|multiLine; Constant|multiLine
platform-property-readable-MultiLine = Многострочный режим

# CharacteristicsDescription|multipleValuesKeyField
platform-property-readable-MultipleValuesKeyField = Поле ключа множественных значений

# CharacteristicsDescription|multipleValuesOrderField
platform-property-readable-MultipleValuesOrderField = Поле порядка множественных значений

# CharacteristicsDescription|multipleValuesUseField
platform-property-readable-MultipleValuesUseField = Поле использования множественных значений

# MdObject|name
platform-property-readable-Name = Имя

# Reviewed Designer XML structural field
platform-property-readable-Object = Объект

# CharacteristicsDescription|objectField
platform-property-readable-ObjectField = Поле объекта

# BasicFeature|passwordMode; Constant|passwordMode; Dimension|passwordMode; Field|passwordMode; Resource|passwordMode
platform-property-readable-PasswordMode = Режим пароля

# Reviewed Designer XML structural field
platform-property-readable-Presentation = Представление

# BasicFeature|quickChoice; Catalog|quickChoice; ChartOfAccounts|quickChoice; ChartOfCalculationTypes|quickChoice; ChartOfCharacteristicTypes|quickChoice; Constant|quickChoice; DimensionTable|quickChoice; Dimension|quickChoice; Enum|quickChoice; ExchangePlan|quickChoice; Field|quickChoice; Resource|quickChoice; Table|quickChoice
platform-property-readable-QuickChoice = Быстрый выбор

# Reviewed Designer XML structural field
platform-property-readable-Ref = Ссылка

# Reviewed Designer XML structural field
platform-property-readable-StandardAttribute = Стандартный реквизит

# StandardTabularSectionDescription|standardAttributes
platform-property-readable-StandardAttributes = Стандартные реквизиты

# Reviewed Designer XML structural field
platform-property-readable-StandardTabularSection = Стандартная табличная часть

# MdObject|synonym
platform-property-readable-Synonym = Синоним

# BasicCommand|toolTip; BasicFeature|toolTip; BasicTabularSection|toolTip; CommandGroup|toolTip; Constant|toolTip; Dimension|toolTip; Field|toolTip; Resource|toolTip
platform-property-readable-ToolTip = Подсказка

# CharacteristicsDescription|typeField
platform-property-readable-TypeField = Поле вида

# InformationRegisterDimension|typeReductionMode
platform-property-readable-TypeReductionMode = Режим сокращения типа

# CharacteristicsDescription|typesFilterField
platform-property-readable-TypesFilterField = Поле отбора видов

# CharacteristicsDescription|typesFilterValue
platform-property-readable-TypesFilterValue = Значение отбора видов

# FunctionalOptionsParameter|use; HierarchicalDbObjectAttribute|use; HierarchicalDbObjectTabularSection|use; ScheduledJob|use
platform-property-readable-Use = Использование

# StyleItem|value
platform-property-readable-Value = Значение

# ChoiceParameterLink|changeMode
platform-property-readable-ValueChange = Режим изменения связанного значения

# CharacteristicsDescription|valueField
platform-property-readable-ValueField = Поле значения
