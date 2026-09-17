# Покрытие типов небольшими fixtures

Проверено 2026-09-17. Сверка с текущим справочником: 65 типов.
T64 проверяет общую identity-оболочку parser для всех 65 типов. Это не проверка
специальных свойств и коллекций каждого типа. «T65» ниже означает отсутствие
отдельного XML-примера и необходимость дополнить покрытие схем.

| Тип | XML-пример и ожидаемый ID |
|---|---|
| `configuration` | есть |
| `data-processor` | есть |
| `report` | есть |
| `accounting-register` | есть |
| `accumulation-register` | есть |
| `bot` | T65 |
| `business-process` | T65 |
| `calculation-register` | есть |
| `catalog` | есть |
| `chart-of-accounts` | T65 |
| `chart-of-calculation-types` | T65 |
| `chart-of-characteristic-types` | T65 |
| `command-group` | T65 |
| `common-attribute` | T65 |
| `common-command` | T65 |
| `common-form` | T65 |
| `common-module` | есть |
| `common-picture` | T65 |
| `common-template` | T65 |
| `constant` | T65 |
| `defined-type` | T65 |
| `document` | есть |
| `document-journal` | T65 |
| `document-numerator` | T65 |
| `enum` | есть |
| `event-subscription` | T65 |
| `exchange-plan` | T65 |
| `external-data-source` | T65 |
| `filter-criterion` | T65 |
| `functional-option` | T65 |
| `functional-option-parameter` | T65 |
| `http-service` | есть |
| `information-register` | есть |
| `integration-service` | есть |
| `language` | T65 |
| `role` | T65 |
| `scheduled-job` | T65 |
| `sequence` | T65 |
| `session-parameter` | T65 |
| `settings-storage` | T65 |
| `style` | T65 |
| `style-item` | T65 |
| `subsystem` | есть |
| `task` | T65 |
| `web-service` | есть |
| `ws-reference` | T65 |
| `xdto-package` | T65 |
| `form` | есть |
| `template` | есть |
| `command` | есть |
| `attribute` | есть |
| `tabular-section` | есть |
| `dimension` | есть |
| `resource` | есть |
| `requisite` | T65 |
| `enum-value` | есть |
| `accounting-flag` | T65 |
| `ext-dimension-accounting-flag` | T65 |
| `recalculation` | есть |
| `column` | T65 |
| `url-template` | есть |
| `method` | есть |
| `operation` | есть |
| `parameter` | есть |
| `integration-service-channel` | есть |
