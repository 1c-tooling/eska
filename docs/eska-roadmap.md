# eska — Product Specification and Development Roadmap

> Статус документа: рабочая спецификация для последовательной реализации `eska` через Codex.
>
> Этот документ описывает **что строим**, **в каком порядке**, **какое должно быть поведение CLI**, **какие архитектурные ограничения соблюдать** и **по каким критериям считать каждый этап завершённым**.

---

## 1. Что такое `eska`

`eska` — современный CLI-инструментарий для разработки проектов на платформе **1С:Предприятие**, написанный на Rust.

Главная цель — упростить повседневную разработку 1С и предоставить единый developer workflow поверх:

- системы контроля версий;
- исходников Designer XML;
- платформенных инструментов 1С;
- сборки;
- проверок;
- форматирования;
- версионирования;
- релизов;
- CI/CD;
- будущих IDE/GUI-интеграций.

`eska` не должна быть просто обёрткой над Git или набором shell-скриптов.

Пользователь должен работать с **намерениями уровня разработки**, например:

```text
начать задачу
посмотреть состояние проекта
сохранить изменения
синхронизироваться с командой
опубликовать изменения
завершить задачу
захватить конфликтный объект
проверить проект
собрать проект
создать релиз
```

а не постоянно собирать низкоуровневые команды Git, `ibcmd` и других инструментов вручную.

---

## 2. Текущие принятые решения

Эти решения считаются зафиксированными до отдельного пересмотра.

### 2.1. Язык реализации

- Rust.
- CLI строится на `clap`.
- Использовать современные idiomatic Rust practices.
- Не добавлять абстракции и crates «на будущее» без текущей необходимости.

### 2.2. Локализация

Локализация уже реализована.

Поддерживаемые языки:

```text
ru-RU
en-US
```

Правила:

- команды, опции, config keys, JSON fields и internal identifiers не локализуются;
- локализуются help, ошибки, warnings, progress и интерактивные сообщения;
- все новые пользовательские сообщения обязаны добавляться сразу в обе локали;
- machine-readable output должен быть одинаковым при любом языке.

### 2.3. Формат исходников 1С

На текущем этапе поддерживается **только Designer XML** — файловая выгрузка Конфигуратора.

Не реализовывать сейчас:

- EDT project format;
- `1cedtcli`;
- Java/EDT dependencies;
- автоматическое хранение двух представлений одного проекта.

Архитектура не должна намеренно блокировать поддержку других source formats в будущем, но не нужно создавать лишние интерфейсы для EDT заранее.

### 2.4. Сборка

Планируется сборка через `ibcmd`.

Новые возможности `ibcmd`, появившиеся в современных версиях платформы 1С, включая ветку 8.5, можно использовать по мере реализации build subsystem.

Не пытаться писать собственный replacement платформенного сборщика.

### 2.5. VCS

На первом этапе:

```text
eska
├── gix crate
└── system Git как документированный capability fallback
```

`gix` подключается как Rust crate и входит в бинарник `eska`.

Отдельная установка `gix` пользователю не требуется.

Для каждой VCS-операции сначала использовать `gix`, если закреплённая версия
сохраняет требуемые semantics и safety guarantees. System Git допустим только для
конкретно зафиксированного пробела возможностей, а не как общий путь для всех
network/mutating операций.

Не ставить отказ от Git самоцелью.

### 2.6. VCS UX

Основной CLI не должен копировать Git:

```text
НЕ:
eska git fetch
eska git checkout
eska git rebase
eska git push
```

Основной UX:

```text
eska clone
eska start
eska status
eska diff
eska save
eska sync
eska publish
eska finish
```

Конкретные Git-операции определяются workflow policy проекта.

### 2.7. Workflow

При создании/инициализации проекта пользователь выбирает типовую стратегию:

```text
trunk
git-flow
github-flow
custom
```

Типовые стратегии должны иметь встроенные разумные defaults.

Нестандартное поведение задаётся конфигурацией проекта.

Размер или тип проекта может влиять на предложенный default, но не должен жёстко определять workflow.

---

# 3. Основные архитектурные принципы

## 3.1. Не проектировать весь продукт заранее

Каждый этап должен добавлять минимальный необходимый слой.

Избегать:

- пустых модулей;
- traits без реальной второй реализации;
- generic abstractions без необходимости;
- dependency injection framework;
- plugin system до появления реального plugin use case.

Разрешается проектировать небольшие extension points только там, где уже известно, что следующий этап на них опирается.

## 3.2. Core не должен зависеть от CLI presentation

Бизнес-логика не должна:

- печатать локализованные строки напрямую;
- парсить terminal output;
- принимать решения на основании цветов/TTY;
- зависеть от `clap`.

CLI отвечает за:

- аргументы;
- локализованный human output;
- prompts;
- exit codes;
- выбор human/json presentation.

## 3.3. Structured errors

По мере роста проекта ошибки должны иметь структурированное внутреннее представление.

Не писать в глубине core:

```rust
anyhow!("Проект не найден")
```

Предпочитать ошибки уровня предметной области:

```text
ProjectNotFound
RepositoryNotFound
WorkflowConflict
LockAlreadyHeld
```

а локализованный текст формировать на presentation layer.

## 3.4. Human и machine output

Все команды, полезные для CI/IDE/GUI, со временем должны поддерживать:

```text
--format human
--format json
```

Human output может локализоваться.

JSON:

- не локализуется;
- имеет стабильные имена полей;
- должен быть пригоден для VS Code extension и CI.

## 3.5. Безопасность важнее сокращения команд

Нельзя скрывать destructive behavior настолько, чтобы пользователь не понимал последствия.

Для операций, способных потерять работу:

- выполнять preflight checks;
- по возможности давать preview;
- требовать явный `--force`, если безопасного пути нет;
- не использовать `reset --hard` как универсальное решение.

---

# 4. Целевая модель проекта

В перспективе core должен оперировать сущностью `Project`.

Минимально:

```text
Project
├── root
├── type
├── source
├── source_format
├── configuration
├── vcs
└── workflow
```

`Project` остаётся одной независимо собираемой и версионируемой
единицей: конфигурацией, расширением, внешней обработкой или
отчётом. Не добавлять в него несколько `source`.

Для монорепозитория над проектами вводится отдельная сущность
`Workspace`:

```text
Git repository
└── Workspace
    ├── Project: report
    ├── Project: processing
    └── Project: processing
```

Workspace управляет составом проектов, общими build defaults и
repository-wide workflow. Каждый member сохраняет собственный `Project`
и корневой Designer XML descriptor.

## 4.1. Типы проектов

Поддержать:

```text
configuration
extension
processing
report
```

Соответствие:

- `configuration` — конфигурация;
- `extension` — расширение конфигурации;
- `processing` — внешняя обработка;
- `report` — внешний отчёт.

## 4.2. Source format

Пока только:

```text
designer-xml
```

В коде допустимо представлять это как enum с одним вариантом, если это не создаёт лишнюю сложность.

---

# 5. Конфигурация проекта

Файл проекта:

```text
eska.toml
```

Он хранит только **проектные** настройки.

Пользовательские настройки, например язык интерфейса, не должны храниться в project config.

## 5.1. Требования

Конфигурация должна:

- быть компактной;
- иметь разумные defaults;
- не дублировать defaults без необходимости;
- быть расширяемой;
- иметь понятные diagnostics;
- корректно валидироваться.

Пример будущего файла:

```toml
[project]
type = "configuration"
source = "src"
source_format = "designer-xml"

[vcs]
main_branch = "main"

[vcs.workflow]
preset = "trunk"

[format]
line_width = 120
```

Не считать этот пример окончательной схемой — API config следует стабилизировать постепенно.

Будущий workspace использует virtual root manifest с `[workspace]` и
отдельные `eska.toml` у members. Секции `[project]` и `[workspace]` в
одном manifest в первой версии взаимоисключающие. Одиночный
существующий `[project]` остаётся валидным без миграции. Полный
контракт зафиксирован в [workspace roadmap](roadmap/11-workspaces.md).

---

# 6. Порядок реализации

---

# Milestone 0 — Clean baseline

## Статус

Завершён.

Проект очищен от старых экспериментальных команд и неиспользуемых зависимостей.

Локализация `ru-RU` / `en-US` реализована.

Перед продолжением Codex должен проверить фактическое текущее состояние репозитория, а не предполагать его по этому документу.

---

# Milestone 1 — Project model и `eska.toml`

## Цель

Научить `eska` находить, загружать и валидировать проект.

## Реализовать

### Project discovery

CLI, запущенный:

```text
/project
/project/src
/project/src/CommonModules/...
```

должен находить ближайший вверх по дереву:

```text
eska.toml
```

и считать его root проекта.

### Project config

Реализовать:

- загрузку TOML;
- parse;
- validation;
- defaults;
- понятные локализованные ошибки.

### Project types

Ввести типы:

```text
configuration
extension
processing
report
```

### Source

Пока:

```text
designer-xml
```

### Внутренний API

Должна появиться простая модель проекта, которой смогут пользоваться следующие команды.

## Не реализовывать

- Git operations;
- project creation;
- templates;
- XML parser всей конфигурации;
- build;
- fmt.

## Критерии готовности

- проект корректно находится из вложенных директорий;
- неправильный `eska.toml` даёт понятную ошибку;
- неизвестный project type отклоняется;
- source directory валидируется;
- unit/integration tests проходят;
- human messages локализованы;
- core не зависит от локализованных строк.

---

# Milestone 2 — `eska new`, `eska init`, `eska clone` и templates

Это первый пользовательски значимый milestone.

## 2.1. `eska new`

Назначение:

> создать новый проект 1С, сразу готовый к работе с `eska`.

Пример:

```text
eska new my-project
```

Интерактивно выбрать:

```text
Тип проекта:
  Конфигурация
  Расширение
  Внешняя обработка
  Внешний отчёт

Workflow:
  Trunk-based
  Git Flow
  GitHub Flow
```

Неинтерактивный вариант обязателен:

```text
eska new my-project \
    --type configuration \
    --workflow trunk
```

## 2.2. `eska init`

Назначение:

> подключить `eska` к существующей Designer XML выгрузке.

Команда должна:

1. найти/проверить исходники;
2. определить максимально возможные параметры автоматически;
3. запросить только то, что нельзя определить;
4. создать `eska.toml`;
5. предложить VCS workflow;
6. при необходимости инициализировать repository.

`init` не должен повреждать существующий проект.

## 2.3. Built-in templates

Встроенные шаблоны:

```text
configuration
extension
processing
report
```

Шаблон должен быть минимальным.

Не генерировать большое количество CI/config файлов без явного запроса.

Базово допустимы:

```text
eska.toml
.gitignore
.gitattributes
src/
README.md
```

если конкретный template действительно нуждается в них.

## 2.4. Template architecture

Сразу предусмотреть возможность будущих:

```text
custom templates
company templates
remote templates
```

но НЕ реализовывать remote registry сейчас.

## 2.5. Git init

По умолчанию `new` может предложить/выполнить VCS initialization.

Должен существовать escape hatch:

```text
--no-vcs
```

## 2.6. `eska clone`

Назначение:

> клонировать существующий готовый проект `eska` и сразу проверить его контракт.

```text
eska clone <repository> [directory]
```

Первая версия использует `gix` для clone/fetch/checkout и принимает URL или
локальный путь. HTTP(S) transport встроен и не требует system Git; локальный и
SSH transport наследуют фактическое поведение upstream `gix clone` и используют
protocol helpers `git-upload-pack`/`ssh`. Каталог назначения должен отсутствовать.
После checkout выполняются обычные discovery и validation `eska.toml`; при
ошибке удаляется только каталог, созданный текущим запуском.

`clone` не совмещается с `init`, не исправляет чужой проект и пока не добавляет
shallow clone, выбор ветки и submodules. Имя remote по умолчанию — `origin`, с
явной возможностью выбрать другое имя.

## Критерии готовности

```text
eska new ...
eska init ...
eska clone ...
```

создают валидный проект, который затем успешно загружается через Project model.

---

# Milestone 3 — Repository layer

## Цель

Добавить базовую работу с Git repository без workflow automation.

## Основной принцип

`gix` — основной backend каждой VCS-операции, для которой закреплённая версия
сохраняет требуемое поведение и гарантии безопасности.

System Git использовать только как capability fallback для конкретно
зафиксированной недостающей возможности. Не повторять им произвольную ошибку
`gix` и не считать все network/mutating операции fallback-категорией.

## На первом этапе через `gix` желательно реализовать

- repository discovery;
- HEAD;
- current branch;
- refs;
- worktree/index status;
- changed paths;
- commit history basics;
- merge-base, если нужен следующему milestone;
- clone/fetch и безопасные ref updates по мере появления потребителя.

## System Git fallback

Допустим для пока не покрытой высокоуровневой orchestration worktree/index,
rebase/merge/stash, hooks/editor/signing, Git LFS и конкретных неподдержанных
transport/credential сценариев. Push оценивается заново в задаче `publish`.

Прямые `Command::new("git")` не должны быть размазаны по commands.

Сделать небольшой infrastructure layer.

Каждая задача, оставляющая system Git, должна перечислить вызовы и объяснить,
какой контракт пока нельзя равноценно сохранить через `gix`.

## Не делать

- отдельную установку `gix`;
- shell parsing;
- парсинг локализованного human output Git, если доступен machine format;
- собственную реализацию Git object database.

---

# Milestone 4 — Workflow policy engine

## Цель

До реализации `start/sync/publish` формализовать branching policy проекта.

Типовые presets:

```text
trunk
git-flow
github-flow
```

и возможность:

```text
custom
```

## Workflow должен описывать минимум

- base branch;
- working branch policy;
- task branch naming;
- sync strategy;
- integration target;
- publish behavior;
- finish behavior.

## Примеры

### Trunk

```text
base: main
working branch: short-lived
sync: rebase on origin/main
publish: push task branch
integration: MR -> main
```

Опционально позднее:

```text
direct trunk
```

без feature branch.

### Git Flow

```text
main
develop
feature/*
release/*
hotfix/*
```

Обычная задача:

```text
feature/* from develop
```

Hotfix:

```text
hotfix/* from main
```

### GitHub Flow

```text
main
short-lived branch
MR/PR -> main
```

## Custom

Custom config должен позволять переопределять типовой preset без создания нового Rust enum для каждого корпоративного workflow.

---

# Milestone 5 — Основной VCS UX

После этого milestone `eska` должна быть пригодна для повседневной командной разработки.

Основной пользовательский цикл:

```text
status
start
diff
save
sync
publish
finish
```

`status`, `start`, `diff` и `save` уже образуют локальный baseline. `clone`,
gix-first миграция, history и semantic model также завершены. Ближайший порядок
теперь замыкает практический цикл через `switch`, локальный `finish` и `build`;
`sync`, `publish`, locking и test backend возвращаются после проверки MVP
согласно разделу 14.

---

## 5.1. `eska status`

Это НЕ копия `git status`.

Команда показывает состояние **работы над проектом**.

Пример:

```text
Проект      Billing
Workflow    Git Flow
Задача      FI-1234
Ветка       feature/FI-1234
База        develop

Изменения
  5 изменено
  1 добавлено

Синхронизация
  локально +2
  удалённо +0

Захваты
  1 объект

Состояние
  ✓ можно сохранять
  ✓ можно публиковать
```

Нужен:

```text
--format json
```

JSON считается API и тестируется.

---

## 5.2. `eska start <task>`

Семантика:

> начать работу над задачей в соответствии с workflow policy проекта.

Пример:

```text
eska start FI-1234
```

### Trunk

Примерно:

```text
fetch
update main
create short-lived branch
switch
register current task
```

### Git Flow

Примерно:

```text
fetch
update develop
create feature/FI-1234 from develop
switch
register task
```

### Hotfix

В будущем:

```text
eska start FI-1234 --kind hotfix
```

должен использовать policy для hotfix.

### Preflight

`start` не должен автоматически уничтожать незавершённые изменения.

При dirty workspace:

- отказ;
- или безопасный предложенный путь;
- shelve будет добавлен позже.

---

## 5.3. `eska diff`

Сначала реализовать нормальный file-level diff/status.

Не пытаться сразу делать semantic 1C diff.

Но API команды должен позволять позже перейти к object-aware representation.

Допустимые режимы:

```text
eska diff
eska diff --raw
eska diff --format json
```

---

## 5.4. `eska save`

Семантика:

> сохранить выбранный ChangeSet в историю VCS.

Не привязывать public UX к обязательному знанию Git staging area.

Базовые сценарии:

```text
eska save
eska save -m "..."
```

В перспективе:

```text
eska save --interactive
eska save --auto
```

### Базовое правило

Commit message должен описывать **именно тот ChangeSet, который входит в commit**.

Не анализировать unstaged changes, если они не входят в save.

### Редактор

Если `-m` не указан, первое время допустимо открывать configured editor.

Позже будет генерация черновика commit message.

---

## 5.5. `eska sync`

Семантика:

> синхронизировать текущую работу с актуальным состоянием командной базовой линии в соответствии с workflow.

Пользователь не должен выбирать `fetch/rebase/merge` вручную для типового сценария.

Если remote из workflow policy существует в repository, `sync` сначала получает
его состояние через gix-first network layer. Если remote не настроен, fetch не
выполняется: источником синхронизации служит локальная base.

Пример Git Flow:

```text
feature/FI-1234
  rebase onto origin/develop
```

Trunk:

```text
task branch
  rebase onto origin/main
```

При конфликте:

```text
Synchronization stopped.

Conflicts:
...

Resolve and run:
  eska continue

Cancel:
  eska abort
```

`sync`, `continue` и `abort` реализуются одной задачей. Нельзя выпускать
конфликтующий `sync` без штатного пути продолжить или отменить операцию.

---

## 5.6. `eska publish`

Семантика:

> сделать текущую сохранённую работу доступной команде.

На первом этапе:

- preflight;
- sync requirement согласно policy;
- push;
- set upstream при необходимости.

Позже:

- MR/PR creation;
- checks;
- provider integration.

---

## 5.7. `eska finish`

Семантика:

> завершить локальную работу над задачей.

Проверки:

- нет unsaved changes;
- работа опубликована/интегрирована согласно policy;
- нет опасных локальных состояний.

Действия могут включать:

```text
release locks
switch to base
update base
remove local task branch
clear task state
```

Не удалять remote branch без явной policy.

---

# Milestone 6 — Незавершённые и безопасные VCS-операции

Реализовать:

```text
eska restore
eska switch
eska shelve
eska unshelve
eska shelves
eska history
```

## `continue` / `abort`

Унифицированное управление незавершённой workflow-операцией.

Первая реализация поставляется вместе с `sync`, а не отдельной следующей задачей.

Не заставлять пользователя помнить:

```text
git rebase --continue
git merge --abort
...
```

## `switch`

Переключение между задачами/workspaces.

Dirty state должен обрабатываться безопасно.

## `shelve`

Понятие «отложить изменения» подходит разработчикам 1С.

Первоначально может использовать Git stash как backend, но public API не должен зависеть от термина `stash`.

## `restore`

Destructive behavior должно иметь понятные safeguards.

## `history`

Сначала обычная история commits/tasks.

Позже object-aware history.

---

# Milestone 7 — Locking конфликтных объектов 1С

## Цель

Дать Git-first workflow возможность монопольного захвата плохо сливаемых структурных объектов.

Пользователь работает с логическими объектами:

```text
eska lock Document.ЗаказКлиента.Form.ФормаДокумента
eska unlock Document.ЗаказКлиента.Form.ФормаДокумента
eska locks
```

а не с XML path.

## Типичные lockable objects

- Form;
- Role;
- DataCompositionSchema;
- другие структурные metadata objects по configuration policy.

## Backend

Первоначально можно использовать Git LFS locking / server lock API.

Не использовать Git LFS storage для Designer XML только ради locking, если это уничтожает обычный diff/blame.

Конкретный lock backend не должен быть частью public CLI.

## Поведение unlock

Если есть неопубликованные изменения lockable object:

- не отпускать lock молча;
- объяснить причину;
- предложить publish/save;
- `--force` только как явный escape hatch.

---

# Milestone 8 — Designer XML project model

Это один из ключевых технических milestones.

## Цель

`eska` должна понимать не только файлы, но и логические объекты 1С.

Например:

```text
Document.ПоступлениеНаСчетТекущихРасчетов
Document.ПоступлениеНаСчетТекущихРасчетов.Form.ФормаДокумента
CommonModule.ИдентификацияПлатежей
```

## Реализовать

- object discovery;
- mapping logical object -> physical paths;
- reverse mapping changed paths -> logical objects;
- metadata type;
- object name;
- module paths;
- form paths;
- stable internal ObjectId.

## Performance

На больших конфигурациях нельзя каждый раз бессмысленно обходить и парсить всё.

Допускается добавить:

```text
.eska/
  cache/
  index/
  state/
```

только когда это реально необходимо.

Кэш не хранится в VCS.

---

# Milestone 9 — Semantic change model

Ввести промежуточную сущность:

```text
ChangeSet
    ↓
SemanticChangeAnalyzer
    ↓
ChangeSummary
```

Она должна использоваться несколькими функциями, а не быть кодом только для commit message.

Будущие потребители:

- `eska diff`;
- locks;
- commit message generator;
- changelog;
- affected analysis;
- CI.

## Первый semantic diff

Начать с простых надёжных случаев:

- объект добавлен/удалён/изменён;
- module changed;
- method/function changed;
- form changed;
- metadata attribute changed.

Не пытаться сразу построить идеальный three-way semantic merge engine.

---

# Milestone 10 — Генерация commit message

## Основной сценарий

```text
eska save
    ↓
exact ChangeSet
    ↓
semantic summary
    ↓
generated draft
    ↓
editor
    ↓
commit
```

Пример:

```text
feat(identification): добавить отмену идентификации

- Добавлена обработка отмены идентификации.
- Изменена форма документа.
- Изменено формирование движений.
```

## Режимы

```text
eska save
```

генерация -> editor -> commit.

```text
eska save -m "..."
```

явное сообщение без генерации.

Позже:

```text
eska save --auto
```

генерация -> commit.

## AI

AI не является обязательной частью.

Первый generator должен быть deterministic/semantic.

Позже допускается provider:

```text
semantic
ai
auto
```

Предпочтительный AI pipeline:

```text
raw diff
  ↓
eska semantic summary
  ↓
small structured context
  ↓
AI text refinement
```

Не отправлять огромный raw Designer XML diff без необходимости.

## Git compatibility

Позже можно добавить:

```text
eska hooks install
```

и использовать `prepare-commit-msg`, чтобы обычный `git commit` также мог получать draft от `eska`.

---

# Milestone 11 — `fmt`

Только после формирования project/source model.

## Требования

```text
eska fmt
eska fmt <scope>
eska fmt --check
```

Formatter:

- deterministic;
- быстрый;
- пригодный для CI;
- одинаковый результат независимо от среды;
- работает только с поддерживаемыми исходниками.

## Scope

В перспективе:

```text
eska fmt
eska fmt CommonModule.Платежи
eska fmt Document.ЗаказКлиента
```

## Не делать

Не форматировать XML regex-ами, если структура требует parser-aware изменений.

---

# Milestone 12 — `check`

`check` — агрегатор проверок проекта.

Пример:

```text
Formatting        ✓
Project           ✓
Designer XML      ✓
BSL diagnostics   ✓
VCS policy        ✓
```

Не переписывать существующие зрелые анализаторы только ради собственного бренда.

Допустима интеграция с BSL Language Server и другими внешними анализаторами.

Обязателен:

```text
--format json
```

---

# Milestone 13 — Build subsystem

## Основная команда

```text
eska build
```

Пользователь не должен поддерживать большой custom Taskfile/bash pipeline для типового проекта.

## Первый backend

Designer XML + `ibcmd`.

Примерный pipeline:

```text
Designer XML
    ↓
temporary infobase / build environment
    ↓
ibcmd import
    ↓
database/config update as required
    ↓
artifact
```

Тип artifact зависит от project type:

- configuration;
- extension;
- processing;
- report.

Перед реализацией проверять актуальные возможности текущих версий `ibcmd`.

Не копировать устаревший pipeline автоматически, если новая платформа умеет операцию проще.

## Build directory

Generated output не является source of truth.

Типично:

```text
build/
```

должен быть VCS-ignored.

---

# Milestone 14 — Development environments

Реализовать после стабильной сборки.

Возможные команды:

```text
eska env list
eska env create
eska env use
eska env reset
```

Цель — стандартизировать dev/test infobases.

Credentials не хранить в project config в открытом виде.

---

# Milestone 15 — Быстрый development loop

Очень важный UX milestone.

## `eska apply`

Назначение:

> применить текущие изменения Designer XML к development infobase.

В перспективе:

- определить changed objects;
- выбрать partial/full update;
- выполнить минимально достаточное действие;
- обновить DB configuration при необходимости.

Пример:

```text
Changed:
  CommonModule.Идентификация
  Document.Платеж.Form.ФормаДокумента

Applying...
  modules       ✓
  metadata      ✓
  database      ✓
```

## `eska run`

Запустить 1С с environment текущего проекта.

Возможные варианты:

```text
eska run
eska designer
```

Точные команды определить при реализации.

---

# Milestone 16 — Workspace per task

Опциональный, но полезный advanced workflow.

Пример:

```text
eska start FI-1234 --workspace
```

Может создать:

```text
task
├── Git worktree
├── isolated build directory
└── isolated dev infobase
```

Цель — одновременно держать несколько задач без постоянного переключения одной ИБ между состояниями конфигурации.

Не реализовывать раньше обычного workflow.

---

# Milestone 17 — `affected` и dependency analysis

После появления semantic model.

Команда:

```text
eska affected
```

Пример:

```text
Changed
  CommonModule.ИдентификацияПлатежей

Potentially affected
  Document.ПоступлениеНаСчетТекущихРасчетов
  Document.БанковскийРеестр

Tests
  PaymentIdentification
```

Использование:

```text
eska check --affected
eska test --affected
```

Цель — масштабирование на крупные конфигурации.

---

# Milestone 18 — Versioning

Команды:

```text
eska version
eska version bump patch
eska version bump minor
eska version bump major
```

Позже:

```text
eska version bump auto
```

Auto strategy может учитывать:

- Conventional Commits;
- semantic changes;
- project policy.

Не смешивать версию самого бинарника `eska` и версию 1С-проекта.

Версию `eska` продолжает менять release automation проекта `eska`.

Реализованный T25 читает четырёхкомпонентную версию из прямого
`Properties/Version` корневого объекта Designer XML. `bump major|minor|patch`
меняет соответственно revision, subrevision или version и начинает младшие
компоненты заново; build начинается с 1. XML не сериализуется: меняются только
байты значения версии, остальной файл сохраняется побайтно. Human output
локализован, JSON использует стабильную схему версии 1. Автоматический выбор bump
остаётся частью будущего release pipeline.

---

# Milestone 19 — Release

Команда:

```text
eska release
```

Возможный pipeline:

```text
determine version
↓
validate project
↓
update project version
↓
changelog
↓
commit
↓
tag
↓
build
↓
artifacts
```

Каждый шаг должен быть policy/config driven.

Поддержать dry-run до любых destructive/write действий.

---

# Milestone 20 — CI/CD integration

`eska` не является CI server.

Она предоставляет одинаковые команды локально и в CI:

```text
eska fmt --check
eska check
eska test
eska build
```

Будущая команда:

```text
eska ci init
```

может создавать thin adapters для:

- GitLab CI;
- GitHub Actions;
- других provider'ов позже.

CI config должен содержать минимум business logic.

---

# Milestone 21 — IDE / GUI integrations

После стабилизации CLI и JSON protocol.

## VS Code

Первый GUI frontend.

Extension должна быть тонкой.

Возможности:

- project status;
- start/sync/publish;
- lock/unlock object;
- lock status;
- diagnostics;
- command palette;
- status bar.

Не создавать отдельную VCS implementation в TypeScript.

## EDT

Только позднее, если появится спрос.

## Standalone GUI

После CLI/VS Code.

Все frontends используют тот же core/protocol.

---

# Milestone 22 — Project workspaces и монорепозиторий

Поддержать один Git-репозиторий с несколькими независимыми
проектами 1С. Основной сценарий — набор внешних отчётов и
обработок в каталогах `src/<member>`.

Целевой UX:

```text
eska build                         # все members из workspace root
eska build -p sales-report         # один member
eska build --workspace             # все members из вложенного каталога
eska version -p sales-report
eska version -p sales-report bump patch
eska new import-orders --type processing
cd src/legacy-orders && eska init   # для уже скопированной выгрузки
```

Project-команды работают с выбранными members; repository workflow остаётся
общим. `new` создаёт `src/<name>`, а `init` подключает существующий каталог и обе
команды обновляют явный список `workspace.members`. Массовый version bump без
явного флага запрещён. Существующие single-project CLI и JSON contracts
сохраняются.

Этот workspace не связан с isolated Git worktree для отдельной задачи из
Milestone 16. Детальная декомпозиция T44–T48:
[project workspaces](roadmap/11-workspaces.md).

---

# 7. Дополнительные команды и поведение

## `eska doctor`

Следующая задача T29 — read-only диагностика текущего CLI. За один запуск
проверять project/workspace config, source и корневой Designer XML descriptor,
требуемую и установленную платформу, `ibcmd`, runner, repository/workflow и Git.
Требование `1cv8` относится к `patch`, Git LFS — к реально использующим его
attributes; отсутствие remote допустимо для локального workflow.

Собирать независимые результаты после ошибок, объяснять пропущенные проверки и
давать конкретный способ исправления с указанием затронутой команды. Human output
доступен на RU/EN, JSON имеет стабильные check IDs, статусы и коды ошибок.
Проверка наличия descriptor не подменяет сборку и проверку BSL платформой.

Locking, development databases, установка инструментов и `fix/setup` не входят
в первый `doctor`. Он не выполняет fetch/push и не меняет config, source или
repository. Детальные границы и приёмка: [T29](roadmap/09-build-and-runtime.md).

---

## `eska setup`

Поздний onboarding helper:

```text
clone
cd project
eska setup
```

Возможные действия:

- проверить toolchain;
- установить project hooks;
- подготовить dev environment;
- импортировать конфигурацию;
- выполнить необходимые project setup steps.

Не делать package manager для всей ОС.

---

# 8. UX rules

## 8.1. Основные команды должны быть короткими

Предпочитать:

```text
eska start
eska sync
eska save
eska publish
```

а не:

```text
eska vcs workflow sync-current-task
```

Namespaces использовать только если действительно улучшают структуру редких/administrative функций.

## 8.2. Defaults должны покрывать 80–90% сценариев

Пользователь не должен каждый раз указывать:

```text
--base main
--strategy rebase
--remote origin
--branch feature/...
```

Это свойства policy проекта.

## 8.3. Advanced users сохраняют escape hatch

Пока backend Git:

```text
git ...
```

можно использовать напрямую.

`eska` не должна ломать стандартный Git repository нестандартным закрытым форматом.

## 8.4. Dry run

Для текущих `save` и `build` запланированы отдельные задачи T51 и T52:

```text
eska save --dry-run
eska build --dry-run
```

`save` должен показывать точный scope и сообщение без staging, editor/hooks и
commit. `build` — план проектов, platforms/runners и outputs без создания базы
или artifacts; чтение версии инструмента допускается. Preview использует общую
с исполнением подготовку, но не заменяет повторный preflight при следующем
запуске. Эти режимы пока не реализованы; работающий `patch --dry-run` сохраняется.
Для остальных сложных операций также оценивать единообразный `--dry-run`.

---

# 9. Performance requirements

`eska` ориентируется в том числе на большие конфигурации.

Поэтому при реализации каждой функции проверять:

1. Нужно ли обходить весь проект?
2. Можно ли использовать Git changed paths?
3. Можно ли обработать только changed objects?
4. Можно ли кэшировать стабильную project model?
5. Можно ли избежать subprocess?
6. Можно ли выполнять работу параллельно безопасно?

Не делать premature optimization.

Но нельзя архитектурно привязать `status`, `diff` или `save` к обязательному полному parse всей конфигурации.

---

# 10. Testing strategy

Каждый milestone должен иметь автоматические тесты.

## Unit tests

Для:

- config parsing;
- workflow resolution;
- locale-independent internal logic;
- path/object mapping;
- version logic;
- semantic analysis.

## Integration tests

CLI:

```text
eska ...
```

проверяется как внешний executable.

Тестировать:

- exit codes;
- stdout/stderr;
- Russian human output;
- English human output;
- JSON schema.

## Repository tests

Создавать временные Git repositories и реальные commit graphs.

Не mock'ать Git там, где небольшой настоящий repository даёт более надёжный тест.

## Golden tests

Можно применять осторожно для:

- help;
- semantic diff;
- formatted output.

Не использовать огромные brittle snapshots.

---

# 11. Definition of Done для каждого Codex task

Codex не должен заканчивать задачу после того, как код «примерно работает».

Каждый implementation task должен:

1. изучить текущую архитектуру;
2. реализовать только scope текущего milestone;
3. не начинать следующий milestone;
4. обновить обе локализации для всех новых human strings;
5. добавить/обновить тесты;
6. выполнить:

```text
cargo fmt --check
cargo check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

7. проверить relevant CLI вручную;
8. удалить ставший ненужным код/dependencies;
9. не оставлять закомментированный legacy code;
10. дать отчёт:
   - что реализовано;
   - какие решения приняты;
   - какие файлы изменены;
   - какие tests/checks пройдены;
   - какие ограничения остались.

Версию crate `eska` вручную не менять, если release/version automation проекта отвечает за неё.

---

# 12. Правила для Codex при неоднозначности

Если реализация допускает несколько вариантов:

1. сначала ориентироваться на эту спецификацию;
2. затем на существующую архитектуру репозитория;
3. выбирать минимальное решение, которое не блокирует следующий известный milestone;
4. не добавлять speculative abstractions;
5. не менять публичный CLI без необходимости;
6. не вводить новую dependency, если задача разумно решается уже имеющимися средствами.

Если обнаружена проблема, требующая изменения ранее принятой архитектуры, Codex должен явно описать её, а не молча перестраивать весь проект.

---

# 13. Приоритеты roadmap

Локальный CLI MVP, версия проекта и workspace уже реализованы. После анализа
текущего функционала от 2026-09-08 приоритет — диагностика, сохранность machine
contracts, масштабирование semantic-анализа и предсказуемость сборки:

```text
P0 — надёжность существующего CLI
T29 doctor
T49 JSON-ошибки и обратимые пути
T50 анализ затронутых объектов и явный fallback

P1 — управляемость текущих операций
T51 save --dry-run
T52 build --dry-run
T53 finish после squash/rebase: сначала спецификация
T54 паспорт артефакта и фактический снимок исходников
T55 переносимые проверки сборки

P2 — следующее крупное расширение VCS
T37 sync / continue / abort одной задачей

После P0–P2, последней в текущей очереди
T26 fmt
```

Остальной backlog сохраняется: affected/check, environments, apply/run,
release/CI helpers, shelve/restore, publish и VS Code. T23 test backend и T39
locking остаются `DEFERRED`; новые source formats и GUI не входят в текущий scope.
У `check`, release и CI сохраняются собственные зависимости от quality-этапов.

System Git orchestration остаётся capability fallback через существующий
infrastructure layer. Исправления текущего CLI не разрешают изменять GitHub
Actions, publication pipeline или добавлять соседние возможности без задачи.

---

# 14. Ближайший порядок задач после текущего состояния

Текущий baseline — завершены T01–T22, T25, T28–T29, T34, T40, T42–T49:

```text
Project/config/discovery
new/init/templates
repository/workflow policies
status/start/diff/save/history
Designer XML model/semantic diff/commit draft
switch/finish
build .cf/.cfe/.epf/.erf и ограниченный patch-extension
project versioning и workspace members
```

Ближайшие задачи выполнять в таком порядке, если не принято новое решение:

```text
1. T50 — анализ затронутых объектов и явный fallback, IN-PROGRESS
2. T51 — предварительный просмотр save
3. T52 — предварительный просмотр build
4. T53 — finish после squash/rebase, NEEDS-SPEC до реализации
5. T54 — паспорт собранного артефакта
6. T55 — переносимые проверки сборки
7. T37 — sync / continue / abort
8. T26 — fmt, последняя задача текущей очереди
```

Новые контракты, зависимости и критерии готовности приведены в
[T49–T55](roadmap/12-current-functionality.md), общий реестр — в
[трекере](roadmap/README.md). T53 сначала требует спецификации доказательств
интеграции и сохранения ветки; её уточнение не блокирует независимые T54–T55.
Завершённый `finish` до этого сохраняет консервативную ancestry-проверку.

Каждый пункт выполняется только в отдельно запрошенном scope. Обновление roadmap
не запускает реализацию очереди. Один законченный пользовательский результат —
отдельное логическое изменение с релевантными проверками.

---

# 15. Каким должен быть конечный пользовательский опыт

Для небольшого проекта:

```text
eska new my-extension
cd my-extension

eska start TASK-1

# разработка

eska save
eska sync
eska publish
eska finish
```

Для большой конфигурации:

```text
eska start FI-1234
eska lock Document.Платеж.Form.ФормаДокумента

# разработка

eska diff
eska save
eska sync
eska check
eska build
eska publish
eska finish
```

Для монорепозитория внешних отчётов и обработок:

```text
eska version -p sales-report bump patch
eska build -p sales-report
eska build
```

В будущем быстрый локальный цикл:

```text
eska start FI-1234

# код

eska apply
eska run

# код

eska save
eska publish
```

Идеальная onboarding-схема:

```text
eska clone ...
cd project
eska setup
eska start TASK-123
```

---

# 16. Ключевая продуктовая идея

`eska` должна стать не «ещё одной CLI-обёрткой над Git», а **единым developer interface для проектов 1С**.

Низкоуровневые механизмы могут со временем меняться:

```text
system Git fallback -> gix при эквивалентных гарантиях
Git LFS locks -> другой lock backend
старый ibcmd pipeline -> новые возможности 1С 8.5+
ручной diff -> semantic diff
ручной commit message -> generated draft
```

Но пользовательский workflow должен оставаться стабильным:

```text
new
init
start
status
diff
save
sync
publish
finish
lock
check
build
release
```

Именно стабильность этого высокоуровневого интерфейса является одной из главных архитектурных целей проекта.
