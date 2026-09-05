# Build и development runtime

## T28 — Build subsystem через `ibcmd`

**Статус:** `NEXT`
**Зависит от:** T03, T19

Первый пользовательский результат — кроссплатформенная сборка Designer XML
проекта типа `configuration` в `.cf`:

```text
eska build
eska build --output <path>
```

За основу берётся проверенный pipeline из пользовательского `Taskfile.yml`:

```text
выбрать совместимый ibcmd
-> создать управляемую временную файловую базу
-> import Designer XML
-> save .cf
-> очистить только созданные текущим запуском временные файлы
```

Это compatibility path для используемой сейчас платформы `8.3.27.2325`. Перед
фиксацией backend нужно проверить встроенную справку каждой поддерживаемой версии:
если `ibcmd` умеет эквивалентный прямой импорт XML в artifact без временной базы,
он предпочтительнее, а pipeline через базу остаётся явным version-dependent
fallback.

Реализация не переносит Bash/Taskfile в продукт. Процессы запускаются через Rust
`Command`, пути передаются отдельными аргументами, а прерывание и cleanup работают
на Windows и Linux. Прямой запуск `ibcmd` — основной portable backend; Distrobox
остаётся опциональным Linux adapter, а не обязательной частью build core.

Контракт настройки должен разделять переносимые project settings и параметры
конкретной машины:

- требуемая версия платформы и каталог artifacts задаются в project config;
- путь к `ibcmd`, архитектура и Distrobox container переопределяются CLI/env или
  local machine config и не требуют hardcoded абсолютных путей в репозитории;
- поиск проверяет явный путь, `PATH` и стандартные пути установки, затем сверяет
  фактический `ibcmd --version`;
- output path по умолчанию выводится из имени проекта, но переопределяется явно;
- human output локализован, JSON result содержит artifact type/path, platform
  version и duration без credentials и host-specific secrets.

Сначала реализуется корректная полная сборка `.cf`. Поддержка нативных artifacts
остальных уже распознаваемых project types (`.cfe`, `.epf`, `.erf`) добавляется
последующими узкими шагами того же subsystem после проверки доступных команд на
минимальной поддерживаемой версии платформы. Generated `build/` не является
source of truth и игнорируется VCS; существующий artifact заменяется только после
успешной сборки.

T28 реализуется тремя законченными частями:

1. build settings, безопасный поиск/version check `ibcmd` и план artifact без
   запуска сборки;
2. `eska build` для `configuration` с реальным `.cf`, cleanup, RU/EN human output
   и стабильным JSON result; acceptance-сценарий повторяет результат текущего
   Taskfile на `8.3.27.2325`;
3. проверка и добавление нативных artifacts для `extension`, `processing` и
   `report` без изменения контракта полной сборки `.cf`.

## T42 — Patch-extension `.cfe` из разницы веток

**Статус:** `NEEDS-SPEC`
**Зависит от:** T20, T21, T28

Это отдельный artifact, а не режим обычной полной сборки. Для configuration
project команда должна брать base из `workflow.integration_target`, сравнивать
его merge-base с `HEAD` и позволять явный `--base <revision>`. Git branch не
хранит ссылку на «родительскую ветку», поэтому угадывать её по имени нельзя.

До утверждения CLI нужен feasibility prototype на минимальной поддерживаемой
версии `ibcmd`: создать extension с purpose `patch`, корректно заимствовать
изменённые объекты из base configuration, применить их версии из `HEAD` и
сохранить `.cfe`. Добавления, удаления, переименования, изменения корневых свойств
и несовместимые изменения структуры должны иметь явный support matrix и
отклоняться до записи artifact, если безопасная семантика не доказана.

`.cfe` — файл расширения конфигурации. Официальный файл обновления основной
конфигурации имеет формат `.cfu`; его генерация не входит в T42 и при
необходимости проектируется отдельной задачей.

## T29 — `eska doctor`

**Статус:** `PLANNED`  
**Зависит от:** T07, T28

Диагностировать config/source, требуемую и установленную 1С, `ibcmd`, repository,
remote/locking и development environment. По умолчанию read-only; fix/setup только
явным отдельным режимом.

## T30 — Development environments

**Статус:** `PLANNED`  
**Зависит от:** T28

`env list/create/use/reset` стандартизируют dev/test infobases и их project state.
Credentials нельзя хранить открытым текстом в `eska.toml`.

## T31 — `eska apply` / `eska run`

**Статус:** `PLANNED`  
**Зависит от:** T19, T20, T28, T30

`apply` определяет changed objects и выбирает минимально достаточный partial/full
update с DB update при необходимости. `run` запускает 1С для активного environment;
отдельный `designer` уточнить при проектировании CLI.
