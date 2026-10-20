# Навигация по исходному коду

Каталоги называются по области ответственности, файлы — по операции или
конкретной части реализации. Начинайте с обработчика команды, если меняется её
поведение для пользователя, и с модуля проекта, если меняется сама операция.

## Структура

```text
Cargo.toml                          # workspace; existing eska package remains the CLI
crates/eska-project-core/
├── Cargo.toml                      # shared library without Git/platform/BSL runtimes
├── assets/project/                 # built-in .gitattributes and .gitignore
└── src/
    ├── lib.rs                      # config, project and workflow boundaries
    ├── application/                # explicit composition, dependency graph and read-only resolver
    ├── config/                     # strict manifest schemas, serialization and validation
    ├── workflow.rs                 # presets and portable settings
    ├── workflow/policy.rs          # pure policy validation and task plans
    └── project/
        ├── mod.rs                  # shared model and metadata operations
        ├── model.rs                # Project, source format and path invariants
        ├── build.rs                # build settings and platform version only
        ├── workspace.rs            # Workspace, members and portable names
        ├── discovery.rs            # manifest discovery and source validation
        ├── selection.rs            # current/named/all project selection
        ├── create.rs, init.rs       # filesystem-only creation/attachment with rollback
        ├── onboarding.rs            # exact root-manifest enrollment
        ├── designer_xml.rs         # root descriptor classification
        ├── designer_source/        # checked source mapping and bounded reads
        ├── metadata.rs             # logical paths and descriptor-child comparisons
        ├── object_model.rs         # source ownership and affected-object discovery
        ├── metadata_model/         # typed identities, kinds and values
        ├── metadata_parser.rs      # bounded Designer XML parsing
        ├── metadata_parser/        # properties, predefined values and tests
        ├── metadata_workspace/     # tree sessions, search, support and checked edits
        ├── metadata_disk_cache.rs  # disposable source-verified metadata cache
        ├── configurator/           # typed Configurator tree schema and projection
        ├── metadata_edit/          # checked property writes and snapshots
        ├── metadata_rename/        # proven reference changes and atomic transactions
        ├── support.rs              # ParentConfigurations.bin support rules
        ├── support/tests.rs        # support-rule fixtures
        ├── version.rs              # narrow Properties/Version reads and writes
        └── templates.rs            # project scaffold plans without filesystem writes
crates/eska-analyzer/
└── src/
    ├── configuration.rs            # derived config, snapshots, ownership and leases
    ├── migration.rs                # preview/token, complete policy import and rollback
    ├── process.rs                  # published executable contract, bounded I/O and cancellation
    └── storage.rs                  # sibling staging files and OS locks
crates/eska-presentation/
├── locales/                        # common Fluent/catalog source for all tools
└── src/                            # localization, paths, locale bootstrap and core diagnostics
crates/eska-project/
└── src/
    ├── main.rs, cli.rs              # standalone project CLI
    ├── context.rs, operations.rs    # checked project commands without native runtimes
    ├── analyzer_config.rs          # config CLI; no embedded BSL engine
    ├── metadata.rs, version.rs     # shared legacy command presentation
    └── ide/                        # Metadata API framing, DTOs and server lifecycle
crates/eska-check/
└── src/                            # standalone CI CLI, stable report and shared analyzer adapter
crates/eska-build/
└── src/cli/                        # standalone build/unpack and shared legacy platform presentation
crates/eska-terminal/
└── src/                            # shared keyboard menus, confirmation and progress
crates/eska-build-runtime/
└── src/
    ├── build/                      # runner, plans, managed infobases and artifact snapshots
    ├── artifact/                   # unpack, identity and reversible source publication
    └── config/global.rs            # machine-local runner config and transactional editing
src/
├── main.rs                         # existing binary entry point
├── lib.rs                          # current CLI and operation namespaces
├── cli/
│   ├── mod.rs, args.rs             # startup, parsing, locale and top-level help
│   ├── commands/                   # command-specific human/JSON presentation
│   ├── ide.rs                      # shared project server entry point
│   ├── platform.rs                 # machine-local runner selection
│   ├── interactive.rs              # shared terminal menu reexports
│   └── diagnostics.rs              # shared native/core error presentation reexports
├── config/
│   └── mod.rs                      # shared manifests and build-runtime machine config reexports
├── project/
│   ├── mod.rs                      # reexports of shared project operations
│   ├── artifact.rs, build.rs        # reexports of the single build runtime
│   ├── patch/                      # VCS-based patch plans and native extension assembly
│   ├── create.rs, init.rs          # existing onboarding with optional Git/platform selection
│   ├── onboarding.rs               # workspace enrollment and manifest preservation
│   ├── doctor.rs                   # combined project/build/VCS readiness checks
│   ├── diff.rs, semantic/          # exact Git snapshots and semantic ChangeSet
│   └── *.rs                        # existing Git workflow operations
└── vcs/
    ├── mod.rs                      # repository execution boundary
    ├── build.rs                    # provenance and source-change inspector for build orchestration
    ├── workflow.rs                 # reexports of shared settings/policy
    ├── git.rs, repository.rs       # gix opening, refs and history
    ├── command.rs                  # isolated system Git capability fallback
    ├── network.rs                  # gix transport and documented fallback
    └── diff.rs, snapshot.rs, status.rs, shelves/
crates/eska-presentation/locales/{ru-RU,en-US}/main.ftl       # human-readable strings
assets/                            # presentation resources
tests/integration.rs               # CLI/project/VCS integration suite
tests/support/mod.rs               # exclusively owned playground directories
```

`validate.rs` — обработчик существующего запуска `eska` без подкоманды,
а не новая команда `validate`. Самостоятельный `eska-check` использует общий
контекст, полный XML-обход и внешний анализатор; его CLI находится в отдельном package. `src/vcs/git.rs` содержит
общее открытие и инициализацию Git; чтение репозитория находится в
`src/vcs/repository.rs`. `src/project/start.rs` исполняет workflow plan через
`src/vcs/network.rs`, `src/vcs/repository.rs` и узкий fallback в `src/vcs/command.rs`.

## Что менять и где

| Задача | Первый файл |
|---|---|
| Изменить флаги, help или вывод `init` | [`src/cli/commands/init.rs`](../src/cli/commands/init.rs) |
| Изменить флаги, help или вывод `new` | [`src/cli/commands/new.rs`](../src/cli/commands/new.rs) |
| Изменить сборку или её вывод | [`crates/eska-build/src/cli/commands/build.rs`](../crates/eska-build/src/cli/commands/build.rs), затем [`crates/eska-build-runtime/src/build/`](../crates/eska-build-runtime/src/build/) |
| Изменить вывод, JSON или ошибки сборки | [`crates/eska-build/src/cli/commands/build/output.rs`](../crates/eska-build/src/cli/commands/build/output.rs), [`json.rs`](../crates/eska-build/src/cli/commands/build/json.rs), [`errors.rs`](../crates/eska-build/src/cli/commands/build/errors.rs) |
| Изменить план или генерацию patch-extension | [`src/cli/commands/patch.rs`](../src/cli/commands/patch.rs), затем [`src/project/patch/`](../src/project/patch/) |
| Изменить общие настройки запуска платформы | [`src/cli/platform.rs`](../src/cli/platform.rs), затем [`crates/eska-build-runtime/src/build/tool.rs`](../crates/eska-build-runtime/src/build/tool.rs) |
| Изменить проверки или вывод `doctor` | [`src/cli/commands/doctor.rs`](../src/cli/commands/doctor.rs), затем [`src/project/doctor.rs`](../src/project/doctor.rs) |
| Изменить общие имена объектов и путей в CLI | [`src/cli/changes.rs`](../src/cli/changes.rs) |
| Изменить обратимое кодирование JSON-путей и Git-строк | [`crates/eska-presentation/src/encoding.rs`](../crates/eska-presentation/src/encoding.rs) |
| Изменить human/JSON вывод `status` | [`src/cli/commands/status.rs`](../src/cli/commands/status.rs) |
| Изменить версию проекта 1С или её вывод | [`src/cli/commands/version.rs`](../src/cli/commands/version.rs), затем [`crates/eska-project-core/src/project/version.rs`](../crates/eska-project-core/src/project/version.rs) |
| Изменить режимы или вывод `diff` | [`src/cli/commands/diff.rs`](../src/cli/commands/diff.rs), затем [`src/project/diff.rs`](../src/project/diff.rs) |
| Изменить вывод или связь commit с task в `history` | [`src/cli/commands/history.rs`](../src/cli/commands/history.rs), затем [`src/project/history.rs`](../src/project/history.rs) |
| Изменить запуск задачи или его ошибки | [`src/cli/commands/start.rs`](../src/cli/commands/start.rs), затем [`src/project/start.rs`](../src/project/start.rs) |
| Подключить новую явно запрошенную команду | [`src/cli/commands/mod.rs`](../src/cli/commands/mod.rs) |
| Изменить общий `--help`, `--lang`, `--project-dir` | [`src/cli/args.rs`](../src/cli/args.rs) |
| Изменить подключение существующего проекта | [`src/project/init.rs`](../src/project/init.rs): `inspect` — без записи, `apply` — применение |
| Изменить создание проекта или откат | [`src/project/create.rs`](../src/project/create.rs) |
| Изменить состав создаваемых файлов | [`crates/eska-project-core/src/project/templates.rs`](../crates/eska-project-core/src/project/templates.rs) |
| Изменить поиск project/workspace и проверку исходников | [`crates/eska-project-core/src/project/discovery.rs`](../crates/eska-project-core/src/project/discovery.rs) |
| Изменить расчёт состояния проекта и readiness | [`src/project/status.rs`](../src/project/status.rs) |
| Изменить схему project `eska.toml` | [`crates/eska-project-core/src/config/schema.rs`](../crates/eska-project-core/src/config/schema.rs), затем [`crates/eska-project-core/src/config/project.rs`](../crates/eska-project-core/src/config/project.rs) |
| Изменить схему workspace `eska.toml` | [`crates/eska-project-core/src/config/workspace.rs`](../crates/eska-project-core/src/config/workspace.rs), затем [`crates/eska-project-core/src/project/discovery.rs`](../crates/eska-project-core/src/project/discovery.rs) |
| Изменить распознавание типа выгрузки | [`crates/eska-project-core/src/project/designer_xml.rs`](../crates/eska-project-core/src/project/designer_xml.rs) |
| Изменить Git init или обнаружение Git | [`src/vcs/git.rs`](../src/vcs/git.rs) |
| Изменить clone/fetch или transport fallback policy | [`src/vcs/network.rs`](../src/vcs/network.rs) |
| Изменить system Git capability fallback | [`src/vcs/command.rs`](../src/vcs/command.rs) |
| Изменить чтение HEAD, refs или истории | [`src/vcs/repository.rs`](../src/vcs/repository.rs) |
| Изменить состояние файлов и changed paths | [`src/vcs/status.rs`](../src/vcs/status.rs) |
| Изменить workflow policy или план задачи | [`crates/eska-project-core/src/workflow/policy.rs`](../crates/eska-project-core/src/workflow/policy.rs) |
| Изменить клавиши меню | [`src/cli/interactive/keyboard.rs`](../crates/eska-terminal/src/interactive/keyboard.rs) |
| Изменить оформление меню | [`src/cli/interactive/render.rs`](../crates/eska-terminal/src/interactive/render.rs) |
| Изменить приоритет языка | [`crates/eska-presentation/src/localization/locale.rs`](../crates/eska-presentation/src/localization/locale.rs) |

## Границы ответственности

- `main.rs` только передаёт управление CLI. Общие аргументы находятся в `args.rs`;
  список команд и их диспетчеризация — в `commands/mod.rs`.
- Каждый обработчик команды держит вместе свои аргументы, help, диалог и
  представление специфичных для команды ошибок. Общие ошибки проекта, global
  config и платформы находятся в `diagnostics.rs`; общие machine-local options —
  в `platform.rs`, представление путей и semantic identities — в `changes.rs`,
  обратимые JSON-пути и Git byte strings — в `encoding.rs`.
- Обработчики команд не используют внутренние функции соседних команд. Общий
  код сначала поднимается из `commands/` в соответствующий модуль `cli/`.
- `commands/build.rs` управляет выбором, preflight и последовательным выполнением.
  Его внутренние модули отвечают за ошибки, JSON и вывод. Общий
  `cli/process_output.rs` обслуживает диагностику `build`, `import` и `new --from`;
  `process_output/progress.rs` владеет spinner и синхронизацией stderr.
  Эти детали остаются в CLI и не входят в публичный API.
- `project`, `config` и `vcs` не зависят от `cli`, `clap`,
  терминала и локализованных строк. Они возвращают данные и структурированные ошибки.
- Только `cli/interactive/terminal.rs` владеет переключением режимов терминала
  и их восстановлением. Обработка клавиш и отрисовка тестируются без TTY.
- `config/schema.rs` описывает внешний TOML-формат; `config/project.rs` переводит
  его в проверенные настройки, `config/workflow.rs` преобразует строковые значения
  policy и сохраняет только явные overrides. Модель проекта не зависит от TOML-парсера.
- `config/manifest.rs` различает взаимоисключающие `[project]` и `[workspace]`;
  `config/workspace.rs` проверяет корневые defaults и относительные member paths.
  `project/discovery.rs::discover_context` валидирует весь workspace, канонические
  границы и наследование настроек. `build`, `version`, `status`, `diff` и `save`
  используют общий selection; старый `discover` остаётся одно-проектной границей
  команд без workspace-семантики.
- `project/templates.rs` возвращает план файлов, но ничего не записывает.
  Запись и откат принадлежат конкретной операции: у `new` — новый каталог,
  у `init` — только созданные этим запуском config и Git-метаданные.
- `project/onboarding.rs` готовит добавление пути в `workspace.members`, сохраняет
  TOML-комментарии через syntax-aware edit и публикует manifest только при
  совпадении проверенных байтов. Workspace-потоки `new` и `init` используют эту
  транзакцию без member-level Git/workflow; поздняя ошибка возвращает точные
  исходные байты root manifest и удаляет только созданные текущим запуском пути.
- Git находится в `vcs/`: `git.rs` открывает и инициализирует репозитории,
  `repository.rs` возвращает HEAD, refs, историю и ahead/behind, `status.rs`
  сравнивает HEAD/index/worktree, а `diff.rs` разрешает commit-like revisions,
  merge base и сравнивает committed trees. `network.rs` выполняет clone/fetch
  через `gix` и выбирает system Git fallback до сетевой попытки только для
  неподдерживаемых remote-helper transport. Состояние файлов не требует разбора
  Designer XML.
- `project/status.rs` объединяет configuration, workflow policy и read-only Git
  в снимок проекта либо выбранной группы workspace из одного repository status.
  `cli/commands/status.rs` сохраняет одиночную JSON v1 и формирует отдельную
  aggregate JSON v1 с members и workspace-owned files.
- `project/diff.rs` отбирает изменения внутри корня проекта и переводит пути в
  project-relative вид: workspace сохраняет отдельные состояния index/worktree,
  revision comparison — исходные и resolved endpoints и одно состояние файла.
  `project/metadata.rs` распознаёт Designer XML ownership для human-вывода,
  сворачивает служебные payload-файлы в ближайший узел Конфигуратора и сравнивает
  свойства дочерних объектов только в изменённых главных XML-файлах.
  `cli/commands/diff.rs` выбирает проекты и представление, `diff/analysis.rs`
  выполняет semantic-анализ и сообщает fallback. `diff/human.rs` группирует
  logical identities по типу и состоянию, оформляет TTY-заголовки и маркеры.
  `diff/raw.rs` сохраняет табличный контракт, `diff/json.rs` формирует
  workspace JSON версии 1 и revision JSON версии 2. Для project workspace один
  Git snapshot или tree comparison проецируется на selected members и корневые
  файлы; aggregate semantic JSON использует версию 4 и явно описывает полноту
  анализа каждого member.
- `project/object_model.rs` строит read-only индекс логических объектов Designer
  XML. Полный обход остаётся доступен явным библиотечным вызовом; `diff` и draft
  `save` выводят набор нужных дескрипторов из changed paths и не индексируют
  неизменённые members. Читаемый `ObjectId` формируется из
  machine-facing metadata type/name и иерархии; UUID хранится отдельно, поскольку
  Designer может повторять его у разных объектов. Индекс связывает descriptors,
  inline children, формы, модули и payload paths в обоих направлениях, не создавая
  cache и не подключаясь автоматически к file-level командам.
- `project/semantic.rs` координирует чтение снимков и генерацию событий.
  `semantic/changes.rs` нормализует workspace и revision file changes в общий
  `ChangeSet` и проецирует пути через `ObjectModel`; `semantic/model.rs` хранит
  типы событий и ошибок. `semantic/descriptor.rs` сравнивает XML, а `routines.rs`
  разбирает BSL routines. Публичные импорты через `project::semantic` сохранены.
  Результат — детерминированные semantic
  events со стабильной object identity, byte paths и comparison stage. Ошибка
  отдельного дескриптора или консервативного BSL parser сохраняется как точный
  file-level fallback и не отменяет независимые объекты.
- `project/patch/plan.rs` читает только committed Git snapshots и формирует
  полный allowlist-план. `methods.rs` отвечает только за доказуемо безопасные
  замены BSL-методов, `descriptor.rs` — за adoption существующих общих модулей,
  `extension.rs` — за Designer XML/BSL representation, а `execute.rs` — за
  изолированную временную ИБ, platform validation и публикацию нового `.cfe`.
- `project/history.rs` получает ограниченную историю HEAD и связывает commit с
  задачей только при однозначной достижимости из одной локальной task-ветки вне
  base. `cli/commands/history.rs` локализует human-вывод и формирует стабильный
  JSON версии 1, сохраняя произвольные Git-байты через явную кодировку.
- `project/build/` отделяет переносимый план от machine-local обнаружения
  `ibcmd` и исполнения. `BuildPlan` разделяет source root участника и output scope
  workspace; `infobase.rs` повторно использует совместимую служебную базу,
  блокирует конкурентный доступ и удаляет только помеченные eska каталоги;
  `execute.rs` предоставляет read-only preflight и безопасную публикацию
  артефакта. `manifest.rs` создаёт ограниченный
  снимок Designer XML, вычисляет SHA-256 и формирует паспорт без локальных путей;
  `execute.rs` публикует artifact/manifest как одну восстанавливаемую пару.
  `cli/commands/build.rs` сначала
  проверяет всю выбранную группу и обнаруживает подходящие runners. Обычный режим
  затем последовательно выполняет планы, сохраняя одиночный JSON v1 и отдельный
  aggregate JSON v1; `--dry-run` останавливается до исполнения и возвращает
  единый JSON-документ плана v1 с project/workspace scope.
- `tests/fixtures/ibcmd` предоставляет отдельный нативный Rust-процесс для
  переносимых интеграционных проверок host runner. Он воспроизводит только
  используемый command contract `ibcmd`; приёмка с настоящей платформой 1С и
  Linux-сценарии Distrobox ведутся отдельно.
- `project/version.rs` находит единственный корневой Designer XML descriptor,
  валидирует четырёхкомпонентную версию и при bump заменяет только диапазон
  текста прямого `Properties/Version` без повторной сериализации XML.
  `project/selection.rs` разрешает standalone/current/named/all selection без
  зависимости от CLI. `cli/commands/version.rs` сохраняет JSON v1 одиночного
  проекта и формирует отдельный versioned документ для списка workspace members.
- `project/save.rs` строит read-only `SavePlan` с точными scope-relative paths и
  состояниями index/worktree для корня проекта или workspace. Исполнение заново
  проходит ту же подготовку, затем выполняет index snapshot/stage/commit/rollback.
  System Git получает точный cwd и pathspec `.`, поэтому staged sibling paths
  большого репозитория не входят в commit и сохраняются в index.
- `src/project/start.rs` выполняет locale-independent preflight всего worktree,
  получает remote refs через `src/vcs/network.rs`, проверяет ancestry через `gix`,
  обновляет неактивную base ref транзакцией compare-and-swap и активирует новую
  task-ветку. System Git обновляет активную base и переключает worktree, потому
  что эти операции должны согласованно изменить HEAD, index и файлы.
  `cli/commands/start.rs` отвечает только за аргументы и RU/EN presentation.
- `project/finish.rs` определяет задачу по активной ветке, отклоняет dirty и
  незавершённые Git-операции, получает remote refs, fast-forward обновляет base и
  проверяет publication/integration requirement. После системного переключения
  worktree на base локальная task ref удаляется через `gix` с compare-and-swap;
  publish, merge и удаление remote branch не выполняются. `cli/commands/finish.rs`
  отвечает за RU/EN presentation.
- `project/switch.rs` через `gix` проверяет workflow target, локальную ref,
  владельцев linked worktrees и полку целевой ветки. `vcs/shelves` сохраняет
  изменения исходной ветки и восстанавливает целевую полку. Fetch и создание
  веток не выполняются. Изолированный system Git активирует существующую ветку
  с `--no-guess --no-overwrite-ignore`; CLI отвечает за task/base, RU/EN и JSON.
- `vcs/shelves/mod.rs` управляет preflight, repository-wide блокировкой,
  branch/base identity, журналом восстановления и жизненным циклом полки.
  `vcs/shelves/files.rs` хранит raw bytes и SHA-256 снимки только изменённых путей,
  проверяет пути и восстанавливает файлы через временные соседние файлы.
  Исходный index хранится целиком; его tree закреплена через gix ref
  `refs/eska/shelves/<id>`. Storage — `eska-shelves/` в общем Git-каталоге.
  System Git `write-tree` сериализует index, `restore` очищает только captured
  tracked paths через NUL-delimited literal pathspec. Fallback необходим для
  согласованного изменения index/worktree с учётом атрибутов Git; shell и human
  output не разбираются. Полка удаляется после проверки восстановленных байтов.
  `shelves::plan`/`restore_plan` и `switch` preview не создают файлы или блокировки.
  Sparse/split index и специальные index flags отклоняются до записи.
- `project/save.rs` выбирает все changed paths внутри корня проекта, отклоняет
  конфликты и detached HEAD и предоставляет тот же preflight через `SavePlan`
  для `save --dry-run`. При исполнении модуль сохраняет исходный index для
  rollback и поручает staging/commit системному Git. `git commit --only` не
  включает подготовленные sibling paths; `cli/commands/save.rs` отвечает за
  `-m`, preview, configured editor и локализованные сообщения.
- `workflow.rs` хранит выбор preset, проверенные overrides и разрешает доступные
  встроенные policies; `workflow/policy.rs` проверяет поля, содержит defaults
  Trunk, Git Flow и GitHub Flow, применяет overrides и строит декларативный план
  задачи без доступа к репозиторию. Git Flow также хранит внутренние slots
  будущих release- и hotfix-веток;
  планирование не выполняет Git-команды и не заменяет runtime preflight.
- Unit-тесты находятся рядом с реализацией в `#[cfg(test)] mod tests`.
  Интеграционные сценарии сгруппированы по команде или операции проекта;
  тесты discovery/templates также проверяют соответствующий CLI-контракт.

Для небольшой самостоятельной области достаточно одного файла, как
`project/designer_xml.rs`. Подкаталог нужен, когда появляются несколько самостоятельных
обязанностей. Не добавляйте безымянные `utils`, `helpers` или пустые слои на будущее.

## Запуск конкретных тестов

```bash
export ESKA_TEST_ROOT="$(realpath ../eska-playground)"
cargo test --test integration cli::init
cargo test --test integration cli::new
cargo test --test integration project::discovery
cargo test --lib cli::interactive
cargo test --test integration vcs::
```

Полный набор проверок и правила временных каталогов описаны в
[`AGENTS.md`](../AGENTS.md#проверки-и-завершение-задачи).
После выделения Cargo workspace проверяйте все packages:

```bash
cargo fmt --check
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
ESKA_TEST_ROOT="$(realpath ../eska-playground)" cargo test --workspace
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps
```

Изолированная сборка ядра и проверка его normal dependencies:

```bash
cargo check -p eska-project-core
cargo tree -p eska-project-core --edges normal
```

## Изменение путей Rust-модулей

При структурном рефакторинге экспериментальные пути `creation`, `initialization`,
`discovery`, `templates` перенесены в `project::{create, init, discovery, templates}`;
`localization` — в `cli::localization`, `project::WorkflowPreset` — в
`vcs::workflow::WorkflowPreset`. Старые пути не поддерживаются через aliases.
`project::{Project, ProjectType, ...}` и `config::ProjectConfig` остаются точками
доступа к основным типам. Команды, флаги, пользовательские тексты, exit codes и
формат `eska.toml` при этом не изменены.

T90 выделяет реализации манифестов, модели и Designer XML в
`eska-project-core`; существующий CLI использует их через `config`, `project`
и `vcs::workflow`. Корневой пакет остаётся потребителем общей библиотеки,
повторных реализаций этих операций нет. [Границы и baseline](tooling-modularity.md).

В T08 `ProjectConfiguration` хранит `WorkflowSettings` со строковыми overrides,
поэтому больше не реализует `Copy`; getters принимают `&self`.
`workflow()` возвращает выбранный preset, `workflow_settings()` — все настройки.
В discovery настройки клонируются при построении проекта с проверенным source.

## Чтение изменённых файлов

`vcs/snapshot.rs` отделяет чтение содержимого от расчёта Git status.
`FileVersionReader` удерживает HEAD tree и index snapshot на время одного
file-level или semantic анализа. Инициализация ленивая: если содержимое не нужно,
reader не создаётся. Файлы worktree читаются по одному, без накопления всех blobs.
Следующая операция заново получает HEAD и индекс; публичный
`Repository::file_versions` сохраняет чтение одного файла. Атомарный снимок
worktree при параллельном редактировании не гарантируется.

`project/semantic/routines.rs` содержит консервативный parser процедур и функций.
Он читает строки через iterator и собирает нормализованные тела методов вместе с
видом объявления и 1-based координатами ключевого слова, без копии всего модуля
для замены CRLF и без массивов строк. Сопоставление snapshot-пар, подавление
производных событий при lifecycle объекта и создание событий остаются в
`project/semantic.rs`.
`project/semantic/descriptor.rs` строит XML-сигнатуры в одном буфере без
промежуточных строк поддеревьев. Они учитывают namespace URI, отделяют текст от
структуры и игнорируют комментарии, processing instructions и форматирование.
`project/metadata.rs` содержит общий список имён BSL/binary-модулей для
object model и semantic diff.

Условия и результаты локальных замеров: [performance.md](performance.md).
