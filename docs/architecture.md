# Навигация по исходному коду

Каталоги называются по области ответственности, файлы — по операции или
конкретной части реализации. Начинайте с обработчика команды, если меняется её
поведение для пользователя, и с модуля проекта, если меняется сама операция.

## Структура

```text
src/
├── main.rs                      # точка входа бинарника
├── lib.rs                       # карта модулей библиотеки
├── cli/
│   ├── mod.rs                   # запуск CLI и выбор локали
│   ├── args.rs                  # общие аргументы, bootstrap --lang, общий help
│   ├── changes.rs               # общее представление путей и semantic changes
│   ├── platform.rs              # общие machine-local настройки запуска 1С
│   ├── commands/
│   │   ├── mod.rs               # регистрация и диспетчеризация команд
│   │   ├── build.rs             # eska build: аргументы, RU/EN и JSON result
│   │   ├── config.rs            # eska config: init/edit глобальных настроек
│   │   ├── doctor.rs            # eska doctor: selectors, RU/EN и versioned JSON
│   │   ├── platform.rs          # eska platform list: human/JSON presentation
│   │   ├── patch.rs             # eska patch: аргументы, preview и JSON result
│   │   ├── init.rs              # eska init: аргументы, prompts, help, вывод
│   │   ├── new.rs               # eska new: аргументы, prompts, help, вывод
│   │   ├── diff.rs              # eska diff: human/raw/JSON presentation
│   │   ├── finish.rs            # eska finish: localized result и ошибки
│   │   ├── history.rs           # eska history: human/JSON presentation
│   │   ├── save.rs              # eska save: draft сообщения и presentation
│   │   ├── start.rs             # eska start: localized result и ошибки
│   │   ├── status.rs            # eska status: human/JSON presentation
│   │   ├── switch.rs            # eska switch: выбор цели и presentation
│   │   ├── version.rs           # version одного проекта или списка workspace members
│   │   └── validate.rs          # проверка при запуске без подкоманды
│   ├── diagnostics.rs           # общие ошибки project/config/platform
│   ├── interactive/
│   │   ├── mod.rs               # общие варианты выбора и ошибки prompts
│   │   ├── select.rs            # цикл событий и подтверждение выбора
│   │   ├── keyboard.rs          # клавиши, модификаторы, fallback раскладки
│   │   ├── render.rs            # отрисовка, цвета, минимальный размер окна
│   │   └── terminal.rs          # raw mode, alternate screen и восстановление
│   └── localization/
│       ├── mod.rs               # интерфейс локализации
│       ├── locale.rs            # поддерживаемые локали и их приоритет
│       └── localizer.rs         # загрузка Fluent-ресурсов и форматирование
├── project/
│   ├── mod.rs                   # интерфейс модели и операций проекта
│   ├── model.rs                 # Project, типы проекта, инварианты путей
│   ├── create.rs                # создание нового каталога и откат
│   ├── init.rs                  # обнаружение выгрузки, подключение и откат
│   ├── designer_xml.rs          # распознавание корневого XML-дескриптора
│   ├── discovery.rs             # поиск ближайшего проекта и проверка source
│   ├── doctor.rs                # read-only проверки project/build/VCS окружения
│   ├── workspace.rs             # Workspace, members и переносимые имена проектов
│   ├── diff.rs                  # file-level изменения внутри корня проекта
│   ├── finish.rs                # preflight, policy refs и локальное завершение задачи
│   ├── history.rs               # локальная commit history и task attribution
│   ├── metadata.rs              # human-проекция путей и XML-дочерних объектов
│   ├── object_model.rs          # логические Designer XML objects и двусторонний path index
│   ├── clone.rs                 # план clone, владение destination и validation
│   ├── build/
│   │   ├── mod.rs               # публичная граница build subsystem
│   │   ├── settings.rs          # переносимые settings и версия платформы
│   │   ├── plan.rs              # тип и путь build artifact без запуска процессов
│   │   ├── tool.rs              # поиск, version check и запуск ibcmd/Distrobox
│   │   ├── manifest.rs          # снимок source, SHA-256 и паспорт артефакта
│   │   └── execute.rs           # временная база, import, cleanup и публикация
│   ├── patch/
│   │   ├── mod.rs               # публичная граница patch subsystem
│   │   ├── model.rs             # PatchPlan, изменения, модули и ошибки
│   │   ├── plan.rs              # immutable Git endpoints и классификация delta
│   │   ├── methods.rs           # консервативный разбор и замена BSL-методов
│   │   ├── descriptor.rs        # проверка и adoption CommonModule.xml
│   │   ├── extension.rs         # запись Designer XML/BSL с BOM и CRLF
│   │   └── execute.rs           # временная ИБ, platform checks и публикация
│   ├── save.rs                  # project-scoped staging, commit и rollback index
│   ├── selection.rs             # общий выбор current/named/all workspace projects
│   ├── semantic.rs              # ChangeSet → object ownership → ChangeSummary
│   ├── semantic/routines.rs     # потоковое чтение BSL routine snapshots
│   ├── start.rs                 # preflight и исполнение task plan
│   ├── status.rs                # снимок проекта, ChangeSet summary и readiness
│   ├── version.rs               # точечное чтение и замена Properties/Version
│   └── templates.rs             # план файлов встроенного каркаса
├── config/
│   ├── mod.rs                   # интерфейс config и имя eska.toml
│   ├── project.rs               # ProjectConfig, загрузка и валидация
│   ├── manifest.rs              # различение строгих project/workspace manifests
│   ├── workspace.rs             # WorkspaceConfig и проверка member paths
│   ├── workflow.rs              # преобразование workflow-полей в доменную модель
│   └── schema.rs                # TOML-поля, defaults и строковые значения
└── vcs/
    ├── mod.rs                   # граница VCS
    ├── git.rs                   # общее открытие и инициализация Git через gix
    ├── command.rs               # единый system Git capability fallback
    ├── network.rs               # clone/fetch через gix и transport fallback policy
    ├── diff.rs                  # разрешение revisions и tree-to-tree diff через gix
    ├── repository.rs            # discovery, HEAD, refs и ограниченная история
    ├── snapshot.rs              # общий HEAD/index reader на время анализа файлов
    ├── status.rs                # изменения HEAD/index/worktree и changed paths
    ├── workflow.rs              # выбор preset, overrides и разрешение policy
    └── workflow/
        └── policy.rs            # валидация policy и декларативный план задачи

locales/{ru-RU,en-US}/main.ftl    # пользовательские тексты
assets/project/                    # встроенные .gitattributes и .gitignore для new
tests/
├── integration.rs               # точка входа интеграционных тестов
├── cli/{build,diff,doctor,finish,history,init,new,save,start,status,version,localization}.rs
├── project/{discovery,finish,history,save,start,templates,version,workflow}.rs
├── vcs/{diff,network,repository,status,support}.rs # Git-сценарии и fixture-команды
└── support/mod.rs               # общий изолированный временный каталог
```

`validate.rs` — обработчик существующего запуска `eska` без подкоманды,
а не новая команда `validate` или запланированная `check`. `vcs/git.rs` содержит
общее открытие и инициализацию Git; чтение репозитория находится в
`vcs/repository.rs`. `project/start.rs` исполняет workflow plan через
`vcs/network.rs`, `vcs/repository.rs` и узкий fallback в `vcs/command.rs`.

## Что менять и где

| Задача | Первый файл |
|---|---|
| Изменить флаги, help или вывод `init` | [`src/cli/commands/init.rs`](../src/cli/commands/init.rs) |
| Изменить флаги, help или вывод `new` | [`src/cli/commands/new.rs`](../src/cli/commands/new.rs) |
| Изменить сборку или её вывод | [`src/cli/commands/build.rs`](../src/cli/commands/build.rs), затем [`src/project/build/`](../src/project/build/) |
| Изменить план или генерацию patch-extension | [`src/cli/commands/patch.rs`](../src/cli/commands/patch.rs), затем [`src/project/patch/`](../src/project/patch/) |
| Изменить общие настройки запуска платформы | [`src/cli/platform.rs`](../src/cli/platform.rs), затем [`src/project/build/tool.rs`](../src/project/build/tool.rs) |
| Изменить проверки или вывод `doctor` | [`src/cli/commands/doctor.rs`](../src/cli/commands/doctor.rs), затем [`src/project/doctor.rs`](../src/project/doctor.rs) |
| Изменить общие имена объектов и путей в CLI | [`src/cli/changes.rs`](../src/cli/changes.rs) |
| Изменить human/JSON вывод `status` | [`src/cli/commands/status.rs`](../src/cli/commands/status.rs) |
| Изменить версию проекта 1С или её вывод | [`src/cli/commands/version.rs`](../src/cli/commands/version.rs), затем [`src/project/version.rs`](../src/project/version.rs) |
| Изменить режимы или вывод `diff` | [`src/cli/commands/diff.rs`](../src/cli/commands/diff.rs), затем [`src/project/diff.rs`](../src/project/diff.rs) |
| Изменить вывод или связь commit с task в `history` | [`src/cli/commands/history.rs`](../src/cli/commands/history.rs), затем [`src/project/history.rs`](../src/project/history.rs) |
| Изменить запуск задачи или его ошибки | [`src/cli/commands/start.rs`](../src/cli/commands/start.rs), затем [`src/project/start.rs`](../src/project/start.rs) |
| Подключить новую явно запрошенную команду | [`src/cli/commands/mod.rs`](../src/cli/commands/mod.rs) |
| Изменить общий `--help`, `--lang`, `--project-dir` | [`src/cli/args.rs`](../src/cli/args.rs) |
| Изменить подключение существующего проекта | [`src/project/init.rs`](../src/project/init.rs): `inspect` — без записи, `apply` — применение |
| Изменить создание проекта или откат | [`src/project/create.rs`](../src/project/create.rs) |
| Изменить состав создаваемых файлов | [`src/project/templates.rs`](../src/project/templates.rs) |
| Изменить поиск project/workspace и проверку исходников | [`src/project/discovery.rs`](../src/project/discovery.rs) |
| Изменить расчёт состояния проекта и readiness | [`src/project/status.rs`](../src/project/status.rs) |
| Изменить схему project `eska.toml` | [`src/config/schema.rs`](../src/config/schema.rs), затем [`src/config/project.rs`](../src/config/project.rs) |
| Изменить схему workspace `eska.toml` | [`src/config/workspace.rs`](../src/config/workspace.rs), затем [`src/project/discovery.rs`](../src/project/discovery.rs) |
| Изменить распознавание типа выгрузки | [`src/project/designer_xml.rs`](../src/project/designer_xml.rs) |
| Изменить Git init или обнаружение Git | [`src/vcs/git.rs`](../src/vcs/git.rs) |
| Изменить clone/fetch или transport fallback policy | [`src/vcs/network.rs`](../src/vcs/network.rs) |
| Изменить system Git capability fallback | [`src/vcs/command.rs`](../src/vcs/command.rs) |
| Изменить чтение HEAD, refs или истории | [`src/vcs/repository.rs`](../src/vcs/repository.rs) |
| Изменить состояние файлов и changed paths | [`src/vcs/status.rs`](../src/vcs/status.rs) |
| Изменить workflow policy или план задачи | [`src/vcs/workflow/policy.rs`](../src/vcs/workflow/policy.rs) |
| Изменить клавиши меню | [`src/cli/interactive/keyboard.rs`](../src/cli/interactive/keyboard.rs) |
| Изменить оформление меню | [`src/cli/interactive/render.rs`](../src/cli/interactive/render.rs) |
| Изменить приоритет языка | [`src/cli/localization/locale.rs`](../src/cli/localization/locale.rs) |

## Границы ответственности

- `main.rs` только передаёт управление CLI. Общие аргументы находятся в `args.rs`;
  список команд и их диспетчеризация — в `commands/mod.rs`.
- Каждый обработчик команды держит вместе свои аргументы, help, диалог и
  представление специфичных для команды ошибок. Общие ошибки проекта, global
  config и платформы находятся в `diagnostics.rs`; общие machine-local options —
  в `platform.rs`, представление путей и semantic identities — в `changes.rs`.
- Обработчики команд не используют внутренние функции соседних команд. Общий
  код сначала поднимается из `commands/` в соответствующий модуль `cli/`.
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
  `cli/commands/diff.rs` группирует logical identities по типу метаданных и
  состоянию, оформляет TTY-заголовки и маркеры, отдельно формирует raw,
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
- `project/semantic.rs` нормализует workspace и revision file changes в общий
  `ChangeSet`, проецирует пути через `ObjectModel` и сравнивает BSL routines,
  формы и свойства metadata descriptors. Результат — детерминированные semantic
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
  workspace; `execute.rs` предоставляет read-only preflight, владеет временной
  базой и безопасной публикацией артефакта. `manifest.rs` создаёт ограниченный
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
- `project/start.rs` выполняет locale-independent preflight всего worktree,
  получает remote refs через `vcs/network.rs`, проверяет ancestry через `gix`,
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
- `project/switch.rs` через `gix` проверяет workflow target, локальную ref и
  чистоту всего worktree, не выполняя fetch и не создавая веток. Изолированный
  system Git активирует существующую ветку с `--no-guess`, чтобы согласованно
  изменить HEAD, index и файлы; `cli/commands/switch.rs` отвечает за выбор
  task/base и RU/EN presentation.
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

## Изменение путей Rust-модулей

При структурном рефакторинге экспериментальные пути `creation`, `initialization`,
`discovery`, `templates` перенесены в `project::{create, init, discovery, templates}`;
`localization` — в `cli::localization`, `project::WorkflowPreset` — в
`vcs::workflow::WorkflowPreset`. Старые пути не поддерживаются через aliases.
`project::{Project, ProjectType, ...}` и `config::ProjectConfig` остаются точками
доступа к основным типам. Команды, флаги, пользовательские тексты, exit codes и
формат `eska.toml` при этом не изменены.

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
Он читает строки через iterator и собирает только нормализованные тела методов,
без копии всего модуля для замены CRLF и без массивов строк. Сопоставление
snapshot-пар и создание событий остаются в `project/semantic.rs`.
XML-сигнатуры строятся в одном буфере без промежуточных строк поддеревьев;
правила нормализации не изменены.

Условия и результаты локальных замеров: [performance.md](performance.md).
