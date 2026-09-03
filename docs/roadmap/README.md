# eska development tracker

Этот каталог — рабочая декомпозиция продуктовой спецификации
[`eska-roadmap.md`](../../eska-roadmap.md). Исходный документ отвечает на вопрос
«что строим», а этот трекер — «что делать следующим и что уже готово».

## Текущее состояние

Стадия проекта: **workflow policy model завершён**, далее Trunk preset.

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
- `NEXT`: `T09` — Trunk preset;
- VCS-команды и исполнение workflow policy пока не реализованы.

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
| T07 | DONE | Repository layer (`gix`; Git fallback по мере необходимости) | [03-repository-workflow.md](03-repository-workflow.md) |
| T08 | DONE | Workflow policy model | [03-repository-workflow.md](03-repository-workflow.md) |
| T09 | NEXT | Trunk preset | [03-repository-workflow.md](03-repository-workflow.md) |
| T10 | PLANNED | Git Flow preset | [03-repository-workflow.md](03-repository-workflow.md) |
| T11 | PLANNED | GitHub Flow preset | [03-repository-workflow.md](03-repository-workflow.md) |
| T12 | PLANNED | `eska status` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T13 | PLANNED | `eska start` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T14 | PLANNED | `eska diff` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T15 | PLANNED | `eska save` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T16 | PLANNED | `eska sync` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T17 | PLANNED | `eska publish` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T18 | PLANNED | `eska finish` | [04-core-vcs-ux.md](04-core-vcs-ux.md) |
| T19 | PLANNED | `continue` / `abort` | [05-safe-vcs.md](05-safe-vcs.md) |
| T20 | PLANNED | `switch` / shelves / history / restore | [05-safe-vcs.md](05-safe-vcs.md) |
| T21 | PLANNED | Locking объектов | [06-locking-and-xml.md](06-locking-and-xml.md) |
| T22 | PLANNED | Designer XML object model | [06-locking-and-xml.md](06-locking-and-xml.md) |
| T23 | PLANNED | Semantic `ChangeSet` | [07-semantic-changes.md](07-semantic-changes.md) |
| T24 | PLANNED | Semantic diff | [07-semantic-changes.md](07-semantic-changes.md) |
| T25 | PLANNED | Генератор commit message | [07-semantic-changes.md](07-semantic-changes.md) |
| T26 | PLANNED | `eska fmt` | [08-quality.md](08-quality.md) |
| T27 | PLANNED | `eska check` | [08-quality.md](08-quality.md) |
| T28 | PLANNED | Build через `ibcmd` | [09-build-and-runtime.md](09-build-and-runtime.md) |
| T29 | PLANNED | `eska doctor` | [09-build-and-runtime.md](09-build-and-runtime.md) |
| T30 | PLANNED | Development environments | [09-build-and-runtime.md](09-build-and-runtime.md) |
| T31 | PLANNED | `apply` / `run` | [09-build-and-runtime.md](09-build-and-runtime.md) |
| T32 | PLANNED | `affected` analysis | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T33 | PLANNED | Versioning проекта 1С | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T34 | PLANNED | Release pipeline | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T35 | PLANNED | CI integration | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |
| T36 | PLANNED | VS Code extension | [10-delivery-and-integrations.md](10-delivery-and-integrations.md) |

Отложенные и пока недостаточно определённые возможности перечислены в
[99-deferred.md](99-deferred.md). Общие правила для каждой задачи находятся в
[working-agreement.md](working-agreement.md).

## Как обновлять трекер

1. В начале работы пометить одну задачу `NEXT` как `IN-PROGRESS` в её файле и таблице.
2. Не расширять scope за границы задачи; новые идеи записывать в `99-deferred.md`.
3. После реализации выполнить общий Definition of Done.
4. Пометить задачу `DONE`, записать принятые решения и сделать следующую
   разблокированную задачу `NEXT`.
