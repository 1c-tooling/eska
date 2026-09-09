# eska development tracker

Этот каталог — рабочая декомпозиция продуктовой спецификации
[`eska-roadmap.md`](../eska-roadmap.md). Исходный документ отвечает на вопрос
«что строим», а этот трекер — «что делать следующим и что уже готово».

## Текущее состояние

Стадия проекта: **локальный CLI MVP и workspace готовы**. Текущий фокус —
диагностика, надёжность существующих команд и работа с большими выгрузками.

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
- `NEXT`: `T35` — автоматическое сохранение и восстановление незакоммиченного
  состояния при переключении задач;
- `PLANNED`: `T55` — переносимый стенд реализован, завершение ожидает запусков
  на Windows/macOS и приёмки с настоящей платформой 1С;
- `NEEDS-SPEC`: `T53` — завершение задачи после squash/rebase;
- `PLANNED`: `T37` — `sync` / `continue` / `abort` после T35;
- `PLANNED`: `T26` — `eska fmt`, выполняется последней в текущей очереди;
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

Первая версия `switch` работает только с чистой рабочей копией и предлагает
сначала выполнить `save`. По подтверждённому пользовательскому сценарию T35
теперь следующая: переключение должно сохранять незакоммиченное состояние одной
задачи и восстанавливать состояние другой без обязательного commit.

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

Порядок скорректирован по анализу кода и CLI от 2026-09-08. Завершённые этапы
сохраняют `DONE`; найденные ограничения оформлены как отдельные расширения.
Таблица содержит только ещё не завершённые задачи, доступные без уточнения T53.

| Порядок | ID | Результат |
|---|---|---|
| 1 | T35 | Сохранение состояния задачи и автоматическое восстановление при `switch` |
| 2 | T37 | Синхронизация задачи и штатные продолжение/отмена при конфликтах |
| 3 | T55 | Завершение внешних Windows/macOS и реальных 1С-проверок при доступном runner |
| 4 | T26 | Форматирование; последняя задача текущей очереди |

T55 можно завершить раньше T37, если станет доступна требуемая ОС; ожидание
внешнего runner не блокирует реализацию T35 и T37 на текущей системе.

T53 нельзя начинать с ослабления проверки интеграции: сначала снимается
`NEEDS-SPEC`. Уточнение её контракта не блокирует независимые T54–T55.
`publish`, environments, `apply/run` и остальные плановые этапы остаются в backlog;
T23 test backend и T39 locking сохраняют `DEFERRED`.

Детальные контракты улучшений: [T49–T55](12-current-functionality.md).
Этот план разрешает только отдельно запрошенную задачу, не автоматический запуск
всей очереди.

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
| T35 | NEXT | `shelve` / `unshelve` / `shelves` и автоматическое восстановление при `switch` | [05-safe-vcs.md](05-safe-vcs.md) |
| T36 | PLANNED | `eska restore` | [05-safe-vcs.md](05-safe-vcs.md) |
| T37 | PLANNED | `eska sync` / `continue` / `abort` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T38 | PLANNED | `eska publish` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T39 | DEFERRED | Locking объектов | [06-locking-and-xml.md](06-locking-and-xml.md) |
| T40 | DONE | `eska finish` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T41 | PLANNED | VS Code extension | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
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

Отложенные и пока недостаточно определённые возможности перечислены в
[99-deferred.md](99-deferred.md). Общие правила для каждой задачи находятся в
[working-agreement.md](working-agreement.md).

## Как обновлять трекер

1. В начале работы пометить одну задачу `NEXT` как `IN-PROGRESS` в её файле и таблице.
2. Не расширять scope за границы задачи; новые идеи записывать в `99-deferred.md`.
3. После реализации выполнить общий Definition of Done.
4. Пометить задачу `DONE`, записать принятые решения и сделать следующую
   разблокированную задачу `NEXT`.
