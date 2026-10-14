# Структурная проверка Language и Template

Общий SDK проверяет текущие descriptors `Language`, `Template`, `CommonTemplate`
через adapter `structural_descriptor`. У этих leaf kinds нет canonical BSL
объекта в закреплённом kernel. Валидный descriptor получает существующий
`StructureOnly` и одно покрытое объявление; slot resolver возвращает `None`.
Отдельная Configuration либо фиктивный Template model не создаётся.

Проверяются реальный kind/UUID, полный descriptor, дубли и типы present scalar
properties, ownership, unexpected children/references. Missing optional fields
сохраняют отсутствие. Для Language читается LanguageCode; для Template —
TemplateType. Invalid current bytes дают `Invalid` с координатами в текущем
YAML, не модель прежнего диска.

Dirty overlays, watch/close, версии документов, бюджет и input retirement
используют прежний lifecycle. Watch не заменяет черновик; close возвращает
saved source. LocalSyntax по-прежнему получает `SourceOnly`, ноль modeled
descriptors и incomplete metadata context.

## Что означает полнота

Структурная полнота относится только к descriptor fields. Payload шаблона,
XML схемы СКД, `.sdbl`, параметры, настройки и потребители здесь не проверяются.
Valid, missing либо invalid DCS/query resources не получают придуманный DCS
model и не становятся semantic query diagnostics.

`semantic_profile_complete` сохраняет прежний ограниченный контракт:
выполнены выбранные BSL rules при полных YAML/BSL inputs и descriptor coverage.
`semantic_complete = false` сохраняется. На контрольном CF отсутствующие ранее
Language/Template projections больше не делают BSL syntax result incomplete.
Полный query context и EF-27 этим не приняты.

## Проверки

4 новых adapter и 7 SDK tests прошли: exact ranges, malformed/duplicate fields,
current/saved/dirty/watch/close, обе локали, BSL-only coverage и отказ query pulls.
Весь SDK прошёл: 194 unit, 168 analyzer, 23 checker tests. Strict Clippy,
no-default-features и fmt обоих пакетов прошли.

Восемь повторных framed runtime cases настоящих native-origin проектов
подтвердили caller-owned dirty BSL, repair/symbols, saved invalidation и
checker/LSP ParseError byte parity. CF теперь имеет complete descriptor models;
CFE/EPF/ERF syntax сохраняет LocalSyntax/incomplete metadata context.
Установленный IDE-client, DCS execution и platform semantic comparison этой
частью не принимаются. [Обезличенные результаты](structural-metadata-coverage-results.json)
содержат runtime scopes/coverage и все 22 пройденных Rust gates.
