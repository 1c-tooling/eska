# Адресная подготовка метаданных для автодополнения

Первый `textDocument/completion` для прямого обращения `Справочники.Имя.` или
`Catalogs.Name.` получает текущий главный объект и его предопределённые элементы.
Предварительный `eska/metadata/read` больше не требуется для этого обращения.
Сервер использует тот же controller, source budget и canonical slots, что
[дерево](analyzer-metadata.md), [предопределённые данные](analyzer-predefined.md)
и одноразовый checker.

## Разделение ответственности

`Analysis::completion_metadata_target` в закреплённом ядре определяет прямой
MDO target по текущему BSL и позиции. Используются существующие native syntax,
root ownership и execution-environment rules. Строка, комментарий, неподходящее
локальное затенение, недоступный глобальный manager root, `Catalogs.` и
`Metadata.Catalogs.` не становятся запросом на payload конкретного объекта.
Обнаружение зависимости не читает целевой файл и не возвращает LSP DTO.

`AnalysisSession::prepare_completion_metadata` выполняет явную подготовку:
сначала YAML владельца текущего модуля и его primary resource, затем dialog
именно текущей формы, если модуль принадлежит форме. Это позволяет native ядру
учесть текущий модульный контекст. Затем native target связывается через уже
существующий frozen `config_index` с FileId главного descriptor и его владельцем.
Контроллер загружает только этот YAML и primary resource, если они ожидают чтения.
Загрузка целевого объекта не активирует его соседние nested dialogs или BSL bodies.
Поиск FileId использует resident identities; новый index/cache не добавлен.

Transport преобразует текущую UTF-16 позицию, вызывает подготовку и обычный
`module_completions`. Последний остаётся отдельным read-only запросом без source
I/O. SourceReader отвечает за объявленные пути и ограниченное чтение; adapter
и canonical producer формируют модели; native ядро определяет подсказки.
Core не зависит от LSP/CLI, локализованных строк или редактора.

## Текущий ввод, часы и ограничения

Перед подготовкой проверяются module readiness, snapshot, UTF-8 byte boundary
и cancellation. Неактуальная ревизия или неверная позиция не загружают метаданные.
Повторный запрос не перечитывает ready YAML или ресурс. Проверяется readiness
самого input: pending проекция из-за ресурса не разрешает перечитать ready YAML.
Saved cancellation допускает повторное чтение; остальные saved failures
сохраняются до explicit refresh/invalidation. Editor-owned input и его отказ
сохраняют приоритет. Invalid XML/YAML очищает текущую модель; старые items и XML
с диска не становятся fallback. Close восстанавливает фактический saved ввод.

Каждая принятая загрузка, включая отказ, меняет общий `SessionRevision`.
Completion возвращает snapshot после подготовки. До кооперативной остановки
может успеть загрузиться YAML своего модуля; отменённый запрос сообщает этот
актуальный snapshot. Редакторский BSL сохраняет байты и document version.
Нулевой deadline прекращает запрос до чтения. Все входы используют общий
[source budget](source-budget.md); освобождение места само по себе не повторяет
failed saved-чтение. Это не общий бюджет heap/RSS.

Protocol `0.1` добавляет capability `targetedCompletionMetadata: true` внутри
`capabilities.experimental.eska`. Source schemas, JSON result shape, версии
packages и зависимости не изменены. Native facade и shared preparation API
добавлены; API read-only query сохранён.

## Приёмка

Семь shared tests проверяют cold source coverage, обе локали, case-insensitive
native lookup, строки/комментарии/локальное затенение, client/server form context,
собственный dialog, stale/invalid/cancel preflight, ready YAML при pending ресурсе,
редакторский приоритет, saved cancellation retry и quota recovery по событию.
Два native tests проверяют discovery независимо от загрузки; два process tests
проверяют первый запрос без metadata read, UTF-16/BOM/CRLF/non-BMP, состояния
дерева, точный source reserve и восстановление missing/invalid input.
Существующая transport cancellation проверяет оба допустимых момента остановки
до и после загрузки собственного YAML, сохраняя редакторский BSL и ID reuse.

SDK: 316 tests — 154 shared, 144 IDE process, 18 checker process; все 23 gates
feature matrix, strict Clippy, Rustdoc и release прошли. Minimal — 132 tests,
checker — 134/18, analyzer — 154/140; четыре дополнительные IDE tests требуют
обеих CLI features. Native workspace: 2 634 passed / 20 прежних ignored,
все пять gates прошли. Root: 910 passed / 14 прежних ignored и все 15 gates,
включая minimal/project-cli/build-cli и Rustdoc, прошли.

Шесть real stdio сценариев RU/EN использовали полный manifest C1/C2/C3 и owned
частичные копии payloads. В каждой выбран настоящий catalog с predefined XML,
а его owned object module содержит короткое прямое обращение к существующему item.
Остальные payloads намеренно не предоставлены. Manifest/configuration snapshot
проверены при открытии; счётчик текущих inputs перед completion равен нулю.
После первого запроса готовы только module body, YAML его владельца и predefined
resource. Ни metadata read, ни полный semantic scan не выполнялись.

| Corpus | Первый source reserve, bytes | Nested dialogs остались pending | Peak RSS EN/RU, KiB |
|---|---:|---:|---:|
| C1 | 56 739 | 2 | 70 676 / 67 304 |
| C2 | 72 324 | 4 | 253 736 / 258 412 |
| C3 | 29 449 | 1 | 295 096 / 294 920 |

Current resource заменил подсказку, invalid XML удалил прежнюю, watcher сохранил
редакторский отказ, close восстановил saved input. Saved deletion остался отказом
после физического восстановления без события; watched recreation вернула item.
YAML fingerprint и pending forms сохранились; повторный запрос не менял часы или
source reserve. Машинные trace RU/EN совпали. Проверенные 7/9/6 original и 8/10/7
owned files сохранили байты; временные проекты удалены. Это замеры partial copies,
не приёмка полной resident конфигурации, OS watcher или общего memory budget.

Checker C1 сохранил весь предыдущий JSON RU/EN, включая revisions, settings,
1 975 проверенных модулей, 26 findings, 960 metadata issues и 204 388 259 source
bytes. Exit 2 сохраняет явную неполноту покрытия; 7 829 source files не изменены.
495 original native hashes проверены; graph — 206 names / 214 versions.
Release analyzer — 50 578 712 bytes, checker — 46 590 480 bytes.
[Машинный отчёт](analyzer-targeted-completion-results.json) сохраняет результаты.

Подготовка ограничена текущим модулем и прямым manager-object access, распознанным
native ядром. Transitive inference через переменные, чтение чужих module exports,
SDBL, остальные виды metadata, все IDE edits и общий memory budget не принимаются
этим компонентом. Платформа 1С не запускалась: проверены входы и подсказки,
не исполнение BSL. EF-18 остаётся `IN_PROGRESS`; EF-12 и следующие этапы не начаты.
