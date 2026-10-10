# Диагностика текущих моделей YAML

Поведение этого компонента принято на `4581e17`. Его результаты ниже относятся
к этому состоянию; следующая [проверка IDE/CI](analyzer-checker-metadata.md)
подключает тот же metadata query и presentation к самостоятельному checker.

Стандартный [`textDocument/diagnostic`](https://raw.githubusercontent.com/microsoft/language-server-protocol/gh-pages/_specifications/lsp/3.18/language/pullDiagnostics.md)
теперь принимает объявленный YAML выбранного проекта. URI выбирает профиль:
модуль — прежний `syntax`, владелец метаданных — `metadata`. Прежний opaque
provider identifier `eska-syntax` сохранён и принимается для обоих документов;
новых команд или aliases нет. Protocol 0.1 и IDE protocol 1.x сохранены.

`experimental.eska.analysis.profiles` содержит `syntax` и `metadata`,
`metadataEnabled=true`. `diagnosticProvider` объявляется и при отключённом
ParseError, некорректном или неподдержанном `bsl-analyzer.toml`: метаданные
не зависят от BSL rules. Явный BSL pull сохраняет прежние ошибки settings/profile.
Fingerprint BSL не менялся; в metadata report `config_sha256=null`.

## Общая модель и текущий ввод

`AnalysisSession::metadata_diagnostics()` — операция общего core без транспорта,
локализованных строк, I/O и повторного parse. Она возвращает typed причины,
captured byte/UTF-16 ranges, native descriptor path, hash, origin и revision.
SourceReader сохраняет дополнительные границы recoverable parse issues
в том же небольшом coordinate map. Текст YAML для конвертации не удерживается.

Сервер lazy-читает pending owner один раз; отдельно повторяется transient
cancelled saved read. Открытый редактор и остальные failed inputs не заменяются
диском. Snapshot/identifier/URI проверяются до I/O; один прежний timer/token
охватывает чтение, проверки и выдачу. Zero timeout не загружает owner, возвращает
`-32802` с `retriggerRequest=false`; отмена возвращает `-32800`.

В successful full report:

| Поле | Содержание |
| --- | --- |
| `items[].range` | Точный текущий UTF-16 диапазон, end-exclusive |
| `code` | `analysis_adapter_invalid`, `analysis_metadata_parse_issue` или `analysis_metadata_kind_unsupported` |
| `severity` | 1 для некорректных данных; 2 для неподдержанного покрытия |
| `message` | Объяснение RU/EN из Fluent, отдельное от machine fields |
| `data` | `start_byte`, `end_byte`, `native_path`, структурированный `issue` |
| `eska` | snapshot, source/hash/origin, profile `metadata`, coverage и resources |

Adapter cause использует ту же [схему 14 причин](analyzer-adapter-issues.md), что
`model_issue.issue` дерева. Unsupported ownership/type/type field/standard attribute/
property value — warning ограничения модели; остальные причины — error.
Parse cause имеет `{kind: parse_issue, cause: ParseIssue}` с существующей схемой
parser metadata DTO. Unsupported root/kind/value — warning, invalid object и
duplicate identity — error. Unsupported metadata kind содержит
`{kind: unsupported_kind, metadata_kind}` и указывает на настоящий root span.
Неполный main adapter не дублирует уже показанный parse issue. Вложенные descriptor
issues сохраняют свой `native_path`, хотя physical URI и координаты относятся
к YAML владельца; готовая модель main object при этом может остаться доступной.

Coverage отдельно показывает `metadata_document_complete`,
`metadata_model_complete`, `metadata_descriptors_complete` и покрытие проекта.
`semantic_complete=false` сохраняется. Пустые items не означают полный анализ
форм, ресурсов, query text или BSL. `previousResultId` принимается, но пока всегда
возвращается full report, без unchanged/incremental/progress результата.

Если YAML не разобран целиком, файл отсутствует или отказала quota, ответ —
`metadata_input_unavailable` с текущим state/hash/snapshot/resources. Локальная
ошибка YAML grammar/схемы пока не получает отдельный LSP span: failed input
не хранит coordinate map. Координаты не угадываются; старые findings не выдаются
как результат текущей проверки. Исправленный failed saved input требует refresh
или принятого события файла, редакторский — новой full-sync версии.

## Проверки

73 unit/file, 73 analyzer process и девять checker process tests прошли, всего
155; minimal library — 58. Четыре core-теста проверяют adapter/editor/cancellation,
main/nested parse spans, отсутствие duplicate findings и unsupported root location.
Новый retry-тест различает cancelled saved/editor. Четыре process-теста RU/EN
проверяют lifecycle, ranges/hash, стандартный pull, независимость settings,
stale/identifier/zero deadline и quota. Совпадение кодов, byte ranges и сообщений
BSL с общим batch checker сохранено прежним тестом.

Все 19 tool gates и 15 root gates прошли: fmt/check, строгие Clippy/Rustdoc,
тесты четырёх профилей и release builds. Root — 893 passed / 14 прежних ignored;
minimal 165/3, project-cli 181/3, build-cli 200/3. В первой попытке прежний
`failed_cargo_query_uses_actual_root_without_installer_fallback` получил
`ExecutableFileBusy`; повторный полный запуск прошёл без изменения этого теста.
Ключи двух локалей совпадают, добавлены 20 объяснений.

На C1/C2/C3 в RU/EN пройдены 12 stdio-сценариев с disabled/invalid BSL settings,
всего 144 diagnostic pulls. Реальное свойство Server проверено после двух
редакторских сдвигов, удаления saved файла, коррекции, invalid/close/refresh.
Одна настоящая роль каждого корпуса даёт явный warning ограниченного покрытия;
оба `cfg:AnyRef` C1 дают warning с точным independently resolved QName.
Machine результаты locale-equal, human messages локализованы.

Bytes 6/4/4 выбранных оригиналов и копий сохранены; временные проекты удалены.
Состав полный, остальные payload намеренно отсутствовали. Native XML не был
входом сервера, PATH пустой, normal exit не ждал stdin EOF. Peak RSS —
47200–197772 KiB; время сценария — 0,27–0,88 s, ограниченный локальный замер.
Граф прежний: 206 package names / 214 resolved versions; release server —
49125504 bytes. [Машинный отчёт](analyzer-metadata-diagnostics-results.json)
содержит обезличенные результаты и gates.

Kernel patches, crate version, platform build и publication не менялись.
1С, настоящий VS Code client и OS watcher здесь не проверялись. Полная семантика,
остальные модели, все IDE edits, общий heap/RSS budget и Explorer ещё в работе.
EF-18 — `IN_PROGRESS`.
