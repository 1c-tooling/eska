# Семантическая диагностика текущих документов

`eska-analyzer --profile semantic` выполняет настроенные native BSL rules через
ту же `AnalysisSession`, что checker, дерево метаданных и редакторские черновики.
Профиль выбирается при запуске процесса; по умолчанию сохранён `syntax`.
Настройки читаются один раз из `bsl-analyzer.toml` выбранного project member.
Смена settings требует прежнего [restart](analyzer-project-changes.md).

```bash
eska-analyzer --project-dir . --profile semantic
eska-analyzer --project-dir . --profile semantic --timeout-seconds 120
eska-analyzer --project-dir . --profile semantic --max-retained-source-bytes 268435456
```

## Общие входы

SDK-метод `load_semantic_inputs` проверяет `SessionRevision` до чтения и загружает
pending saved YAML/BSL. Временный отказ отменённого saved read допускает повторную
попытку. Ready inputs, editor overlays и editor-owned failures не перечитываются;
остальные saved failures требуют явного input change. Несохранённый отказ
остаётся текущим состоянием, прежний диск не подставляется вместо него.

Все доступные объявленные тела подготовлены до первого semantic query. Текущее
объявление соседнего экспортного метода участвует в проверке caller независимо
от порядка путей. Смена соседнего BSL или YAML меняет общую revision, в том числе
при неудачном вводе. Повторный pull использует готовые inputs. Дисковые изменения
поступают через прежние [file events](analyzer-watched-files.md).

Checker использует тот же метод подготовки; отдельного индекса, списка исходников
или адаптера для IDE нет. Syntax остаётся независимым lazy-профилем.
Недоступное запрошенное тело даёт `module_unavailable`, а не пустой full report.
Недоступные соседние inputs отражаются неполнотой и не становятся старыми телами.

## Согласование и отчёт

Semantic provider объявляет `identifier=eska-semantic` и
`interFileDependencies=true`: изменение одного модуля может изменить diagnostics
другого. Значение соответствует
[контракту pull diagnostics LSP 3.18](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/language/pullDiagnostics.md).
`workspaceDiagnostics=false`; workspace pull, related reports и client refresh
не добавлены. Syntax provider сохраняет прежний identifier и flags.
Неизвестный или несовместимый identifier отклоняется до input I/O.

Стандартный `textDocument/diagnostic` возвращает full report текущего документа.
ESKA snapshot остаётся опциональным guard. Fingerprint включает выбранный профиль
и нормализованные rules; для одинаковых semantic settings он совпадает с checker.
`experimental.eska.analysis` содержит `profiles=[semantic, metadata]`,
`semanticEnabled`, прежние syntax/config fields. Ошибка config или отключение
всех BSL rules не отключают дерево, document sync и независимую YAML-диагностику.

Code, native severity и byte endpoints сохраняются. LSP ranges вычисляются
из текущих bytes в UTF-16, включая BOM/CRLF и non-BMP символы. Semantic message
содержит code и native severity: непрозрачный native prose не имеет единого
RU/EN-контракта. SDK сохраняет native message; YAML, help и errors локализованы.

Только semantic BSL report добавляет coverage `semantic_profile_complete`.
Оно означает выполнение выбранных rules для запрошенного текущего модуля
при полных YAML/BSL inputs и моделях дескрипторов. Это не batch-проверка всех
модулей, не все свойства языка и не исполнение в 1С. `semantic_complete=false`
сохранён. `module_syntax_complete` истинно только при включённом `ParseError`.
Неполные models/inputs допускают findings с явным неполным coverage.

## Ресурсы и остановка

Совокупный предел resident BSL равен общей YAML/BSL source-byte квоте semantic
профиля, по умолчанию 256 MiB; per-document предел — 64 MiB.
Существующий timeout и cancellation охватывают подготовку inputs, вычисления
и конвертацию результата. Stop даёт прежний coded error и сохраняет current
editor bytes; отменённые saved reads можно продолжить новым запросом.

После успешного semantic pull штатный `ide::sweep_lru_deep` очищает computed
diagnostic/source memos и saved reparse hints. Editor hints, готовые resident inputs, hashes и revisions
сохраняются. Дополнительная dependency или cache не добавлены. Source reserve
и file-count LRU не являются общим heap/RSS budget.

## Проверки

Новые SDK/process tests проверяют подготовку без refresh ready inputs, stale
guard до reads, отмену и retry saved, сохранность failed overlays, межмодульные
правки/close, file invalidation, текущие YAML, disabled/invalid config, binary,
quota, timeout и настоящую отмену выполняющегося запроса. Checker/IDE parity
включает fingerprint, code/severity, byte endpoints и независимый UTF-16 расчёт.
Старые syntax tests проходят; его default JSON и settings fingerprint сохранены.

На полной отдельной копии C1 подготовлены 2 645 YAML и 1 975 BSL inputs.
В обеих локалях проверены 12 модулей с 26 сохранёнными `SelfAssign`, 12 повторных
pulls с прежним resultId, current BSL/YAML и восстановление close. Findings
совпали с предыдущим полным checker sweep по code/severity/bytes; все UTF-16
ranges независимо пересчитаны. Исходные и скопированные 7 829 файлов сохранены;
собственный проект удалён. Все модели не поддержаны: профильная полнота false.

Source reserve — 115 209 818 bytes. Cold pull — 6,79/6,35 s EN/RU;
наблюдаемый peak OS RSS — 328 792/327 620 KiB. Это единичные Linux-замеры,
не общий лимит процесса. PATH пуст; платформа и IDE-клиент не запускались.
Workspace MCP недоступен; установленный bsl-analyzer-app 0.3.0 проверил два
новых тела: одна SelfAssign и ноль ParseError. CLI сообщил degraded platform
help с built-in facts; полного help/runtime эти проверки не подтверждают.

Все 19 tool и 15 root gates прошли, включая strict Clippy и Rustdoc warnings
denied. Analysis all-features — 255 tests: 116 unit/file, 125 analyzer, 14 checker;
minimal — 94, checker — 96+14, analyzer — 116+123. Root — 895 passed/14 ignored,
feature matrix сохранена. Kernel в этом срезе не изменялся; предыдущая проверка
дала 1 104 passed/4 ignored. Normal graph — прежние 206 имён/214 версий;
manifest/lock, версии и число locale keys не менялись.

[Обезличенные результаты](analyzer-semantic-diagnostics-results.json).
EF-18 остаётся `IN_PROGRESS`: полные модели/семантика, общий memory budget,
остальные IDE edits и Explorer ещё в работе.
