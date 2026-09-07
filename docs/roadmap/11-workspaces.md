# Project workspaces и монорепозиторий

Этот этап добавляет один Git-репозиторий с несколькими независимо
собираемыми и версионируемыми проектами 1С. Первый целевой
сценарий — монорепозиторий внешних отчётов и обработок.

Не смешивать project workspace с isolated Git worktree для отдельной
задачи: первый описывает состав монорепозитория, второй — способ
изолировать рабочую копию.

## Целевая модель

`Project` остаётся одной сборочной единицей с одним `root`, `source`,
`ProjectType` и корневым Designer XML descriptor. Не превращать
`Project.source` в коллекцию.

```text
Git repository
└── Workspace
    ├── Project: sales-report
    └── Project: import-orders
```

Новые locale-independent сущности:

- `Workspace`: root manifest, список members, общие build defaults и
  repository workflow;
- `WorkspaceMember`: уникальное имя, member root и проверенный
  `Project`;
- discovery context: standalone project, workspace root или workspace member;
- project selection: current member, явно выбранные members или весь
  workspace.

Workspace root, member root и Git worktree — разные понятия. Member и
workspace должны находиться внутри repository, но workspace не обязан
совпадать с Git root.

## Структура и config

```text
external-tools/
├── eska.toml
├── src/
│   ├── sales-report/
│   │   ├── eska.toml
│   │   ├── SalesReport.xml
│   │   └── SalesReport/
│   └── import-orders/
│       ├── eska.toml
│       ├── ImportOrders.xml
│       └── ImportOrders/
└── build/
    ├── sales-report.erf
    └── import-orders.epf
```

Root manifest:

```toml
[workspace]
members = [
    "src/sales-report",
    "src/import-orders",
]

[build]
platform_version = "8.3.27.2325"
artifacts_directory = "build"

[vcs.workflow]
preset = "trunk"
```

Member manifest:

```toml
[project]
name = "sales-report"
type = "report"
source = "."
```

Правила первой версии:

- root manifest содержит `[workspace]` без `[project]`;
- member manifest содержит `[project]` без `[workspace]` и
  `[vcs]`;
- `members` — явный список относительных каталогов; glob и
  автообнаружение не входят в первую версию;
- member paths не пустые, не абсолютные, не содержат `..` и
  после canonicalization остаются внутри workspace;
- пересекающиеся, вложенные и дублирующиеся members отклоняются;
- `project.name` обязателен и уникален для workspace member,
  использует portable ASCII-identifier `[a-z0-9][a-z0-9_-]*` и не
  зависит от имени объекта 1С в XML;
- старый standalone `[project]` без `name` остаётся валидным и
  использует имя project root, как сейчас;
- source разрешается относительно member root и не выходит из
  него;
- корневой `[build]` задаёт общую версию платформы и единый
  каталог artifacts;
- member `[build].platform_version` может переопределить версию;
- member `artifacts_directory` в workspace в первой версии не
  поддерживается; для одиночного проекта его текущая семантика
  сохраняется;
- общий workflow хранится только в root manifest.

Config parsing не должен немедленно подставлять defaults в отсутствующие
build fields. Raw overrides с `Option` сливаются с workspace defaults до
создания итоговых `BuildSettings`.

## Discovery и выбор member

Из member каталога discovery находит ближайший project manifest, затем
продолжает поиск вверх для необязательного workspace manifest. Member
должен быть явно указан в `workspace.members`; иначе config отклоняется
как несогласованный, а не тихо считается standalone project.

Команды, работающие с проектами, получают общие selectors:

```text
-p, --project <name>   выбрать member; флаг можно повторять
--workspace            выбрать все members
```

Без selectors:

- из standalone project выбран он;
- из member каталога выбран текущий member;
- из workspace root выбраны все members для read-only и build
  operations;
- mutating operation, не имеющая безопасной массовой семантики,
  из workspace root требует `--project`.

`--project-dir` остаётся начальным файловым каталогом для discovery и
не заменяет `--project`.

## T44 — Workspace config, model и discovery

**Статус:** `DONE` · **Зависит от:** T03, T07

Добавить strict root/member schemas, locale-independent `Workspace` и
`WorkspaceMember`, inheritance build defaults, path/name validation и discovery context.
Существующий single-project config, CLI и serialization остаются валидными
и не меняют canonical output.

T44 не меняет поведение `build`, `version`, `status`, `diff` и `save`;
он только создаёт проверенную основу для следующих задач. Запуск `eska`
без команды из workspace root валидирует root manifest и все members
без полного semantic parse XML.

**Результат:** добавлены взаимоисключающие strict schemas `[project]` и
`[workspace]`, переносимые `ProjectName`, `Workspace` и `WorkspaceMember`, а также
`discover_context`. Workspace discovery проверяет явный список members,
канонические границы и пересечения путей, уникальность имён, member-only
ограничения и наследование build/workflow. Запуск `eska` без команды использует
новый контекст; существующий `discover` и одно-проектные команды не изменены.

## T45 — Versioning workspace members

**Статус:** `DONE` · **Зависит от:** T25, T44

Целевой CLI:

```text
eska version                       # все версии из workspace root
eska version -p sales-report       # один member
eska version -p sales-report bump patch
```

Из member каталога `eska version` и `bump` без selector работают с этим
member. Из workspace root `bump` без ровно одного `--project`
отклоняется до записи. Массовый bump и `bump auto` не входят в T45.

Точечная замена XML и одиночный JSON schema v1 из T25 не меняются.
Для списка версий вводится отдельный versioned JSON document с именем,
типом, версией и project-relative descriptor каждого member.

**Результат:** `version` использует общий locale-independent selection
current/named/all. Из корня читаются все members, `-p` поддерживает один или
несколько проектов для чтения, а `--workspace` явно выбирает весь workspace из
member-каталога. Одиночный human/JSON v1 сохранён; список получил отдельный JSON
v1 с `projects[]`. `bump` до записи требует текущего member или ровно один `-p`
и меняет только выбранный descriptor.

## T46 — Build workspace members

**Статус:** `DONE` · **Зависит от:** T28, T44

Целевой CLI:

```text
eska build                         # все members из workspace root
eska build -p sales-report         # один member
eska build -p sales-report -p import-orders
eska build --workspace             # все members из member каталога
```

Перед запуском `ibcmd` для всех выбранных members строятся и
валидируются `BuildPlan`: sources, требуемые версии платформы,
artifact paths и коллизии output. Preflight-ошибка любого member блокирует
запуск всей группы.

Первая версия собирает members последовательно. После runtime-ошибки
одного member остальные независимые members продолжаются; итоговый
exit code ненулевой. Каждый артефакт публикуется атомарно по правилам
T28; общая multi-file транзакция не обещается. Parallel `--jobs` отложен до
измерения реальной сборки.

Default outputs: `<workspace>/build/<project.name>.<native-extension>`. `--output`
допустим только для ровно одного member. Одиночный JSON schema v1
из T28 сохраняется; aggregate build получает отдельный versioned
JSON document с результатом каждого member и стабильными error codes.

**Результат:** `build` использует общий current/named/all selection и формирует
все планы до запуска сборочных стадий. Read-only preflight проверяет исходники,
descriptor, output scope и коллизии; требуемые версии `ibcmd` разрешаются для
всей группы заранее. Members выполняются последовательно, runtime-ошибка не
останавливает следующие сборки, а прерывание пользователя оставляет их
не запущенными. Default artifacts публикуются атомарно в общем каталоге под
именем `project.name`; одиночный JSON v1 сохранён, aggregate получил отдельный
JSON v1 со стабильными status/error codes.

## T47 — Workspace-aware `status`, `diff` и `save`

**Статус:** `DONE` · **Зависит от:** T12, T14, T15, T44

- из member каталога `status`, `diff` и `save` сохраняют project-scoped
  поведение;
- из workspace root `status` и `diff` группируют изменения по
  members и отдельно показывают workspace files;
- `save -p <name>` сохраняет только выбранный member;
- `save` из workspace root сохраняет все изменения внутри workspace,
  но не затрагивает соседние каталоги более крупного repository;
- `start`, `switch`, `finish` и `history` остаются repository-wide;
- dirty preflight для branch switching по-прежнему проверяет весь Git worktree.

Workspace human output локализуется; aggregate JSON получает новые
versioned documents и не меняет существующие single-project schemas.

**Результат:** `status` и `diff` используют current/named/all selection,
сохраняют одиночные схемы и из одного Git snapshot группируют members отдельно
от workspace-owned files. Aggregate поддерживает human, JSON, raw, revision и
semantic diff. `save -p` создаёт commit только для одного member; `save` из корня
и `save --workspace` выполняют одну rollback-защищённую Git-транзакцию по всему
workspace, не включая и не снимая staging с соседних путей repository.

## T48 — `new` и `init` для workspace members

**Статус:** `DONE` · **Зависит от:** T04, T06, T44

Добавить member без создания вложенного Git repository и без member-level
workflow. Операция до записи проверяет destination, descriptor, имя,
коллизии и root manifest. При ошибке восстанавливается исходный
root manifest и удаляются только созданные текущим вызовом пути.

**Результат:** `eska new <name>` из workspace создаёт `src/<name>` и запрашивает
только тип проекта; запуск из member создаёт соседний member. `eska init` из
скопированного каталога определяет тип по Designer XML и имя по каталогу, а
`--name` позволяет задать переносимое имя явно. Обе команды автоматически
добавляют относительный путь в `workspace.members`, сохраняют комментарии и
форматирование root TOML, не создают вложенные Git metadata и member-level
workflow. Preflight отклоняет коллизии, повторные имена и вложенные members;
compare-before-replace защищает от конкурентного изменения manifest. После
поздней ошибки manifest восстанавливается byte-for-byte, а rollback удаляет
только созданные текущим запуском пути.

## Общие критерии готовности

- все path/name/config errors структурированы в core и локализованы в CLI;
- legacy single-project fixtures и JSON schemas проходят без изменений;
- выбор current/explicit/all покрыт на RU/EN и в JSON;
- version bump меняет байты только одного descriptor;
- build preflight не запускает частичную группу при ошибке config,
  а runtime-результат каждого member отражён в aggregate output;
- символические ссылки, дублирующиеся/вложенные members, output
  collisions и изменения вне workspace покрыты тестами;
- build и CLI integration tests используют только уникальные каталоги в
  `ESKA_TEST_ROOT`;
- каждая задача T44–T48 отдельно выполняет общий Definition of Done.

## Не входит в этап

- несколько source roots внутри одного `Project`;
- автоматическое включение всех каталогов `src/*`;
- nested workspaces и root project в virtual workspace;
- массовый version bump;
- параллельная сборка;
- единая транзакция публикации всех artifacts;
- EDT или другие source formats;
- dependency graph между members.
