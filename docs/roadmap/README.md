# eska development tracker

Этот каталог — рабочая декомпозиция продуктовой спецификации
[`eska-roadmap.md`](../eska-roadmap.md). Исходный документ отвечает на вопрос
«что строим», а этот трекер — «что делать следующим и что уже готово».

## Текущее состояние

Стадия проекта: **локальный CLI MVP и workspace готовы**. По решению пользователя
от 2026-09-17 текущий фокус ветки `feat/ide` — подготовка `eska` и первой версии
VS Code extension для навигации по Designer XML.

- `DONE`: чистый минимальный Rust CLI;
- `DONE`: локализация `ru-RU` / `en-US`, включая `--help`;
- `DONE`: `T01` — минимальная доменная модель проекта;
- `DONE`: `T02` — схема и загрузка `eska.toml`;
- `DONE`: `T03` — project discovery, проверка исходников и ошибки CLI;
- `DONE`: `T05` — встроенные шаблоны для четырёх типов проектов;
- `DONE`: `T04` — `eska new`, клавиатурный TUI, безопасное создание и минимальный Git init;
- `DONE`: `T06` — `eska init`, подключение существующих исходников;
- `DONE`: `T07` — repository layer: HEAD, refs, история, status и changed paths;
- `DONE`: `T08` — workflow policy model, preset overrides и декларативный план;
- `DONE`: `T09` — Trunk preset;
- `DONE`: `T10` — Git Flow preset;
- `DONE`: `T11` — GitHub Flow preset;
- `DONE`: `T12` — `eska status`;
- `DONE`: `T13` — `eska start`;
- `DONE`: `T14` — object-aware human-представление `eska diff`;
- `DONE`: `T15` — `eska save`;
- `DONE`: `T16` — `eska clone` через `gix`;
- `DONE`: `T17` — реализованные VCS-операции переведены на gix-first слой;
- `DONE`: `T18` — локальная история commit/task без fetch и изменения repository;
- `DONE`: `T19` — Designer XML logical object model;
- `DONE`: `T20` — reusable semantic `ChangeSet`;
- `DONE`: `T21` — semantic diff;
- `DONE`: `T22` — генератор commit message;
- `DONE`: `T34` — безопасное переключение между существующими задачами;
- `DONE`: `T40` — локальное завершение задачи;
- `DONE`: `T28` — сборка `.cf`, `.cfe`, `.epf`, `.erf` через
  настраиваемый `ibcmd`, глобальный machine config и одноразовый выбор
  установленной платформы;
- `DONE`: `T42` — спецификация и feasibility-прототип patch-extension `.cfe`;
- `DONE`: `T43` — production-команда `eska patch` для консервативного набора
  изменений методов общих модулей;
- `DONE`: `T25` — чтение и точечное изменение версии проекта 1С;
- `DONE`: `T44` — workspace config, model и discovery;
- `DONE`: `T45` — версионирование workspace members;
- `DONE`: `T46` — сборка workspace members;
- `DONE`: `T47` — VCS UX workspace members;
- `DONE`: `T48` — onboarding workspace members;
- `DONE`: `T29` — `eska doctor`;
- `DONE`: `T49` — стабильные JSON-ошибки и обратимые пути;
- `DONE`: `T50` — анализ только затронутых объектов и явный fallback;
- `DONE`: `T51` — предварительный просмотр `save`;
- `DONE`: `T52` — предварительный просмотр `build`;
- `DONE`: `T54` — паспорт собранного артефакта;
- `DONE`: `T58` — настройка основной production-ветки Git Flow без изменения
  базы feature-веток;
- `DONE`: `T59` — читаемый semantic-вывод `eska diff` с координатами методов,
  устранением дублей и порядком метаданных Конфигуратора;
- `DONE`: `T35` — автоматическое сохранение и восстановление незакоммиченного
  состояния при переключении задач;
- `PLANNED`: `T56` — автоматическая версия релиза 1С и подготовка Release PR/MR;
  сохранена в backlog после переключения приоритета на IDE;
- `PLANNED`: `T57` — обновление установленного бинарника командой `eska update`
  после T56;
- `PLANNED`: `T55` — переносимый стенд реализован, завершение ожидает запусков
  на Windows/macOS и приёмки с настоящей платформой 1С;
- `NEEDS-SPEC`: `T53` — завершение задачи после squash/rebase;
- `PLANNED`: `T37` — `sync` / `continue` / `abort` после T57;
- `PLANNED`: `T26` — `eska fmt`, последняя в очереди последующих CLI-задач;
- `DONE`: `T60` — модель метаданных и логическая identity;
- `DONE`: `T61` — Designer resolver и открытие через обязательный manifest;
- `DONE`: `T62` — fixtures, матрица типов и большой стенд;
- `DONE`: `T63` — выбор XML parsing по измерениям;
- `DONE`: `T64` — разбор Designer XML в модель метаданных;
- `DONE`: `T65` — схемы дерева Конфигуратора;
- `DONE`: `T66` — read-only metadata workspace API;
- `DONE`: `T67` — lazy cache и инкрементальное обновление;
- `DONE`: `T75` — индекс и API поиска;
- `DONE`: `T68` — дисковый кеш и замеры производительности;
- `DONE`: `T69` — спецификация постоянного IDE protocol;
- `DONE`: `T70` — постоянный IDE процесс;
- `NEXT`: `T76` — основа расширения в отдельном репозитории пользователя;
- `PLANNED`: `T41`, декомпозированная в `T76–T81`, — расширение VS Code с деревом,
  поиском, фильтром, иконками и устанавливаемым VSIX;
- `PLANNED`: `T71–T73` — отдельный последующий этап редактирования существующих
  метаданных после первой поставки; `T74` structural editing — `DEFERRED`;
- `T23` test backend и `T39` locking отложены до проверки этого MVP в реальной
  работе.

Практический MVP замыкает основной пользовательский цикл без обязательных
test backend, locking и публикации через `eska`:

```text
start -> status/diff -> save -> switch/return -> finish
                                      |
                                    build .cf
```

| Пользовательская потребность | Текущее покрытие |
|---|---|
| Создать и начать задачу | `eska start <task>` — `DONE` |
| Увидеть изменённые файлы и объекты | `eska status`, `eska diff` — `DONE` |
| Создать commit | `eska save` — `DONE` |
| Переключиться и позднее вернуться | `eska switch` — `DONE` |
| Завершить задачу | `eska finish` — `DONE` |
| Собрать полный нативный артефакт | `eska build` → `.cf/.cfe/.epf/.erf` — `DONE` |
| Собрать patch-extension из delta | `eska patch` → `.cfe` — `DONE` для ограниченного набора методов |

T42 подтвердил узкий сценарий замены метода общего модуля на 8.3.27.2325. T43
зафиксировал консервативный allowlist и добавил отдельную команду с обязательной
проверкой BSL и применимости пакетным Конфигуратором. Подробности и ограничения —
[в результате T42/T43](t42-patch-extension.md).

T35 добавила автоматические полки при `switch` и явные команды
`shelve` / `unshelve` / `shelves`. T58 завершила настройку production-ветки Git Flow с
сохранением `develop` базой обычных feature-веток. T59 сделала semantic-вывод
однозначным, подавила противоречивые события и добавила координаты методов.
Следующая задача — T76: основа расширения VS Code в отдельном репозитории. T56 остаётся в backlog.

Структурный рефакторинг после T06: команды сгруппированы в `src/cli/commands/`,
операции проекта — в `src/project/`, TOML-схема отделена от проверенных настроек,
TUI разделён на обработку клавиш, отрисовку и управление терминалом. CLI и config
сохранены; T07 не реализуется в рамках рефакторинга. Актуальные пути и границы:
[карта исходного кода](../architecture.md).

Перед началом любой задачи нужно сверять статус с фактическим кодом и менять его
на `DONE` только после выполнения критериев готовности.

## Статусы

- `DONE` — реализовано и проверено в текущем репозитории;
- `NEXT` — следующая задача, которую можно брать без дополнительных решений;
- `PLANNED` — задача определена, но ждёт зависимостей;
- `DEFERRED` — намеренно отложенная возможность;
- `NEEDS-SPEC` — идея зафиксирована, но перед реализацией нужна отдельная спецификация.

## Ближайший порядок выполнения

Порядок изменён пользователем 2026-09-17: ближайший результат — устанавливаемое
расширение VS Code. Завершённые этапы сохраняют `DONE`. Каждая задача имеет
собственные критерии; наличие в очереди не запускает её реализацию автоматически.

| Порядок | ID | Результат |
|---|---|---|
| 1 | T60 | Модель метаданных и стабильная identity — `DONE` |
| 2 | T61 | Manifest, Designer resolver и переход к XML/BSL — `DONE` |
| 3 | T62 | Fixtures, матрица типов и большой стенд — `DONE` |
| 4 | T63 | Выбор XML parser по замерам и проект бюджетов — `DONE` |
| 5 | T64 | Разбор Designer XML по запросу — `DONE` |
| 6 | T65 | Схемы дерева, первая группа модулей и пустые коллекции — `DONE` |
| 7 | T66 | Read-only metadata API — `DONE` |
| 8 | T67 | Lazy cache и обновление затронутых объектов — `DONE` |
| 9 | T75 | Поисковый индекс и core search API — `DONE` |
| 10 | T68 | Дисковый кеш и измеренная производительность backend — `DONE` |
| 11 | T69 | Контракт постоянного IDE protocol — `DONE` |
| 12 | T70 | Процесс `eska ide --stdio` — `DONE` |
| 13 | T76 | Основа расширения и подключение к `eska` — `NEXT` |
| 14 | T77 | Дерево, модули и открытие исходников |
| 15 | T78 | Поиск и переход к результату |
| 16 | T79 | Фильтр пустых групп и исходная настройка |
| 17 | T80 | Иконки метаданных |
| 18 | T81 | Приёмка скорости/UI и VSIX; закрытие T41 |

Для всех четырёх типов проекта обязателен `eska.toml`: его проверенный тип
определяет схему дерева. `.bin` без BSL скрываются. Большой стенд —
`<repo>/../eska-playground/big_configuration_temp`; **сборку этого проекта
не запускать**, использовать только чтение метаданных и замеры. Изменения XML
для тестов выполняются на небольших собственных fixtures. Подробные границы и
неподтверждённые UX-предложения — в [плане расширения](14-vscode-extension.md).

T56 → T57 → T37 → T55 → T26 сохраняются как очередь последующих CLI-задач;
возврат к ней определяется отдельно после первой IDE-поставки. T55 можно
завершить раньше при доступном runner; это не повод собирать большой IDE-стенд.
T71–T73 также требуют отдельного последующего этапа, T74 остаётся отложенной.

T53 нельзя начинать с ослабления проверки интеграции: сначала снимается
`NEEDS-SPEC`. Уточнение её контракта не блокирует независимые T54–T55.
`publish`, environments, `apply/run` и остальные плановые этапы остаются в backlog;
T23 test backend и T39 locking сохраняют `DEFERRED`.

Детальные контракты улучшений: [T49–T55](12-current-functionality.md).
Контракт автоматической подготовки версии релиза: [T56](10-delivery-and-integrations.md).
Контракт обновления установленного CLI: [T57](10-delivery-and-integrations.md).
Этот план разрешает только отдельно запрошенную задачу, не автоматический запуск
всей очереди.

## Последовательность metadata / IDE

Детальные контракты: [метаданные Designer XML и IDE protocol](13-metadata-and-ide.md).
Это развитие T19/T50 с переиспользованием существующих `ObjectId`, discovery и
mapping, а не повторная реализация завершённых задач. Задачи клиента описаны
в [T76–T81](14-vscode-extension.md), они завершают текущую первую поставку.

| Milestone | Задачи | Результат |
|---|---|---|
| Metadata V0.1 — Core model | T60–T62 | Модель, logical identity, resolver, fixtures |
| Metadata V0.2 — Designer XML parser | T63–T64 | Стратегия parsing и parser четырёх типов проектов |
| Metadata V0.3 — Configurator tree | T65–T66 | Schema, golden tests и read-only API |
| Metadata V0.4 — Incremental workspace | T67, T75, T68 | Lazy cache, поиск, обновления и benchmarks |
| IDE V0.1 — Read-only protocol | T69–T70 | Спецификация и постоянный `eska ide --stdio` |
| VS Code V0.1 — Metadata explorer | T41: T76–T81 | Дерево, модули, поиск, фильтр, иконки и VSIX |
| Metadata V0.5 — Existing object editing | T71–T73 | Safe XML patching и изменение существующих элементов |
| Metadata V0.6 — Structural editing | T74 | Отложенные add/remove/rename и согласованная запись файлов |

Metadata/IDE API первой поставки полностью read-only. T41 — сводная задача
клиента, закрываемая результатом T76–T81 после T70. EDT, MCP, AI, собственный
BSL LSP и визуальный редактор форм в это направление не входят.

## Реестр задач

Таблица сохраняет порядок ID; фактическая очередь приведена выше.

| ID | Статус | Задача | Подробности |
|---|---|---|---|
| B00 | DONE | Clean baseline | [00-foundation.md](00-foundation.md) |
| B01 | DONE | Локализация CLI | [00-foundation.md](00-foundation.md) |
| T01 | DONE | Доменная модель `Project` | [01-project-foundation.md](01-project-foundation.md) |
| T02 | DONE | Схема и загрузка `eska.toml` | [01-project-foundation.md](01-project-foundation.md) |
| T03 | DONE | Project discovery и validation | [01-project-foundation.md](01-project-foundation.md) |
| T04 | DONE | `eska new` | [02-project-creation.md](02-project-creation.md) |
| T05 | DONE | Built-in templates | [02-project-creation.md](02-project-creation.md) |
| T06 | DONE | `eska init` | [02-project-creation.md](02-project-creation.md) |
| T07 | DONE | Repository layer (`gix`; документированный Git capability fallback) | [03-repository-workflow.md](03-repository-workflow.md) |
| T08 | DONE | Workflow policy model | [03-repository-workflow.md](03-repository-workflow.md) |
| T09 | DONE | Trunk preset | [03-repository-workflow.md](03-repository-workflow.md) |
| T10 | DONE | Git Flow preset | [03-repository-workflow.md](03-repository-workflow.md) |
| T11 | DONE | GitHub Flow preset | [03-repository-workflow.md](03-repository-workflow.md) |
| T12 | DONE | `eska status` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T13 | DONE | `eska start` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T14 | DONE | `eska diff` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T15 | DONE | `eska save` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T16 | DONE | `eska clone` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T17 | DONE | Gix-first миграция реализованных VCS-операций | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T18 | DONE | `eska history` | [05-safe-vcs.md](05-safe-vcs.md) |
| T19 | DONE | Designer XML object model | [06-locking-and-xml.md](06-locking-and-xml.md) |
| T20 | DONE | Semantic `ChangeSet` | [07-semantic-changes.md](07-semantic-changes.md) |
| T21 | DONE | Semantic diff | [07-semantic-changes.md](07-semantic-changes.md) |
| T22 | DONE | Генератор commit message | [07-semantic-changes.md](07-semantic-changes.md) |
| T23 | DEFERRED | Спецификация test backend | [08-quality.md](08-quality.md) |
| T24 | PLANNED | `affected` analysis | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T25 | DONE | Versioning проекта 1С | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T26 | PLANNED | `eska fmt` (последняя задача текущей очереди) | [08-quality.md](08-quality.md) |
| T27 | PLANNED | `eska check` | [08-quality.md](08-quality.md) |
| T28 | DONE | Build через `ibcmd` | [09-build-and-runtime.md](09-build-and-runtime.md) |
| T29 | DONE | `eska doctor` | [09-build-and-runtime.md](09-build-and-runtime.md) |
| T30 | PLANNED | Development environments | [09-build-and-runtime.md](09-build-and-runtime.md) |
| T31 | PLANNED | `apply` / `run` | [09-build-and-runtime.md](09-build-and-runtime.md) |
| T32 | PLANNED | Release pipeline | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T33 | PLANNED | CI integration | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T34 | DONE | `eska switch` | [05-safe-vcs.md](05-safe-vcs.md) |
| T35 | DONE | `shelve` / `unshelve` / `shelves` и автоматическое восстановление при `switch` | [05-safe-vcs.md](05-safe-vcs.md) |
| T36 | PLANNED | `eska restore` | [05-safe-vcs.md](05-safe-vcs.md) |
| T37 | PLANNED | `eska sync` / `continue` / `abort` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T38 | PLANNED | `eska publish` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T39 | DEFERRED | Locking объектов | [06-locking-and-xml.md](06-locking-and-xml.md) |
| T40 | DONE | `eska finish` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T41 | PLANNED | eska: 1C Explorer — сводная задача T76–T81 | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T42 | DONE | Спецификация и прототип patch-extension `.cfe` из разницы веток | [t42-patch-extension.md](t42-patch-extension.md) |
| T43 | DONE | Генерация patch-extension для ограниченного набора методов | [t42-patch-extension.md](t42-patch-extension.md) |
| T44 | DONE | Workspace config, model и discovery | [11-workspaces.md](11-workspaces.md) |
| T45 | DONE | Versioning workspace members | [11-workspaces.md](11-workspaces.md) |
| T46 | DONE | Build workspace members | [11-workspaces.md](11-workspaces.md) |
| T47 | DONE | Workspace-aware `status`, `diff` и `save` | [11-workspaces.md](11-workspaces.md) |
| T48 | DONE | `new` и `init` для workspace members | [11-workspaces.md](11-workspaces.md) |
| T49 | DONE | Стабильные JSON-ошибки и обратимые пути | [12-current-functionality.md](12-current-functionality.md) |
| T50 | DONE | Анализ затронутых объектов и явный fallback | [12-current-functionality.md](12-current-functionality.md) |
| T51 | DONE | Предварительный просмотр `save` | [12-current-functionality.md](12-current-functionality.md) |
| T52 | DONE | Предварительный просмотр `build` | [12-current-functionality.md](12-current-functionality.md) |
| T53 | NEEDS-SPEC | Завершение задачи после squash/rebase | [12-current-functionality.md](12-current-functionality.md) |
| T54 | DONE | Паспорт собранного артефакта | [12-current-functionality.md](12-current-functionality.md) |
| T55 | PLANNED | Переносимые проверки сборки; ожидает внешней приёмки | [12-current-functionality.md](12-current-functionality.md) |
| T56 | PLANNED | Автоматическая подготовка версии релиза 1С | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T57 | PLANNED | Обновление установленного `eska` | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T58 | DONE | Настраиваемая основная production-ветка Git Flow | [03-repository-workflow.md](03-repository-workflow.md) |
| T59 | DONE | Читаемый semantic-вывод `eska diff` с координатами методов | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T60 | DONE | Модель метаданных и логическая identity | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T61 | DONE | Designer path resolver | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T62 | DONE | Набор Designer XML fixtures | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T63 | DONE | Выбор стратегии XML parsing | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T64 | DONE | Разбор Designer XML в модель метаданных | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T65 | DONE | ConfiguratorSchema и ConfiguratorTree | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T66 | DONE | Read-only metadata workspace API | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T67 | DONE | Lazy index, cache и инкрементальное обновление | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T68 | DONE | Производительность metadata workspace | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T69 | DONE | Спецификация постоянного IDE protocol | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T70 | DONE | Постоянный read-only IDE процесс | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T71 | PLANNED | Архитектура safe XML patching | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T72 | PLANNED | Точечный XML patch и безопасная запись | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T73 | PLANNED | Редактирование существующих элементов | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T74 | DEFERRED | Структурные изменения метаданных | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T75 | DONE | Поиск по метаданным | [13-metadata-and-ide.md](13-metadata-and-ide.md) |
| T76 | NEXT | Основа расширения и подключение к eska | [14-vscode-extension.md](14-vscode-extension.md) |
| T77 | PLANNED | Дерево проектов, модули и открытие исходников | [14-vscode-extension.md](14-vscode-extension.md) |
| T78 | PLANNED | Поиск в расширении и переход к результату | [14-vscode-extension.md](14-vscode-extension.md) |
| T79 | PLANNED | Фильтр пустых групп и настройка исходного режима | [14-vscode-extension.md](14-vscode-extension.md) |
| T80 | PLANNED | Иконки метаданных | [14-vscode-extension.md](14-vscode-extension.md) |
| T81 | PLANNED | Производительность интерфейса, приёмка и VSIX | [14-vscode-extension.md](14-vscode-extension.md) |

Отложенные и пока недостаточно определённые возможности перечислены в
[99-deferred.md](99-deferred.md). Общие правила для каждой задачи находятся в
[working-agreement.md](working-agreement.md).

## Как обновлять трекер

1. В начале работы пометить одну задачу `NEXT` как `IN-PROGRESS` в её файле и таблице.
2. Не расширять scope за границы задачи; новые идеи записывать в `99-deferred.md`.
3. После реализации выполнить общий Definition of Done.
4. Пометить задачу `DONE`, записать принятые решения и сделать следующую
   разблокированную задачу `NEXT`.
