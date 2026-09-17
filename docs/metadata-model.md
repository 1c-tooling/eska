# Модель метаданных для IDE

Реализованная основа T60 находится в `src/project/metadata_model`.
Это библиотечная модель; команды `eska ide` и расширения VS Code пока нет.

`MetadataObject` хранит тип, имя, UUID, родителя и `ObjectId`, без XML DOM и
файловых путей. Существующий `LogicalObject` содержит эту модель вместе со
своим сопоставлением исходников; второго индекса объектов не создаётся.

`ObjectId` сохраняет прежний формат `kind:name/child-kind:name`. В имени
экранируются `%` → `%25`, `/` → `%2F`, `:` → `%3A` именно в таком порядке.
Unicode сохраняется; строка не нормализуется по регистру, locale или ОС.
Имена берутся из метаданных, не из синонимов или физических каталогов.
UUID не участвует в identity и может повторяться. Логическое переименование
меняет ID; перенос проекта или смена source root — нет.

`NodeId` различает реальный объект, модуль `(owner, role)` и виртуальную
коллекцию `(owner, kind)`. Это разные варианты, поэтому «Модули» не конфликтуют
с одноимённым объектом. Роли модулей: общий модуль владельца, объект, менеджер,
набор записей, менеджер значения, управляемое/обычное приложение, сеанс,
внешнее соединение и команда. Наличие файла и допустимость роли для владельца
проверяются последующими resolver/schema; модель не сканирует диск.

`ScopedNodeId` добавляет `ProjectScope`: standalone либо имя workspace member.
Область действия — один контекст discovery. Несколько независимых контекстов
в одной IDE-сессии должны иметь разные пространства имён на уровне сессии;
абсолютные пути не включаются в логическую identity. Wire-формат этих новых
типов ещё не утверждён: он относится к T69, не к текущему CLI JSON.

`MetadataProject::from_root_descriptor` сверяет `ProjectConfiguration` из
manifest с корневым XML через существующий namespace-aware детектор.
Конфигурация и расширение имеют общий XML-тег, но разные `ProjectType`.
Внешний отчёт/обработка различаются с вложенными объектами того же вида по
типу проекта. Открытие только через обязательный manifest и поиск именованного
корневого descriptor относятся к T61; сама проверка типа не требует платформы
1С или заполненного `[build].platform_version`.

## Матрица типов

Единый справочник содержит 65 логических типов. Алиасы внешних descriptors
не создают новый вид объекта. Все 42 вида из корневого списка реального
`big_configuration_temp` проверены по `Configuration.xml` 2026-09-17.
Обозначение «корень стенда» означает наличие в этой выгрузке, а не полную
приёмку parser/schema. Другие виды перенесены из существующего T19; вложенные
узлы сервисов дополнительно сверены на настоящих XML этого стенда. Полная
матрица fixtures и схем дерева развивается в T62/T64/T65.

| Designer XML | Machine kind | Проверка на большом стенде |
|---|---|---|
| Configuration | `configuration` | — |
| ExternalDataProcessor, DataProcessor | `data-processor` | корень стенда |
| ExternalReport, Report | `report` | корень стенда |
| AccountingRegister | `accounting-register` | корень стенда |
| AccumulationRegister | `accumulation-register` | корень стенда |
| Bot | `bot` | — |
| BusinessProcess | `business-process` | корень стенда |
| CalculationRegister | `calculation-register` | — |
| Catalog | `catalog` | корень стенда |
| ChartOfAccounts | `chart-of-accounts` | корень стенда |
| ChartOfCalculationTypes | `chart-of-calculation-types` | корень стенда |
| ChartOfCharacteristicTypes | `chart-of-characteristic-types` | корень стенда |
| CommandGroup | `command-group` | корень стенда |
| CommonAttribute | `common-attribute` | корень стенда |
| CommonCommand | `common-command` | корень стенда |
| CommonForm | `common-form` | корень стенда |
| CommonModule | `common-module` | корень стенда |
| CommonPicture | `common-picture` | корень стенда |
| CommonTemplate | `common-template` | корень стенда |
| Constant | `constant` | корень стенда |
| DefinedType | `defined-type` | корень стенда |
| Document | `document` | корень стенда |
| DocumentJournal | `document-journal` | корень стенда |
| DocumentNumerator | `document-numerator` | корень стенда |
| Enum | `enum` | корень стенда |
| EventSubscription | `event-subscription` | корень стенда |
| ExchangePlan | `exchange-plan` | корень стенда |
| ExternalDataSource | `external-data-source` | — |
| FilterCriterion | `filter-criterion` | корень стенда |
| FunctionalOption | `functional-option` | корень стенда |
| FunctionalOptionsParameter | `functional-option-parameter` | корень стенда |
| HTTPService | `http-service` | корень стенда |
| InformationRegister | `information-register` | корень стенда |
| IntegrationService | `integration-service` | корень стенда |
| Language | `language` | корень стенда |
| Role | `role` | корень стенда |
| ScheduledJob | `scheduled-job` | корень стенда |
| Sequence | `sequence` | корень стенда |
| SessionParameter | `session-parameter` | корень стенда |
| SettingsStorage | `settings-storage` | корень стенда |
| Style | `style` | — |
| StyleItem | `style-item` | корень стенда |
| Subsystem | `subsystem` | корень стенда |
| Task | `task` | корень стенда |
| WebService | `web-service` | корень стенда |
| WSReference | `ws-reference` | корень стенда |
| XDTOPackage | `xdto-package` | корень стенда |
| Form | `form` | — |
| Template | `template` | — |
| Command | `command` | — |
| Attribute | `attribute` | — |
| TabularSection | `tabular-section` | — |
| Dimension | `dimension` | — |
| Resource | `resource` | — |
| Requisite | `requisite` | — |
| EnumValue | `enum-value` | — |
| AccountingFlag | `accounting-flag` | — |
| ExtDimensionAccountingFlag | `ext-dimension-accounting-flag` | — |
| Recalculation | `recalculation` | — |
| Column | `column` | — |
| URLTemplate | `url-template` | вложенный узел сервиса |
| Method | `method` | вложенный узел сервиса |
| Operation | `operation` | вложенный узел сервиса |
| Parameter | `parameter` | вложенный узел сервиса |
| IntegrationServiceChannel | `integration-service-channel` | вложенный узел сервиса |

Неизвестный тег возвращает `UnknownMetadataKind` с исходным именем, без
подмены знакомым типом. Существующие CLI IDs сохранены. Индекс T19 дополнен
пятью ранее пропускавшимися вложенными видами сервисов; их подписи добавлены
в обе локали. Поведение прежних 60 видов не меняется.
