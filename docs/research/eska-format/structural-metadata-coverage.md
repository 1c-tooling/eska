# Структурная проверка листовых метаданных

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

## CommonPicture и CommandGroup

Следующая часть допускает ещё два проверенных kind через общий
`is_structural_descriptor_kind`. SDK использует тот же predicate; произвольный
`Unsupported` не становится `StructureOnly`. Для `CommonPicture` проверяются
два флага доступности. Для `CommandGroup` — enum `Representation` и `Category`,
локализованный `ToolTip`, пустая картинка либо native readable reference с
`Ref` и `LoadTransparent`. Допустимы домены `StdPicture` и `CommonPicture` с
проверенным именем. Наличие картинки и её binary payload не проверяются.

Неизвестные MD fields, qualifiers, непроверенные inline picture и
`SuppressObject` отклоняются. Семантические kinds, в том числе
`SessionParameter`, не получают структурное разрешение. Старые три kind
сохраняют свой контракт. Lifecycle, локализация координат и отказ на текущем
неверном descriptor используют прежние SDK inputs.

Полный adapter suite — 85/85; четыре SDK lifecycle, 67 metadata regression,
три framed LSP и два checker groups прошли, включая RU/EN. Strict Clippy обоих
пакетов и scoped fmt прошли. В адресной проверке native-origin CF CommandGroup
и configured-CFE CommonPicture оба текущих descriptor получили `StructureOnly`,
одну modeled declaration и ноль parse issues. BSL/query bodies не читались,
источники сохранены побайтно с mode/mtime, оба процесса завершены.

Это адресная проверка immutable debug binary: зафиксированы 13 файлов части,
но полный build-time fingerprint Rust/Cargo не снимался; бинарник мог включать
соседнюю current-form доработку. Изолированная сборка, весь состав CF/CFE,
платформенная совместимость и полная EF-18/EF-27 этим не приняты.
[Обезличенные результаты](presentation-leaves-results.json).
