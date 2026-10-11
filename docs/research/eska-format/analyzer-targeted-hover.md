# Адресная подготовка карточки метода

Первый `textDocument/hover` на имени метода прямого common/manager вызова
подготавливает его текущий ввод. Предварительный metadata read, documentSymbol
или completion не требуется. Документацию, объявление и возвращаемый тип
вычисляет [позиционная карточка ядра](analyzer-symbol-hover.md).

## Граница ядра и host

`Analysis::symbol_hover_dependency_offset` использует текущее native syntax
и `NameClass::FieldName { is_call: true }`. Имя called member, включая оба края,
выбирает offset своего dot. Receiver, metadata object, plain field, constructor,
local callee, string и comment не выбирают dependency anchor. Вложенный вызов
имеет собственную цель. Discovery не читает payload и не создаёт индекс.

`AnalysisSession::prepare_symbol_hover_metadata` проверяет ready caller,
snapshot, byte boundary и cancellation до IO. При наличии anchor вызывается
[общая подготовка](analyzer-targeted-completion.md): current own YAML и declared
primary resource/dialog, затем YAML прямой цели и доступный
[manager](analyzer-manager-completion.md) или
[common-module body](analyzer-common-module-completion.md).
Непрямые receivers не расширяют загрузку транзитивно. Другие bodies и sibling
dialogs остаются pending. Non-callee tokens не читают metadata через этот путь.

Signature и hover используют общий preparation helper с двумя фактическими
native discovery consumers. После подготовки `module_symbol_hover` вызывается
с **исходным** offset имени и новой ревизией; отдельный read-only контракт
сохранён. Host не разбирает BSL declarations, не разрешает символ и не выводит тип.
Общий input controller сохраняет source quota, editor authority и saved failures.

## Владение receiver

Проверка обнаружила ошибку native positional API: textual qualified lookup
возвращал export одноимённого общего модуля при локальном receiver, даже когда
target уже был ready. Ограничение холодной загрузки не исправляло этот случай.

Для called-member token теперь сначала применяется native typed method
resolution. Namespace может быть lowered как один path без отдельно типизированного
receiver; его прежний textual route допускается только после общего с completion
`bare_global_name_claim`, с `Resolver::for_module`. Проверяется реальный user owner
в body/module/form/object scope; workspace namespace не считается своим затенением.
Локальная переменная сохраняет собственный платформенный метод.

Named query и plain-field resolution этой правкой не изменены. Краткий native
method label по-прежнему не содержит параметры; их предоставляет signatureHelp.
Новая семантическая модель или копия правил shadowing в host не добавлена.

## Текущий ввод и протокол

Первый ответ может увеличить snapshot из-за чтения YAML/resource/BSL. Повтор с
принятым snapshot сохраняет clocks/reserve. Current target BSL меняет docs/type
без изменения hash/span caller. Invalid BSL/YAML убирает прежнюю карточку.
Saved watcher не заменяет overlay, close читает фактический saved файл.
Missing body освобождает свой reserve; восстановление без события сохраняет
cached failure, watched recreation разрешает повторную загрузку.

Protocol `0.1` добавляет `inputPreparation: "direct-member-callees"` внутрь
`capabilities.experimental.eska.hover`. Hover result shape и source schemas
сохраняются. Экспериментальный Rust API получает отдельные preparation и native
anchor methods. Версии, dependencies, локализованные поля и error kinds прежние.

## Проверки

Четыре shared tests проверяют cold common/manager cards, exact reserve,
current docs/type, original token span, invalid/close, pending соседей,
non-callee/no-read, local shadowing до/после загрузки и stale/Unicode/cancellation
preflight. Два native anchor tests проверяют callee boundaries и non-callee
rejection; два native regressions проверяют parameter/local/module-variable/
assignment ownership, обычный common export и typed platform member.
Оба regressions воспроизвели ошибку до исправления и прошли после него.

Два новых framed stdio tests проверяют cold common/manager RU/EN, current docs/type,
watch/invalid/close и оба края имени. Прежний cross-module test начинает с холодной
карточки и сохраняет current YAML rename refusal без XML fallback.
SDK — 341 tests: 170 shared, 153 IDE process, 18 checker process. Все 23 gates,
strict Clippy, Rustdoc и release прошли. Minimal — 148, checker — 150/18,
analyzer — 170/149; четыре IDE tests требуют обеих CLI features.
Native workspace — 2 646 passed / 20 прежних ignored, все пять gates прошли.
Root — 910 passed / 14 прежних ignored, все 15 gates прошли.

Двенадцать real stdio сценариев C1/C2/C3 RU/EN используют полные manifests
и owned partial copies. Настоящий saved метод выбран native differential
completion в **отдельном** процессе. В свежем процессе первым source consumer
является hover; настоящий target body сохранён без изменений.

| Corpus | Цель | Saved body, bytes | Первый source reserve, bytes | Peak RSS EN/RU, KiB |
|---|---|---:|---:|---:|
| C1 | Общий модуль | 884 | 9256 | 72036 / 72216 |
| C1 | Модуль менеджера | 3025 | 59821 | 72772 / 72572 |
| C2 | Общий модуль | 206 | 8769 | 256652 / 256632 |
| C2 | Модуль менеджера | 20223 | 92513 | 258920 / 260708 |
| C3 | Общий модуль | 288 | 8706 | 292520 / 292772 |
| C3 | Модуль менеджера | 817 | 30307 | 295624 / 295912 |

Учтены ровно два BSL и required own/target YAML/resource. Незадействованный
owner и 2/4/1 manager nested dialogs остаются pending. Current purpose/String
return type, invalid BSL/YAML, watch/close, loaded-target local shadowing и saved
deletion/recreation проверены для обоих targets. Hash, byte/UTF-16 span и оба
края callee проверены независимо с BOM/CRLF/non-BMP. RU/EN machine traces совпали
после исключения только human contents и opaque connection identifiers.
Байты 9 original / 7 owned files common cases и 11/13/10 original / 9/11/8 owned
files manager cases сохранились. Временные проекты удалены, PATH пустой;
normal exit не требует stdin EOF.

Checker C1 сохранил полный предыдущий JSON RU/EN: 1 975 modules, 26 findings,
960 metadata issues, settings, revisions, coverage, source reserve 204 388 259
bytes и exit 2. Все 7 829 source files и 495 original native hashes сохранены.
Runtime graph — 206 names / 214 versions. Release analyzer — 50 611 144 bytes,
checker — 46 596 992. [Машинный отчёт](analyzer-targeted-hover-results.json)
содержит gates и замеры.

Подготовка ограничена called-member tokens и прямыми canonical targets.
Global unqualified exports, прочие inputs и полная семантика этим компонентом
не принимаются. Partial copies не подтверждают общий heap/RSS budget;
проверены explicit watcher events, не OS watcher. Платформа 1С не запускалась.
EF-18 остаётся `IN_PROGRESS`; следующие этапы не начаты.
