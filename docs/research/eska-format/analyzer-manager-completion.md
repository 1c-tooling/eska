# Текущие методы менеджера в автодополнении

Первый `textDocument/completion` для прямого `Catalogs.Name.` или
`Справочники.Имя.` теперь подготавливает также объявленный BSL-модуль менеджера.
Экспортные методы берутся из его текущего текста через прежний native completion.
Предварительный metadata/module read не требуется. Это продолжение
[адресной подготовки метаданных](analyzer-targeted-completion.md).

## Ответственность и текущий ввод

`Analysis::completion_manager_module` использует native discovery прямого MDO,
root ownership, environment rules и существующий `ModuleIndex::resolve_manager`.
Он возвращает resident FileId объявленного менеджера без чтения тела. Строки,
комментарии, неподходящее локальное затенение и обращения к коллекции не получают
целевой модуль. Имена учитывают native case-insensitive lookup.

`AnalysisSession::prepare_completion_metadata` связывает FileId с уже объявленным
source input и готовит ровно этот manager body через общий controller. Остальные
BSL bodies и соседние dialogs остаются pending. Отдельный `module_completions`
по-прежнему выполняет read-only query; новый path index, parser или cache не добавлен.
Экспортность и сигнатура определяются существующими native symbol tree/signature
queries, без разбора BSL регулярными выражениями в host или transport.

Saved pending и saved cancellation допускают чтение. Ready и editor-owned inputs
не перечитываются; остальные saved failures требуют явного refresh/invalidation.
Binary manager не преобразуется в текст и не получает disk fallback. Загрузка
использует те же лимиты текста, source budget, cancellation и общий SessionRevision.
Completion возвращает snapshot после принятой подготовки, включая отказ чтения.

Overlay заменяет методы сохранённого текста. Некорректный BSL остаётся текущим
текстовым input, но его native symbol tree больше не возвращает прежние exports.
Событие saved watcher сохраняет редакторский input. Close читает фактический saved
файл; удаление освобождает его source reserve и прежние методы. Физическое
восстановление файла без события само по себе не повторяет failed saved-чтение.
YAML и predefined resource сохраняют отдельные входы и ревизии.

Protocol `0.1` добавляет `managerModuleCompletion: true` внутри
`capabilities.experimental.eska`. Source/result schemas, package versions и
dependencies прежние. Native facade расширен; shared preparation сохраняет свою
сигнатуру, а read-only completion contract не меняется.

## Проверки

Четыре shared tests проверяют cold адресную загрузку, реальные native candidates,
RU/EN и case-insensitive lookup, private exclusion, точный source reserve,
overlay/invalid/watch/close, saved cancellation retry, editor cancellation authority,
quota failure, binary/missing input и отсутствие неявного retry. Два native tests
проверяют выбор FileId; два process tests проверяют первый completion без чтения
дерева и lifecycle текущего менеджера через framed stdio в обеих локалях.

SDK: 322 tests — 158 shared, 146 IDE process, 18 checker process; все 23 gates
feature matrix, strict Clippy, Rustdoc и release прошли. Minimal — 136 tests,
checker — 138/18, analyzer — 158/142; четыре IDE tests требуют обеих CLI features.
Native workspace: 2 636 passed / 20 прежних ignored, пять gates прошли.
Root: 910 passed / 14 прежних ignored; все 15 gates, включая минимальные features,
project-cli/build-cli и Rustdoc, прошли.

Шесть stdio сценариев C1/C2/C3 RU/EN использовали полный frozen manifest и owned
частичные копии payloads: настоящий YAML, predefined XML и неизменённый saved
manager BSL, короткий собственный caller. Остальные payloads намеренно отсутствовали.
До первого completion дерево и manager body не читались. Сравнение native candidates
с invalid manager подтвердило именно exports сохранённого реального текста.

| Corpus | Manager body, bytes | Saved exports | Первый source reserve, bytes | Pending dialogs | Peak RSS EN/RU, KiB |
|---|---:|---:|---:|---:|---:|
| C1 | 3 025 | 2 | 59 764 | 2 | 54 476 / 60 224 |
| C2 | 20 223 | 2 | 92 547 | 4 | 257 928 / 258 276 |
| C3 | 817 | 1 | 30 266 | 1 | 294 484 / 293 668 |

Current manager заменил saved exports, private method исключён, invalid BSL удалил
прежние методы, close восстановил реальный saved текст. Предопределённые элементы
сохранились. Watched deletion/recreation проверили отказ, освобождение bytes и
восстановление; repeated completion не менял часы/source reserve. Машинные trace
RU/EN совпали. Проверенные 8/10/7 original и 9/11/8 owned files сохранили байты;
временные проекты удалены. PATH пустой; normal exit не требовал stdin EOF.

Checker C1 сохранил полный предыдущий JSON RU/EN: revisions, settings, coverage,
1 975 проверенных модулей, 26 findings, 960 metadata issues и 204 388 259 source
bytes. Exit 2 сохраняет явную неполноту; 7 829 source files не изменены.
495 original native hashes проверены; runtime graph прежний — 206 names / 214
versions. Release analyzer — 50 584 128 bytes, checker — 46 591 296 bytes.
[Машинный отчёт](analyzer-manager-completion-results.json) сохраняет gates и замеры.

Подготовка ограничена менеджером прямого native MDO access. Common-module exports,
transitive inference через переменные, остальные BSL bodies, SDBL и полная
семантика не принимаются этим компонентом. Partial copies не подтверждают полный
resident RSS или общий memory budget. Проверены явные watcher events; OS watcher
и платформа 1С не запускались. EF-18 остаётся `IN_PROGRESS`; следующие этапы не начаты.
