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

`parameters_complete`, `consumer_complete` и `semantic_complete` остаются false;
параметры, вычисляемые поля, связи, итоги и настройки требуют следующих проверок
EF-27. Это ограничение сериализатора отчёта, а не вывод из пустого списка ошибок.

## BOM в отдельном запросе

Начальный UTF-8 BOM теперь сохраняется как native trivia в общем SDBL token
converter. Parser tree содержит прежние байты, output fields и параметры
сохраняют исходные byte ranges; syntax diagnostics и consumer model используют
одно правило. BOM в середине текста или повторный BOM остаются ошибками.
Оба parser entry points проверены на RU/EN query vocabulary, comments, Unicode
и CRLF. SDK current/saved IDE/checker consumer cases также содержат BOM.

Для этого закреплённый пакет `parser` той же upstream-ревизии включён в локальный
native workspace. Package versions не менялись. Исходные hashes записаны в
provenance; native fixtures сохранены побайтно. Их whitespace exceptions
ограничены двумя конкретными файлами с literal source bytes. Snapshot lookup
в одном upstream test привязан к manifest самого crate: прежний heuristic
ошибочно выбирал внешний workspace ESKA. Golden CST не менялся.

## Общий отчёт IDE и checker

`AnalysisSession::query_consumer_report` переводит библиотечный результат в
`QueryConsumerReport`: контекст, coverage и findings без AST, XML или копии
текущего текста. Контекст содержит root/composition, session/input revisions
и source/hash/origin трёх входов: query, owner и resource. Unified form использует
тот же owner snapshot; её `resource_revision` равен null. Отдельный DCS XML
имеет собственную revision. Invalid current resource возвращает `unavailable`
с machine code/location; сохранённый XML не подставляется.

Authoring location имеет tagged precision `exact`, `enclosing_node` либо
`unmapped`. Для XML byte range и UTF-16 endpoints относятся к одному current
resource. Для формы endpoints берутся из уже принятого YAML snapshot. Если
отображения нет, координаты не выдумываются и диагностике нельзя назначить
произвольную строку.

В initialize клиент отдельно передаёт `initializationOptions.eska.queryConsumers:
true`; без `queryAnalysis: true` сервер отвечает invalid params. Capability
`experimental.eska.queryConsumers` версии 1 объявляет точный scope и правило
`query_consumer_output_field_missing`. Существующий
`textDocument/diagnostic` с identifier `eska-query-analysis` добавляет
`result.eska.consumers` только согласовавшему расширение клиенту. Новый RPC,
таймер, writer и cancellation route не создаются. Findings получают локализованное
`message`; остальные machine fields независимы от языка.

В owner navigation `queries[].resource_source` явно задаёт физический путь
authoring resource. Он отличается от `native_resource`, особенно для EPF/ERF,
и позволяет клиенту синхронизировать текущий XML через прежний input journal.

Semantic checker автоматически добавляет optional `data.report.query_files[].consumers` в
прежнюю JSON schema 1. Отчёт совпадает с IDE по findings, coverage и source
stamps; session revisions двух процессов могут различаться. В checker JSON
нет локализованных сообщений, human output показывает их на RU/EN и сохраняет
явные причины неполноты. Syntax checker не добавляет это поле. Неполный semantic
профиль по-прежнему завершается exit 2.

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

Следующий report/transport gate прошёл 16 проверок адаптера/SDK и debug/release
сборок. SDK — 213 default и 509 all-features, adapter — 100; fmt/check/строгие
Clippy и Rustdoc прошли. Все 1508 Rust/Cargo files совпали до и после gate.
Корневые fmt/check/Clippy/default/all-features tests/Rustdoc также прошли:
1508 default и 1629 all-features tests, по 24 opt-in ignored.

Три process-level группы подтверждают CF/configured-CFE на RU/EN, независимый
opt-in и deadline, текущий XML repair/invalid/close, точные Unicode/CRLF ranges,
saved IDE/checker parity и locale-independent checker JSON. Тест текущего XML
не меняет query version; отмена и нехватка контекста не становятся checked empty.
Установленный клиент принят [отдельным прогоном](explorer-query-consumers.md).

После BOM-исправления прошли 16 native/SDK gates и debug/release builds:
4158 native tests (25 ignored), SDK — 213/509 без ignored. Все 1555 Rust/Cargo
files совпали до и после проверок. Корневые шесть gates повторены: 1508/1629 tests,
по 24 opt-in ignored; Clippy и Rustdoc чистые. Package names/versions в обоих
lockfiles не изменились. Полный Explorer — 323/323; прежний неизменяемый VSIX
с новым release analyzer повторил CF/configured-CFE/native-CF приёмку с BOM
в самом query buffer, полными output names и без ложного QueryParseError.
Native CFE отказала semantic context явно; все 12 children завершились.
