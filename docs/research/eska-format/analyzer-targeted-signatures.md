# Адресная подготовка подсказки параметров

Первый `textDocument/signatureHelp` для прямого вызова общего модуля или
менеджера подготавливает его текущий ввод. Предварительный metadata read,
documentSymbol или completion не требуется. Сигнатуры, перегрузки и активный
параметр по-прежнему вычисляет [native call binding](analyzer-call-signatures.md).

## Граница ядра и host

`Analysis::signature_help_dependency_offset` использует текущее native syntax
и `resolve_callee_at`. Ближайший активный список аргументов определяет вызов;
из callee берётся offset его собственного member access. Вложенный вызов имеет
свою цель. Имя общего модуля внутри аргумента локального вызова не становится
зависимостью. Constructor, unqualified и inactive call не дают member anchor.
Поиск не читает payload и не создаёт индекс.

`AnalysisSession::prepare_signature_help_metadata` отдельно проверяет ready
caller, snapshot, byte boundary и cancellation. При наличии anchor он вызывает
[общую адресную подготовку](analyzer-targeted-completion.md): текущий YAML своего
модуля и declared primary resource/dialog, затем YAML прямой цели и доступный
[manager](analyzer-manager-completion.md) или
[common-module body](analyzer-common-module-completion.md).
Для платформенного member call подготавливается own context, даже если foreign
metadata target отсутствует. Остальные bodies и sibling dialogs остаются pending.
Непрямой receiver не расширяет загрузку транзитивно.

После подготовки `module_signature_help` вызывается с **исходным** offset внутри
аргументов и новой текущей ревизией. Его отдельный read-only контракт сохранён.
Host не разрешает имя, не извлекает параметры regex и не ранжирует overloads.
Общий controller сохраняет source quota, editor authority и cached saved failures.

## Текущий ввод и протокол

Первый ответ может увеличить snapshot из-за чтения YAML/resource/BSL. Повторный
запрос с принятым snapshot не меняет clocks/reserve. Current target parameters
меняют сигнатуру без изменения hash caller; ошибочный BSL или YAML убирает прежнюю
сигнатуру. Saved watcher не заменяет editor input, close читает фактический saved
файл. Missing body освобождает свой reserve; восстановление без события не
повторяет failed saved read, watched recreation разрешает новую загрузку.

Discovery и чтение используют прежние guards отмены, deadline, source versions
и UTF-16 conversion. Подготовка не добавляет semantic visibility policy в native
signature query. В частности, доступность для холодной загрузки общего модуля
определяется прежними completion rules; разрешение уже готового вызова остаётся
у call binding. Coverage по-прежнему явно сообщает неполноту.

Protocol `0.1` добавляет `inputPreparation: "direct-member-targets"` внутрь
`capabilities.experimental.eska.signatureHelp`. Signature result shape и source
schemas сохраняются. Экспериментальный Rust API получает отдельную preparation
функцию и native anchor query; версии и зависимости не меняются.

## Проверки

Четыре shared tests проверяют cold common/manager signatures, exact source reserve,
current parameters, invalid/close, pending соседей, argument namespace, local
shadowing, constructor, stale/Unicode/cancellation preflight. Два native tests
проверяют ближайший member callee и non-member/inactive/non-code rejection.
Три новых process tests проверяют cold common/manager и incidental argument
namespace через framed stdio RU/EN. Прежний cross-module test теперь начинает с
холодной сигнатуры; rename/invalid YAML и текущие параметры остаются проверенными.

SDK: 335 tests — 166 shared, 151 IDE process, 18 checker process; все 23 gates,
strict Clippy, Rustdoc и release прошли. Minimal — 144, checker — 146/18,
analyzer — 166/147; четыре IDE tests требуют обеих CLI features.
Native workspace — 2 642 passed / 20 прежних ignored, все пять gates прошли.
Root — 910 passed / 14 прежних ignored, все 15 gates включая minimal/project-cli/
build-cli и Rustdoc прошли.

Двенадцать real stdio сценариев C1/C2/C3 RU/EN используют полный manifest и
owned partial copies. Настоящий exported метод выбран native differential
completion в **отдельном** процессе. В новом процессе первым source consumer
является signatureHelp, target body сохранён без изменений. После первого
запроса учтены ровно caller и target BSL, own/target YAML и required primary
resource. Незадействованный сосед и 2/4/1 manager nested dialogs остаются pending.

| Corpus | Цель | Saved body, bytes | Saved parameters | Первый source reserve, bytes | Peak RSS EN/RU, KiB |
|---|---|---:|---:|---:|---:|
| C1 | Общий модуль | 884 | 0 | 9256 | 61020 / 59228 |
| C1 | Модуль менеджера | 3025 | 1 | 59821 | 58172 / 61608 |
| C2 | Общий модуль | 206 | 1 | 8769 | 258240 / 258304 |
| C2 | Модуль менеджера | 20223 | 1 | 92513 | 260084 / 259824 |
| C3 | Общий модуль | 288 | 0 | 8706 | 293528 / 293588 |
| C3 | Модуль менеджера | 817 | 1 | 30307 | 296604 / 296688 |

Current/invalid BSL, invalid YAML, watch/close и watched deletion/recreation
проверены для обоих видов targets. Parameter labels независимо декодированы
из UTF-16; non-BMP символ стоит перед курсором и в новом default value.
Repeated queries сохраняют clocks/reserve, RU/EN machine traces совпали.
9 original / 7 owned files common-module cases и 11/13/10 original / 9/11/8 owned
files manager cases сохранили байты. После current owner YAML revisions моделей
форм закономерно обновляются; identities и pending состояния dialogs сохраняются.
Временные проекты удалены, PATH пустой, normal exit не требует stdin EOF.

Checker C1 сохранил весь предыдущий JSON RU/EN: settings, revisions, coverage,
1 975 modules, 26 findings, 960 metadata issues, 204 388 259 source bytes и exit 2.
Все 7 829 source files и 495 original native hashes сохранены; runtime graph —
206 names / 214 versions. Release analyzer — 50 613 024 bytes, checker — 46 593 104.
[Машинный отчёт](analyzer-targeted-signatures-results.json) содержит gates и замеры.

Подготовка ограничена active member calls и прямыми canonical targets. Global
unqualified exports, transitive inference, остальные inputs и полная семантика
этим компонентом не принимаются. Partial copies не подтверждают общий heap/RSS
budget; проверены explicit watcher events, не OS watcher. Платформа 1С не запускалась.
EF-18 остаётся `IN_PROGRESS`; следующие этапы не начаты.
