# Текущий общий модуль в адресном автодополнении

Первый `textDocument/completion` для прямого `ИмяОбщегоМодуля.` подготавливает
его YAML, затем доступный вызывающему коду объявленный BSL body. Экспортные методы
и сигнатуры берутся из текущего текста прежними native queries. Metadata/module
read заранее не требуется; соседние bodies остаются pending.
Это расширяет [адресную подготовку](analyzer-targeted-completion.md) и дополняет
[текущие методы менеджера](analyzer-manager-completion.md).

## Ответственность и правила имени

`Analysis::completion_metadata_target` теперь возвращает также `CommonModule`
для прямого bare receiver из resident module index. Native dot/prefix syntax
исключает строку, комментарий и chain receiver; lookup имён case-insensitive.
Shared session использует существующий frozen `common_module_index`, чтобы найти
owner FileId, и читает только его YAML. Новый index/cache не добавлен.

После публикации текущих флагов `Analysis::completion_common_module` проверяет
native resolver и `EnvFilter::admits_common_module`. Только доступный namespace
получает body FileId для общего controller. Непрочитанный, invalid или inaccessible
owner не превращается в разрешение читать body. Ready/editor authority, failure
states, source/text budgets, cancellation и SessionRevision прежние. Read-only
`module_completions` по-прежнему не выполняет source I/O.

Исправлена ошибка прежнего fast path: параметр `Utility` получал exports
одноимённого общего модуля. Проверка имени теперь применяется и к discovery, и
к native completion. Общий `bare_global_name_claim` получает текущий body scope
и `Resolver::for_module`: bindings, module/form/object members сохраняют свои
обычные правила, а workspace namespace является проверяемым кандидатом.
Набор локальных имён не перечисляется вручную. Параметр, declared variable или
reaching assignment не наследуют exports чужого модуля; структура локальной
переменной сохраняет собственные поля.

Current BSL заменяет exports; invalid BSL удаляет прежние symbols. Current YAML
сохраняет контроль доступности даже уже загруженного body; invalid YAML очищает
каноническую модель и его exports не становятся fallback. Saved watcher не
перезаписывает editor-owned input. Close восстанавливает фактические saved bytes.
Missing body освобождает свой reserve; восстановление без события не повторяет
failed saved-чтение. Binary/bodyless namespace не получает текстового fallback.

Protocol `0.1` добавляет `commonModuleCompletion: true` внутри
`capabilities.experimental.eska`. Source/result schemas, версии и зависимости
прежние. Native dependency discovery теперь может вернуть `CommonModule`;
shared preparation API сохраняет сигнатуру. Это расширение экспериментального API.

## Проверки

Четыре shared tests проверяют cold source coverage, обе локали, case-insensitive
lookup, private exclusion, текущие BSL/YAML, доступность перед body read и после
него, shadowing до/после загрузки, exact reserve, missing/watch recovery и binary/
bodyless targets. Четыре native tests проверяют FileId/prefix discovery, non-code/
chain rejection, несколько владельцев имени и поля локальной структуры.
Регрессионный native test сначала воспроизвёл ошибку параметра на `af4709d`;
после исправления все его варианты прошли. Два process tests проверяют framed
stdio RU/EN, UTF-16/BOM/CRLF/non-BMP, cold body loading и текущую доступность.

SDK: 328 tests — 162 shared, 148 IDE process, 18 checker process; все 23 gates
feature matrix, strict Clippy, Rustdoc и release прошли. Minimal — 140 tests,
checker — 142/18, analyzer — 162/144; четыре IDE tests требуют обеих CLI features.
Native workspace: 2 640 passed / 20 прежних ignored и пять gates прошли.
Root: 910 passed / 14 прежних ignored и все 15 gates, включая minimal/project-cli/
build-cli и Rustdoc, прошли.

Шесть stdio сценариев C1/C2/C3 RU/EN использовали полный manifest и owned partial
copies: два настоящих YAML, неизменённый target body и короткий собственный caller.
До первого completion source reserve равен нулю; после него учтены ровно два YAML
и два BSL body. Проверяемый соседний owner сохранил pending state.
Native differential completion с invalid body подтвердил настоящие saved exports.

| Corpus | Saved body, bytes | Saved exports | Первый source reserve, bytes | Peak RSS EN/RU, KiB |
|---|---:|---:|---:|---:|
| C1 | 884 | 3 | 9 207 | 76 696 / 76 676 |
| C2 | 206 | 1 | 8 682 | 259 444 / 259 912 |
| C3 | 288 | 1 | 8 663 | 294 492 / 294 536 |

Current/invalid BSL, valid inaccessible/invalid YAML, overlay authority, close,
локальный параметр и watched deletion/recreation дали ожидаемые native candidates.
Repeated completion сохранил часы/reserve; RU/EN machine traces совпали. Девять
проверенных original и семь owned files каждого corpus сохранили байты, включая
native XML/BSL выбранного модуля. Временные проекты удалены; PATH пустой, normal
exit не требовал stdin EOF.

Checker C1 сохранил полный предыдущий JSON RU/EN: revisions, settings, coverage,
1 975 проверенных модулей, 26 findings, 960 metadata issues, 204 388 259 source bytes
и exit 2. Все 7 829 source files и 495 original native hashes не изменены.
Runtime graph — прежние 206 names / 214 versions. Release analyzer — 50 582 664
bytes, checker — 46 593 936 bytes.
[Машинный отчёт](analyzer-common-module-completion-results.json) сохраняет gates и замеры.

Подготовка ограничена прямым namespace. Global unqualified exports, transitive
inference через переменные, остальные тела, SDBL и полная семантика не принимаются
этим компонентом. Partial copies не подтверждают полный RSS/global memory budget;
проверены explicit watcher events, не OS watcher. Платформа 1С не запускалась.
EF-18 остаётся `IN_PROGRESS`; следующие этапы не начаты.
