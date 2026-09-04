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
│   ├── commands/
│   │   ├── mod.rs               # регистрация и диспетчеризация команд
│   │   ├── init.rs              # eska init: аргументы, prompts, help, вывод
│   │   ├── new.rs               # eska new: аргументы, prompts, help, вывод
│   │   └── validate.rs          # проверка при запуске без подкоманды
│   ├── diagnostics.rs           # общие сообщения ошибок проекта и config
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
│   ├── discovery.rs             # поиск ближайшего проекта и проверка source
│   └── templates.rs             # план файлов встроенного каркаса
├── config/
│   ├── mod.rs                   # интерфейс config и имя eska.toml
│   ├── project.rs               # ProjectConfig, загрузка и валидация
│   ├── workflow.rs              # преобразование workflow-полей в доменную модель
│   └── schema.rs                # TOML-поля, defaults и строковые значения
├── designer_xml.rs              # распознавание корневого XML-дескриптора
└── vcs/
    ├── mod.rs                   # граница VCS
    ├── git.rs                   # общее открытие и инициализация Git через gix
    ├── repository.rs            # discovery, HEAD, refs и ограниченная история
    ├── status.rs                # изменения HEAD/index/worktree и changed paths
    ├── workflow.rs              # выбор preset, custom overrides и разрешение policy
    └── workflow/
        └── policy.rs            # валидация policy и декларативный план задачи

locales/{ru-RU,en-US}/main.ftl    # пользовательские тексты
assets/project/                    # встроенные .gitattributes и .gitignore для new
tests/
├── integration.rs               # точка входа интеграционных тестов
├── cli/{init,new,localization}.rs
├── project/{discovery,templates,workflow}.rs
├── vcs/{repository,status,support}.rs # реальные Git-репозитории и fixture-команды
└── support/mod.rs               # общий изолированный временный каталог
```

`validate.rs` — обработчик существующего запуска `eska` без подкоманды,
а не новая команда `validate` или запланированная `check`. `vcs/git.rs` содержит
общее открытие и инициализацию Git; чтение репозитория находится в
`vcs/repository.rs`. Workflow policy не исполняется.

## Что менять и где

| Задача | Первый файл |
|---|---|
| Изменить флаги, help или вывод `init` | [`src/cli/commands/init.rs`](../src/cli/commands/init.rs) |
| Изменить флаги, help или вывод `new` | [`src/cli/commands/new.rs`](../src/cli/commands/new.rs) |
| Подключить новую явно запрошенную команду | [`src/cli/commands/mod.rs`](../src/cli/commands/mod.rs) |
| Изменить общий `--help`, `--lang`, `--project-dir` | [`src/cli/args.rs`](../src/cli/args.rs) |
| Изменить подключение существующего проекта | [`src/project/init.rs`](../src/project/init.rs): `inspect` — без записи, `apply` — применение |
| Изменить создание проекта или откат | [`src/project/create.rs`](../src/project/create.rs) |
| Изменить состав создаваемых файлов | [`src/project/templates.rs`](../src/project/templates.rs) |
| Изменить поиск корня и проверку исходников | [`src/project/discovery.rs`](../src/project/discovery.rs) |
| Изменить схему `eska.toml` | [`src/config/schema.rs`](../src/config/schema.rs), затем [`src/config/project.rs`](../src/config/project.rs) |
| Изменить распознавание типа выгрузки | [`src/designer_xml.rs`](../src/designer_xml.rs) |
| Изменить Git init или обнаружение Git | [`src/vcs/git.rs`](../src/vcs/git.rs) |
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
  представление собственных ошибок. Общие ошибки проекта — в `diagnostics.rs`.
- `project`, `config`, `designer_xml` и `vcs` не зависят от `cli`, `clap`,
  терминала и локализованных строк. Они возвращают данные и структурированные ошибки.
- Только `cli/interactive/terminal.rs` владеет переключением режимов терминала
  и их восстановлением. Обработка клавиш и отрисовка тестируются без TTY.
- `config/schema.rs` описывает внешний TOML-формат; `config/project.rs` переводит
  его в проверенные настройки, `config/workflow.rs` преобразует строковые значения
  policy и сохраняет только явные overrides. Модель проекта не зависит от TOML-парсера.
- `project/templates.rs` возвращает план файлов, но ничего не записывает.
  Запись и откат принадлежат конкретной операции: у `new` — новый каталог,
  у `init` — только созданные этим запуском config и Git-метаданные.
- Git находится в `vcs/`: `git.rs` открывает и инициализирует репозитории,
  `repository.rs` возвращает HEAD, refs и историю, `status.rs` сравнивает
  HEAD/index/worktree. Состояние файлов не требует разбора Designer XML.
- `workflow.rs` хранит выбор preset, проверенные overrides и разрешает доступные
  встроенные policies; `workflow/policy.rs` проверяет поля, содержит Trunk и Git
  Flow defaults, применяет overrides и строит декларативный план задачи без
  доступа к репозиторию. Git Flow также хранит внутренние slots будущих release-
  и hotfix-веток; defaults GitHub Flow добавляются в T11;
  планирование не выполняет Git-команды и не заменяет runtime preflight.
- Unit-тесты находятся рядом с реализацией в `#[cfg(test)] mod tests`.
  Интеграционные сценарии сгруппированы по команде или операции проекта;
  тесты discovery/templates также проверяют соответствующий CLI-контракт.

Для небольшой самостоятельной области достаточно одного файла, как
`designer_xml.rs`. Подкаталог нужен, когда появляются несколько самостоятельных
обязанностей. Не добавляйте безымянные `utils`, `helpers` или пустые слои на будущее.

## Запуск конкретных тестов

```bash
ESKA_TEST_ROOT=/home/kas/Projects/eska-playground cargo test --test integration cli::init
ESKA_TEST_ROOT=/home/kas/Projects/eska-playground cargo test --test integration cli::new
ESKA_TEST_ROOT=/home/kas/Projects/eska-playground cargo test --test integration project::discovery
ESKA_TEST_ROOT=/home/kas/Projects/eska-playground cargo test --lib cli::interactive
ESKA_TEST_ROOT=/home/kas/Projects/eska-playground cargo test --test integration vcs::
```

Полный набор проверок и правила временных каталогов описаны в
[`README.md`](../README.md#тестирование-при-разработке).

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
