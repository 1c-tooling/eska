# eska

[![Release-plz](https://github.com/1c-tooling/eska/actions/workflows/release-plz.yml/badge.svg)](https://github.com/1c-tooling/eska/actions/workflows/release-plz.yml)
[![Binaries](https://github.com/1c-tooling/eska/actions/workflows/release.yml/badge.svg)](https://github.com/1c-tooling/eska/actions/workflows/release.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

**Инструменты для проектов 1С в формате Designer XML: метаданные, BSL, Git и сборка.**

ESKA подключает выгрузку Конфигуратора, предоставляет дерево и редактирование
свойств в VS Code/VSCodium, готовит контекст внешнего BSL Analyzer, проверяет
проект для CI и собирает `.cf`, `.cfe`, `.epf`, `.erf` установленной платформой 1С.
Исходниками остаются обычные XML, BSL и ресурсы выгрузки. Изменения из рабочей
информационной базы автоматически не выгружаются.

Запросы СКД и ручных динамических списков доступны в отдельных SDBL-вкладках
Explorer: сохранение меняет только выбранный запрос XML, dirty-буфер владельца
остаётся несохранённым. Текущему адаптеру нужен Metadata API 1.16 с устойчивыми
сессиями ESKA; восстановление требует явного продолжения.
[Приёмка и ограничения](docs/roadmap/16-query-editing.md#t101--запросы-скд-и-динамических-списков).
BSL-литерал открывается командой «Редактировать запрос отдельно» (API 1.19).
Save сохраняет черновик; «Применить» меняет модуль одной отменяемой правкой
без сохранения на диск. Запрос заменяет вкладки родителя и возвращает их после
закрытия, отмены или применения. Пока открыт дочерний редактор, навигация
к родителю возвращает фокус дочернему.

[Подробное руководство](docs/user-guide.md) ·
[Сайт и инструкция](https://1c-tooling.github.io/eska/docs/) ·
[ESKA: 1C Explorer](https://github.com/1c-tooling/eska-vscode-explorer)

## Из чего состоит ESKA

| Инструмент | Назначение | Что требуется для его сценария |
| --- | --- | --- |
| [`eska-project`](docs/project-cli.md) | Проект, контекст приложения, Metadata API, существующие свойства и переименование | Designer XML; для дерева не нужны Git, платформа или анализатор |
| [`eska-check`](docs/check-cli.md) | Структурная проверка XML и полный анализ BSL для CI | Внешний `bsl-analyzer-app` |
| [`eska-build`](docs/build-tool.md) | Сборка, распаковка, import, patch и служебная ИБ | Платформа 1С с `ibcmd`; import/patch также используют `eska-vcs` |
| [`eska-vcs`](docs/vcs-tool.md) | Workflow, status, semantic diff, commits и полки | Git для операций записи и transport; чтение выполняется через `gix` |
| [`eska`](docs/component-delegation.md) | Общая команда, вызывающая нужный компонент | Компоненты выбранной операции |
| [ESKA: 1C Explorer](https://github.com/1c-tooling/eska-vscode-explorer) | Дерево метаданных, свойства, поддержка 1С и BSL-клиент в редакторе | VS Code/VSCodium ≥ 1.109.0 и `eska-project` с Metadata API 1.12 |

Для дерева достаточно project-компонента. Анализатор, сборщик и VCS можно
подключать отдельно. BSL Analyzer поставляется как **внешний upstream-бинарник**:
его Rust-ядро не встроено в ESKA. Текущая интеграция проверена с
[`bsl-analyzer-app` 0.3.2](https://github.com/itrous/bsl-analyzer/releases/tag/v0.3.2),
машинный контракт 3.4; другой сервер автоматически не подставляется.

## Полная ESKA или мини-программы

**Через общую `eska`** выполняйте `eska build`, `eska check`, `eska status`,
`eska save`. **Напрямую** используйте соответствующие мини-программы:

| Действие | Общая команда | Мини-программа |
| --- | --- | --- |
| Сборка | `eska build` | `eska-build build` |
| Полная проверка XML/BSL | `eska check` | `eska-check` |
| Состояние Git | `eska status` | `eska-vcs status` |
| Semantic diff | `eska diff --semantic` | `eska-vcs diff --semantic` |
| Сохранение commit | `eska save -m "Сообщение"` | `eska-vcs save -m "Сообщение"` |
| Метаданные | `eska metadata …` | `eska-project metadata …` |
| Версия проекта | `eska version` | `eska-project version` |

Это альтернативные входы для одного проекта и одного `eska.toml`.
Полный комплект содержит оболочку и четыре компонента: `eska` запускает
нужный executable, поэтому установка только crate `eska` не заменяет установку
владельцев операций. Мини-программы работают без оболочки.

**Общий CLI пока покрывает не все мини-команды.** `context`, `analyzer-config`
и явный `validate` доступны в `eska-project`, `unpack` — в `eska-build`.
Текущий Explorer использует `eska-project serve --stdio`;
`eska ide --stdio` остаётся legacy-входом. `eska new/init` настраивают платформу
и Git-workflow, а `eska-project new/init` создают только структуру проекта.

[Полная таблица существующих команд и различий](docs/user-guide.md#components) ·
[Девять сценариев с примерами](docs/user-guide.md#scenarios)

## Установка

### Текущие компоненты

Компонентная поставка доступна начиная с ESKA `0.15.0`.
Версия checkout указана в `Cargo.toml`, готовые бинарники — в
[GitHub Releases](https://github.com/1c-tooling/eska/releases).
Новые возможности этой документации требуют также текущей сборки расширения.
Опубликованная Cargo-версия не означает наличие готовых бинарников той же версии
в GitHub Releases.

С Rust/Cargo можно установить нужный компонент или весь набор:

```sh
cargo install --locked eska-project eska-check eska-build eska-vcs eska
```

Для сборки checkout из корня этого репозитория, с Rust из `rust-toolchain.toml`:

```sh
cargo build --workspace --bins --release --locked
export PATH="$PWD/target/release:$PATH"
eska-project --version
eska --help
```

Команда `export` действует в текущем shell. Для GUI укажите абсолютный путь
к `target/release/eska-project` в настройке `eska.explorer.executable`.
На Windows бинарники имеют суффикс `.exe`.
Для минимального дерева достаточно `cargo build -p eska-project --release --locked`.
[Пошаговая установка CLI, анализатора и VSIX](docs/user-guide.md#installation).

### Опубликованный релиз

Установщики скачивают **опубликованный** комплект из GitHub Releases.
Он может отставать от текущих исходников.

Linux и macOS:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://1c-tooling.github.io/eska/install | sh
```

Windows PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://1c-tooling.github.io/eska/install.ps1 | iex"
```

После первой установки откройте новый терминал и выполните `eska --version`.
Release pipeline настроен на полный комплект и
[выборочные компонентные ZIP](docs/component-installation.md); перед установкой
проверьте Assets выбранного [релиза](https://github.com/1c-tooling/eska/releases).
Для локального компонентного комплекта предусмотрены установка, проверка,
обновление, удаление и восстановление по receipts.

`eska update --check` и `eska update` относятся к опубликованному каналу
и поддерживаемому способу установки. Checkout обновляется и пересобирается
отдельно. [Обновление CLI](docs/cli-update.md).

## Быстрый старт

### Подключить существующую выгрузку для Explorer и анализа

Если `my_configuration/src/Configuration.xml` уже существует:

```sh
cd my_configuration
eska-project init --source src
eska-project validate
eska-project context --format json
```

`init` создаёт `eska.toml`, определяет тип по XML и сохраняет исходные файлы.
Он не инициализирует Git и не ищет платформу 1С. Для выгрузки прямо в корне
используйте `--source .`. Если манифест уже есть, начинайте с `validate`.
Поддерживаются конфигурация, расширение, внешняя обработка и внешний отчёт.

### Создать каркас или распаковать файл 1С

```sh
eska-project new my-report --type report
```

Это пустой source и манифест: XML объекта 1С команда не генерирует.
Для существующего бинарного файла используйте установленную платформу:

```sh
eska-build unpack processor.epf processor-xml --platform-version 8.3.27.2325
eska-project init processor-xml --source .
```

Версию в примере замените своей. `unpack` принимает CF/CFE/EPF/ERF,
пишет только в **новый** каталог и не создаёт манифест или Git.
Для проекта с Git-workflow общий CLI предоставляет составную операцию:

```sh
eska new processor --from processor.epf --workflow trunk --platform-version 8.3.27.2325
```

Она требует project/build/VCS-компоненты. Замена исходников существующего
проекта выполняется через `eska import FILE --dry-run`, затем `eska import FILE`
с просмотром и подтверждением.

## Подключение IDE

Установите актуальный VSIX Explorer, откройте доверенную папку с `eska.toml`
и задайте backend `project`. Он запускает `eska-project serve --stdio`.
Команда **Подготовить проектный и языковой компоненты** проверяет project
и анализатор отдельно; доступность установки зависит от Assets релиза.
Для локального checkout используйте абсолютный путь к собранному бинарнику.

![Дерево конфигурации в ESKA: 1C Explorer](docs/images/explorer-tree.png)

В дереве доступны метаданные, модули, формы, команды, предопределённые данные
и остальные файлы проекта. Есть поиск по имени и синониму, сортировка,
фильтр пустых корневых групп, Git-подсветка и индикаторы поддержки 1С.
Свойства открываются в отдельной вкладке: сначала для чтения, затем доступные
поля можно разблокировать замочком. Переименование выполняется через preview
замен и перемещений. Поддержка 1С проверяется backend независимо от замочка.

BSL-клиент предоставляет диагностики, completion, hover и переходы;
базовая подсветка BSL/SDBL работает без сервера. Отдельный `.sdbl` пока получает
базовую подсветку. Конструктор запросов с API 1.28 предоставляет вкладки:
«Таблицы и поля», «Связи», «Группировка», «Условия», «Порядок»,
«Объединения и псевдонимы», «Итоги», «Пакет». Таблицы и поля
выбираются из метаданных текущего приложения; доступны псевдонимы, агрегаты,
соединения, обычный и агрегатный отбор, направление сортировки. Виртуальные таблицы получают параметры
из метаданных; вложенные запросы редактируются в той же панели с возвратом
к родителю. Временные таблицы, их поля и зависимости пакета разрешает ESKA;
доступны создание, переименование, удаление и обычные индексы. Доступны ветки `UNION`/`UNION ALL`, общие итоги, иерархия и периоды.
Модель, формирование выражений и
сохранение текста принадлежат ESKA; расширение отображает операции. Изменения
применяются к тому же черновику; отмена возвращает его без изменений.
[Виртуальные таблицы и составные запросы](docs/roadmap/19-query-advanced.md).
Визуального редактора форм нет.

[Настройка редактора, навигация и свойства со скриншотами](docs/user-guide.md#explorer) ·
[Metadata API](docs/ide-protocol.md) · [Правила поддержки](docs/support-policy.md)

## Проверка и анализ

```sh
eska-project analyzer-config prepare --analyzer /path/to/bsl-analyzer-app
eska check --analyzer /path/to/bsl-analyzer-app --format json
```

Для проверки без оболочки замените вторую команду на:

```sh
eska-check --analyzer /path/to/bsl-analyzer-app --format json
```

Замените путь на настоящий. Явные команды управления конфигом анализатора
доступны через `eska-project`. Сама проверка умеет подготовить управляемый
конфиг автоматически; пользовательский файл требует миграции.
`validate` проверяет корневые дескрипторы;
`eska-check` проверяет объявленные XML и выполняет полный BSL-анализ.
Компиляцию и поведение на платформе 1С они не подтверждают.

Первичный конфиг — `eska.toml`. Состав приложения задаётся через
`[applications.<name>]`, политика анализатора — через `[tools.bsl-analyzer]`.
`bsl-analyzer.toml` генерируется ESKA; существующий пользовательский файл
требует явной миграции с предварительным просмотром и резервной копией.

[Контексты приложений](docs/application-context.md) ·
[Настройки и миграция анализатора](docs/analyzer-configuration.md) ·
[Отчёт, пороги и exit codes CI](docs/check-cli.md)

## Работа над задачей

Для сценария Git используйте проект, созданный через `eska new` или
подключённый через `eska init` с выбранным workflow. Например, из каталога
проекта с существующим `src`:

```sh
eska init --workflow trunk --platform-version 8.3.27.2325
eska save -m "chore: Подключён проект ESKA"
eska start TASK-123
eska status
eska diff --semantic
eska save --dry-run
eska save -m "fix: Исправлен расчёт суммы"
```

`trunk` использует `main` → `task/<ID>`, `github-flow` — `main` → `feature/<ID>`,
`git-flow` — `develop` → `feature/<ID>`. Пользовательские правила задаёт `custom`.
Сначала завершите создание проекта и первый commit, затем начинайте задачу.
`save` не отправляет изменения на сервер; публикация ветки и PR выполняются
отдельно. `finish` проверяет интеграцию задачи, переключает на базовую ветку
и удаляет локальную ветку задачи; merge, push и PR он не создаёт.

[Git-workflow, полки и завершение](docs/user-guide.md#daily-work) ·
[Самостоятельный VCS](docs/vcs-tool.md)

## Сборка

Через общую ESKA:

```sh
eska platform list
eska build --dry-run
eska build --manifest
```

Альтернатива для того же проекта — мини-программа:

```sh
eska-build platform list
eska-build build --dry-run
eska-build build --manifest
```

Выберите один вариант. Обычная сборка без паспорта — `eska build` или
`eska-build build`. Для `eska build --manifest` нужен `eska-vcs`, который
предоставляет Git-сведения. Прямой build с паспортом работает без VCS
и отмечает Git как `unavailable`.

Укажите точную установленную версию в `[build].platform_version`.
Нужен `ibcmd` той же версии; для patch также нужен `1cv8`.
Distrobox нужен только при размещении платформы в Linux-контейнере.
Артефакты по умолчанию попадают в `build/`. Команды `eska clean` или
`eska-build clean` удаляют управляемую служебную ИБ, сохраняя опубликованные результаты.

[Платформа, сборка EPF/ERF, паспорт и ограничения patch](docs/user-guide.md#build) ·
[Самостоятельный сборщик](docs/build-tool.md) · [Проверки сборки](docs/build-validation.md)

## Версия проекта 1С

`eska version` / `eska-project version` показывают версию из XML.
Это версия проекта 1С, а не установленного CLI.
[Изменение версии](docs/user-guide.md#version).

## Настройки

`eska.toml` хранит тип и source, сборку, workspace, приложения и политику анализа.
Машинные пути платформы и Distrobox задаются через `eska config init/edit`
или `eska-build config init/edit`. `--project-dir` задаёт начало поиска манифеста.
`--lang ru|en` меняет human-вывод; JSON keys и машинные статусы не локализуются.

[Примеры TOML и workspace](docs/user-guide.md#workspace) ·
[Настройка платформы](docs/user-guide.md#build) ·
[Диагностика](docs/user-guide.md#troubleshooting)

## Разделы CLI

| Сценарий | Команды |
| --- | --- |
| Проект | `eska-project new`, `init`, `validate`, `context`, `doctor`, `version` |
| Метаданные / IDE | `eska-project metadata`, `serve --stdio` |
| Анализ | `eska-project analyzer-config status/prepare/migrate`, `eska-check`, `eska check` |
| Сборка | `eska-build build`, `unpack`, `import`, `patch`, `clean`, `platform list`, `config` |
| Git | `eska-vcs clone/status/diff/history/start/switch/save/finish/shelve/unshelve/shelves` |
| Общий вход | `eska new/init/import/build/check/status/...`, `eska update` |

Точный интерфейс каждого установленного компонента показывает `--help`.
Аргументы самостоятельного project CLI и составных `eska new/init` различаются.
[Соответствие всех публичных команд](docs/user-guide.md#components).

## Документация и ограничения

[Руководство пользователя](docs/user-guide.md) · [Архитектура](docs/architecture.md) ·
[Выпуск и установка](docs/releases.md) · [Roadmap](docs/roadmap/README.md)

Основная приёмка выполнена на Linux/VSCodium и платформе 8.3.27.2325.
Наличие сборок других ОС не заменяет приёмку Windows, macOS, Microsoft VS Code
и Remote. Поддерживается Designer XML; EDT и экспериментальный формат ESKA
не используются в текущем цикле. Отладчик остаётся отдельной опциональной задачей.

[Apache License 2.0](LICENSE).
