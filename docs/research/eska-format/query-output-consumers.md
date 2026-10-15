# Поля результата и явные потребители запроса

Library API `prepare_query_consumers` и `query_consumer_diagnostics` соединяет
текущий `.sdbl`, его owner/resource snapshot и native metadata выбранного CF
либо явно настроенной CFE. `QueryConsumerDiagnostics` сохраняет прежние
root/revision/hash/origin stamps, native модель, findings и причины неполноты.
BSL-тело и сохранённый XML вместо текущего overlay не создаются и не читаются.

## Native модель

`Analysis::query_text_model` использует существующие parser/HIR и
`FileQueryResolver` с native descriptor FileId владельца. Границы top-level
SELECT/DROP берутся из AST; flattened UNION branches ядра группируются внутри
своего SELECT. Временные таблицы, порядок полей и повторы сохраняются.

`QueryOutputField` содержит original alias/column name, ordinal, field/expression
и name ranges, native `QueryOutputType` и признак известного типа. Последний
identifier сложного выражения остаётся отдельным heuristic `raw_name` и не
получает права называться результатом. `*` не разворачивается; типы UNION не
объявляются объединёнными. Unknown/Error внутри composite/defined type остаются
неполными. Все native ranges относятся к исходному UTF-8 тексту.

Параметры сохраняют каждое настоящее AST occurrence, написание и диапазон имени
без `&`; comments/string literals не становятся параметрами. Это не декларации
и не доказанные типы параметров. Одноимённый `SessionParameter` их не определяет.

Модель содержит structured native diagnostics. Нулевой список не означает
валидность: неизвестный вызов может не выдать native issue. Поэтому отдельно
проверяются известные HIR operations/functions во всех моделируемых clauses,
включая вложенные выражения и UNION. Отмена проверяется до/после native parse и
lowering и между projected items; немедленная отмена внутри parser не обещается.

## Проверяемые bindings

Для DCS `DataSetFieldField` сравнивается явный `field` только соответствующего
query source/slot и полного dataset ancestry. Для manual DynamicList проверяется
прямой UI `DataPath` вида `List.Field` с настоящими attribute/element identities.
Finding отсутствующего поля указывает source resource, native range и точность
authoring range: XML Exact отличается от enclosing YAML node.

Например, после замены `SELECT 1 AS Code` на `SELECT 1 AS Name` текущая ссылка
DCS на `Code` получает finding в XML ресурсе. Изменение условия при сохранённом
`Code` не создаёт такой ошибки. Прежний результат другого dataset, query source
или root не служит fallback.

Отсутствие доказывается только для одного однозначного result set, полного
набора имён и известных типов каждой UNION branch, без native runtime errors
или неподдержанных выражений. Duplicate names сравниваются по native правилам
без изменения исходного написания. Stars, unnamed/unknown outputs, несколько
result sets, temporary packages, nested DataPath, неоднозначные lists и
platform-enriched MainTable дают явные причины неполноты, а не missing field.
Непроверенные resource parts сохраняют собственные координаты.

Это экспериментальный library contract. Новый RPC, JSON/batch schema и IDE
consumer diagnostics этой частью не добавлены. `parameters_complete`,
`consumer_complete` и `semantic_complete` остаются false; параметры, вычисляемые
поля, связи, итоги и настройки требуют следующих проверок EF-27.

## Проверки

Девять native tests проверяют реальные AST/HIR facts, UNION/package boundaries,
точные Unicode/CRLF ranges, параметры и неподдержанные вложенные выражения.
Десять SDK groups (девять без composition feature) проверяют DCS/DynamicList,
condition-only изменения, неоднозначные имена, current failures, cancellation
и root visibility. Сопоставление имён следует native Unicode case folding,
включая различие греческой конечной сигмы.

Общий замороженный набор native/adapter/SDK прошёл 21 gate и обе сборки binaries:
fmt, check, строгий Clippy/Rustdoc, SDK default/all-features. SDK — 213/506,
adapter — 100, selected native workspace — 3310 passed / 21 ignored. Все 1502
Rust/Cargo files совпали до и после проверок и release build. Корневое приложение
этим срезом не изменено; его отдельный ранее выполненный gate сохраняется.
Новый запуск платформы 1С для consumer library не выполнялся.
