# Покрытие типов небольшими fixtures

Проверено 2026-09-17. Сверка с текущим справочником: 67 типов.
T64 проверял общую identity-оболочку 65 видов. T65 добавил реквизит адресации
задачи и [матрицу схем](schema-cases.json) с синтетическим XML для всех классов;
корень покрыт четырьмя наборами. Это не загрузка каждого типа в платформу.
Внешние источники данных сохраняют явный fallback для нераспознанных потомков.
«Матрица T65» означает общий синтетический descriptor и проверку групп/модулей,
а не отдельный реальный экспорт. Порядок прямых и сгруппированных потомков
проверяется независимо от порядка XML.

| Тип | XML-пример и ожидаемый ID |
|---|---|
| `configuration` | есть |
| `data-processor` | есть |
| `report` | есть |
| `accounting-register` | есть |
| `accumulation-register` | есть |
| `bot` | Матрица T65 |
| `business-process` | Матрица T65 |
| `calculation-register` | есть |
| `catalog` | есть |
| `chart-of-accounts` | Матрица T65 |
| `chart-of-calculation-types` | Матрица T65 |
| `chart-of-characteristic-types` | Матрица T65 |
| `command-group` | Матрица T65 |
| `common-attribute` | Матрица T65 |
| `common-command` | Матрица T65 |
| `common-form` | Матрица T65 |
| `common-module` | есть |
| `common-picture` | Матрица T65 |
| `common-template` | Матрица T65 |
| `constant` | Матрица T65 |
| `defined-type` | Матрица T65 |
| `document` | есть |
| `document-journal` | Матрица T65 |
| `document-numerator` | Матрица T65 |
| `enum` | есть |
| `event-subscription` | Матрица T65 |
| `exchange-plan` | Матрица T65 |
| `external-data-source` | Матрица T65 |
| `filter-criterion` | Матрица T65 |
| `functional-option` | Матрица T65 |
| `functional-option-parameter` | Матрица T65 |
| `http-service` | есть |
| `information-register` | есть |
| `integration-service` | есть |
| `language` | Матрица T65 |
| `role` | Матрица T65 |
| `scheduled-job` | Матрица T65 |
| `sequence` | Матрица T65 |
| `session-parameter` | Матрица T65 |
| `settings-storage` | Матрица T65 |
| `style` | Матрица T65 |
| `style-item` | Матрица T65 |
| `subsystem` | есть |
| `task` | Матрица T65 |
| `web-service` | есть |
| `web-socket-client` | Синтетические descriptor/resolver/module tests; реальная выгрузка пока не сверена |
| `ws-reference` | Матрица T65 |
| `xdto-package` | Матрица T65 |
| `form` | есть |
| `template` | есть |
| `command` | есть |
| `attribute` | есть |
| `addressing-attribute` | Матрица T65; форма тега сверена с задачей большого стенда |
| `tabular-section` | есть |
| `dimension` | есть |
| `resource` | есть |
| `requisite` | Матрица T65 |
| `enum-value` | есть |
| `accounting-flag` | Матрица T65 |
| `ext-dimension-accounting-flag` | Матрица T65 |
| `recalculation` | есть |
| `column` | Матрица T65 |
| `url-template` | есть |
| `method` | есть |
| `operation` | есть |
| `parameter` | есть |
| `integration-service-channel` | есть |
