# Использования текущего BSL-символа

`eska-analyzer` обслуживает стандартный `textDocument/references` через
`AnalysisSession::module_references()`. Поиск и классификация declaration/call/
write/read принадлежат закреплённому ядру BSL. Адаптер проверяет текущие цели
и переносит native hits в `ReferenceSearch`; собственного поиска по строкам,
второго индекса, чтения XML или загрузки соседних модулей нет.

Используется типизированный `ide::find_references_by_name()` с positional anchor
и resident inputs. Прежний native API, возвращающий только Vec, скрывает различие
между отсутствующим, неподдержанным и разрешённым символом без использований.
Dictionary fallback не применяется. Ambiguous/AnchorStale или иной источник тела
в positional query дают references_inconsistent, `-32603`.
Неподдержанный native scope даёт references_symbol_unsupported, `-32000`,
с category. Это явный отказ, а не успешный пустой поиск.

## Протокол и текущие диапазоны

После handshake `0.1` и initialized запрос принимает textDocument URI,
position в UTF-16 и обязательный context.includeDeclaration. Необязательны
workDoneToken, partialResultToken и `eska.snapshot`. Progress и streaming пока
не отправляются; provider объявлен без workDoneProgress. Типизированный decoding
предшествует общим snapshot guards. Session/revision и URI проверяются до lazy IO.
Контракт определяется [LSP references](https://raw.githubusercontent.com/microsoft/language-server-protocol/gh-pages/_specifications/lsp/3.18/language/references.md).

При корректных frozen settings объявляется referencesProvider=true.
`experimental.eska.references` сообщает scope `current-ready-bsl-inputs`,
incompleteResults `location-context-or-error`, maxFiles=200 и maxLocations=10000.
Повреждённые settings запрещают запрос и публикацию provider; отключённый
ParseError не отключает языковую операцию. Binary/missing/quota-denied caller
даёт module_unavailable. Editor bytes имеют приоритет; close восстанавливает
фактический сохранённый BSL.

LSP-позиция переводится в точный byte offset текущего caller. Вход в native
positional API использует обратное преобразование собственного LF-based UTF-16
индекса ядра. Transport отдельно учитывает LF/CRLF/CR, BOM и non-BMP; строки
протокола не подставляются в native index. Ограничение native line comments
при одиночном CR сохраняется, как в [signatureHelp](analyzer-call-signatures.md).
Скрытой нормализации bytes нет.

Каждый непустой стандартный Location содержит дополнительный `eska`:
snapshot, caller source/hash, profile `references`, position_byte,
include_declaration, coverage, resources и search. Search сохраняет files_scanned,
files_capped, hits_capped и оба предела. Kind и target source/hash/start_byte/
end_byte относятся к конкретному текущему occurrence, отдельно от caller.
Проверка declared FileId, ready text и UTF-8 границ общая с definition.
Координаты всех occurrences одного target вычисляются одним проходом;
native порядок результатов сохранён.

Не найденный anchor при готовых inputs возвращает стандартный null.
Разрешённый символ без использований при готовых и uncapped inputs возвращает
стандартный `[]`. В обоих случаях неполнота inputs даёт references_incomplete,
`-32000`, с текущим контекстом. Непустые частичные результаты допустимы:
каждый Location сохраняет coverage и flags. semantic_complete всегда false.
Результат не доказывает безопасность rename.

Семь languageMethods и восемь cancellableMethods включают references.
Общий request control сохраняет отмену `-32800`, deadline `-32000` без
retriggerRequest и остановку нулевого timeout до IO. Обычный лимит — 300 секунд.
Отмена кооперативная; задержка и потребление ресурсов не имеют жёсткой гарантии.
Новые варианты экспериментального QueryError — UnsupportedReferenceSymbol
и InconsistentReferences. API core ещё не стабилизирован.

## Ограничения поиска и проверки

200 ограничивает native candidate-file walk, но не построение name-usage index.
10000 ограничивает проекцию Locations после native collection, но не объём
временных native hits. Эти пределы не являются бюджетом CPU, heap или RSS.
Проверочный debug-набор из четырёх core-тестов с 10001 реальным чтением одного
параметра занял 117,95 секунды; это время всего набора, не отдельного запроса.
В итоговом обычном тесте предел проекции проверяется повторением одного
подтверждённого native hit. Скорость native collection этим не исправлена.

Пять новых core-тестов и девять process tests проверяют includeDeclaration,
лексическую область одноимённых параметров, пустой результат и unsupported scope,
cross-module calls, current target/hash, YAML rename, saved/editor/close,
BOM/non-BMP/EOL, required params, guards/settings, quotas и нулевой deadline.
Общие cancellation tests покрывают numeric/Unicode ID, RU/EN и продолжение
сессии. Граница 10000 и несовместимые native outcomes проверяются отдельно.

All-features: 100 unit/file, 106 analyzer и 11 checker process tests, всего 217.
Minimal — 78; checker — 80 unit/file и 11 process; analyzer — 100 unit/file
и 105 process. Все 19 tool gates прошли: fmt, check, strict Clippy, test,
Rustdoc с warnings denied в четырёх profiles и обе release-сборки.
Все 15 root gates прошли: основной набор, minimal/project-cli/build-cli и Rustdoc;
root — 894 passed / 14 прежних ignored. Diff проверен с учётом CRLF.
Новых dependencies нет. Normal graph анализатора прежний — 206 имён/214 версий;
отдельный runtime LSP, Tokio, reqwest и Git transport не добавлены.
Release analyzer — 49942712 bytes; checker — 46267864 bytes.

[Обезличенные измерения](analyzer-references-results.json) фиксируют 24 сценария
C1/C2/C3: small/medium сохранённый общий модуль, оба includeDeclaration и RU/EN.
192 pulls дали 96 подтверждённых поисков и 160 Locations, 72 явных incomplete
и 24 stale refusals. Реальное объявление использовано с проверочным caller
только в собственном editor overlay; отдельный overlay проверил область параметра.
Current byte/UTF-16 ranges, hashes и machine traces согласованы между локалями.
Загружены только текущие YAML-владельцы и запрошенный BSL; соседние inputs pending.
18 выбранных оригиналов и bytes копий сохранены; собственные проекты удалены.
Процессы завершились штатно при открытом stdin и пустом PATH.

VmHWM — 68136–247092 KiB; query time — 0,0001355–0,138889 секунды.
Это частичные ленивые сценарии, а не полный анализ конфигураций или SLA.
Платформа 1С, IDE-клиент и OS watcher в этой части не проверялись.
Rename plans, полнота моделей/семантики, общий memory budget и Explorer
остаются в работе; EF-18 — `IN_PROGRESS`.
