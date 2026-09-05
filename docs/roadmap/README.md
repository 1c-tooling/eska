# eska development tracker

Этот каталог — рабочая декомпозиция продуктовой спецификации
[`eska-roadmap.md`](../eska-roadmap.md). Исходный документ отвечает на вопрос
«что строим», а этот трекер — «что делать следующим и что уже готово».

## Текущее состояние

Стадия проекта: **semantic `ChangeSet` завершён**, далее semantic diff.

- `DONE`: чистый минимальный Rust CLI;
- `DONE`: локализация `ru-RU` / `en-US`, включая `--help`;
- `DONE`: `T01` — минимальная доменная модель проекта;
- `DONE`: `T02` — схема и загрузка `eska.toml`;
- `DONE`: `T03` — project discovery, проверка исходников и ошибки CLI;
- `DONE`: `T05` — встроенные шаблоны для четырёх типов проектов;
- `DONE`: `T04` — `eska new`, клавиатурный TUI, безопасное создание и минимальный Git init;
- `DONE`: `T06` — `eska init`, подключение существующих исходников;
- `DONE`: `T07` — repository layer: HEAD, refs, история, status и changed paths;
- `DONE`: `T08` — workflow policy model, custom overrides и декларативный план;
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
- `IN-PROGRESS`: `T21` — semantic diff.

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

## Очередь реализации

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
| T21 | IN-PROGRESS | Semantic diff | [07-semantic-changes.md](07-semantic-changes.md) |
| T22 | PLANNED | Генератор commit message | [07-semantic-changes.md](07-semantic-changes.md) |
| T23 | PLANNED | Спецификация test backend | [08-quality.md](08-quality.md) |
| T24 | PLANNED | `affected` analysis | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T25 | PLANNED | Versioning проекта 1С | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T26 | PLANNED | `eska fmt` | [08-quality.md](08-quality.md) |
| T27 | PLANNED | `eska check` | [08-quality.md](08-quality.md) |
| T28 | PLANNED | Build через `ibcmd` | [09-build-and-runtime.md](09-build-and-runtime.md) |
| T29 | PLANNED | `eska doctor` | [09-build-and-runtime.md](09-build-and-runtime.md) |
| T30 | PLANNED | Development environments | [09-build-and-runtime.md](09-build-and-runtime.md) |
| T31 | PLANNED | `apply` / `run` | [09-build-and-runtime.md](09-build-and-runtime.md) |
| T32 | PLANNED | Release pipeline | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T33 | PLANNED | CI integration | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T34 | PLANNED | `eska switch` | [05-safe-vcs.md](05-safe-vcs.md) |
| T35 | PLANNED | `shelve` / `unshelve` / `shelves` | [05-safe-vcs.md](05-safe-vcs.md) |
| T36 | PLANNED | `eska restore` | [05-safe-vcs.md](05-safe-vcs.md) |
| T37 | PLANNED | `eska sync` / `continue` / `abort` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T38 | PLANNED | `eska publish` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T39 | PLANNED | Locking объектов | [06-locking-and-xml.md](06-locking-and-xml.md) |
| T40 | PLANNED | `eska finish` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T41 | PLANNED | VS Code extension | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |

Отложенные и пока недостаточно определённые возможности перечислены в
[99-deferred.md](99-deferred.md). Общие правила для каждой задачи находятся в
[working-agreement.md](working-agreement.md).

## Как обновлять трекер

1. В начале работы пометить одну задачу `NEXT` как `IN-PROGRESS` в её файле и таблице.
2. Не расширять scope за границы задачи; новые идеи записывать в `99-deferred.md`.
3. После реализации выполнить общий Definition of Done.
4. Пометить задачу `DONE`, записать принятые решения и сделать следующую
   разблокированную задачу `NEXT`.
