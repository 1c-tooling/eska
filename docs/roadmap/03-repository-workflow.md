# Repository и workflow policy

## T07 — Repository layer

**Статус:** `DONE`
**Зависит от:** T03

Создать единый infrastructure layer. Через `gix` реализовать discovery, HEAD,
branch/refs, worktree/index status, changed paths, базовую историю и merge-base по
мере реальной необходимости. System Git оставить fallback для network/mutating,
credentials, signing и LFS операций. Не размазывать `Command::new("git")` по CLI.

**Готово, когда:** реальные временные repositories покрывают detached HEAD,
unborn branch, dirty state и простой commit graph; core возвращает структурированные
данные, а не Git human output.

**Реализовано:** `vcs::repository::Repository` открывает ближайший рабочий
репозиторий через общую с `init` границу `vcs/git.rs`, возвращает HEAD, полные refs
и ограниченную историю. `vcs/status.rs` разделяет HEAD/index/worktree, конфликты,
intent-to-add и untracked; changed paths получаются из готового результата.
Core не зависит от CLI, TTY или локализации. Имена и пути сохраняют исходные байты.

**Решения и границы:**

- `gix 0.80.0` сохранён, включён только дополнительный feature `status`;
- discovery учитывает `.git`-файлы и linked worktrees, отвергает bare и
  повреждённый ближайший Git; применяет только локальную конфигурацию репозитория;
- refs отсортированы по полному имени, symbolic targets не теряются;
- история обходит родителей в ширину от HEAD с лимитом, без собственного графа
  или парсинга вывода Git;
- status не записывает index, игнорирует stat-only refresh и ignored-файлы;
  перемещения представлены удалением/добавлением без similarity scan;
- добавлена защита от паники `gix-index 0.48` на усечённом основном index;
  атомарность относительно внешних изменений не обещается;
- несовместимые транзитивные версии `hashbrown`/`foldhash` задокументированы
  в адресных исключениях существующего `clippy.toml`;
- merge-base и исполнитель системного Git отложены до конкретного потребителя;
  network/mutating, credentials, signing и LFS-команды остаются будущими
  операциями внутри `vcs/`; заготовки исполнителя не добавлены;
- CLI и config не менялись, новых пользовательских строк и JSON-схем нет.

**Проверено:** `cargo fmt --check`, `cargo check`,
`cargo clippy --all-targets --all-features -- -D warnings`,
`ESKA_TEST_ROOT=/home/kas/Projects/eska-playground cargo test` — успешно
(47 unit, 53 integration). Реальные Git-сценарии покрывают unborn/detached HEAD,
packed и symbolic refs, linked worktree, merge graph, staged/unstaged, untracked,
ignored, конфликт, intent-to-add, повреждённый index и не-UTF-8 пути. Дочерний
процесс проверяет чтение без Git в PATH и с перенаправляющим окружением.
Ручные `new`/`init` на RU/EN выполнены в отдельных временных каталогах playground;
подключение проекта внутри родительского Git сохраняет его метаданные побайтно.

## T08 — Workflow policy model

**Статус:** `DONE`
**Зависит от:** T02, T07

Модель определяет base branch, working branch policy, task naming, sync strategy,
integration target, publish и finish behavior. `custom` переопределяет preset через
config, а не через новый Rust enum на каждую компанию.

**Реализовано:** `vcs::workflow::WorkflowSettings`, `PolicyOverrides`,
`WorkflowPolicy` и `TaskPlan`. Модель проверяет поля и сочетания настроек,
применяет явные overrides к переданной базовой policy и строит детерминированный
план без доступа к Git, файловой системе, времени или локали.

**Решения и границы:**

- Внешние TOML-поля остаются в `config/schema.rs`, преобразования policy —
  в `config/workflow.rs`; доменная проверка и планирование — в
  `vcs/workflow/policy.rs`. Новых dependencies нет.
- `custom` наследует именованный preset через `extends` и частичные поля
  `[vcs.workflow.policy]` либо задаёт все поля самостоятельно без `extends`.
  Наследование от `custom` запрещено; переданная в `resolve` база должна
  соответствовать выбранному preset. Готовые defaults остаются T09–T11.
- Старый config с одним `preset`, включая `custom`, читается и записывается
  в прежнем компактном виде. Ненастроенный `custom` не получает скрытых defaults;
  попытка получить полную policy возвращает структурированную ошибку.
- Модель содержит base branch, task branch policy и шаблон с одним `{task}`,
  remote, sync strategy, integration target, publish, finish requirement и
  локальную очистку. Task ID подставляется буквально; Git-имена проверяются
  через имеющийся `gix`. Работа непосредственно в базовой ветке не добавлена.
- `false` в overrides сохраняется; условия publish/finish перепроверяются
  после наследования. Удаление локальной ветки требует подтверждения интеграции;
  удаление remote-ветки не подразумевается.
- `ProjectConfiguration` больше не `Copy`, getters принимают `&self`;
  `workflow()` возвращает выбор, `workflow_settings()` — настройки. Discovery
  сохраняет policy при построении проекта.
- CLI проверяет policy и выводит ошибки на RU/EN. Новых команд и JSON-схем нет;
  Git-операции не исполняются. Проверки существования веток, dirty state,
  публикации и интеграции остаются runtime preflight будущих команд.

**Проверено:** `cargo fmt --check`, `cargo check`,
`cargo clippy --all-targets --all-features -- -D warnings`,
`ESKA_TEST_ROOT=/home/kas/Projects/eska-playground cargo test` — успешно
(57 unit, 56 integration). Покрыты детерминированный план, inheritance и
явное `false`, все строковые enum-значения, round-trip config, невалидные поля,
имена веток, неполная policy, противоречия настроек, discovery и RU/EN diagnostics.
В playground вручную проверены пример policy из README, ошибка `base_branch`,
`new`/`init` на обоих языках и сохранность Git-метаданных.

## T09 — Trunk preset

**Статус:** `DONE`
**Зависит от:** T08

Short-lived task branch от `main`, rebase на `origin/main`, publish task branch,
integration через MR. Direct-trunk оставить отложенным вариантом.

**Реализовано:** встроенная policy `trunk` создаёт ветку `task/{task}` от `main`,
синхронизирует её через rebase на `refs/remotes/origin/main`, публикует task-ветку
и требует подтверждённой интеграции в `main` перед завершением. После интеграции
разрешено удалить локальную task-ветку; remote-ветка не удаляется.

**Решения и границы:**

- `WorkflowSettings::resolve(None)` разрешает готовый Trunk preset и частичные
  overrides `custom extends = "trunk"`; явное `false` продолжает сохраняться;
- Git Flow и GitHub Flow по-прежнему требуют явно переданную базовую policy до
  T10–T11; config не материализует встроенные defaults при сериализации;
- plan остаётся чистым и декларативным: Git-команды, проверка репозитория,
  создание MR и runtime preflight не добавлены;
- direct-trunk не добавлен и остаётся в deferred scope; новых config-полей,
  пользовательских строк, CLI-команд и dependencies нет.

**Проверено:** `cargo fmt --check`, `cargo check --offline`, 62 unit-теста и 57
integration-тестов в изолированном соседнем playground — успешно; 4 workflow
integration-теста включают RU/EN CLI validation. Clippy по всем targets и features
успешно проверил код; cargo lint `multiple_crate_versions` не удалось повторить
без отсутствующих в локальном cache платформенных crates и доступа к crates.io.

## T10 — Git Flow preset

**Статус:** `DONE`
**Зависит от:** T08

`feature/*` от `develop`; заложить policy slots для будущих `release/*` и
`hotfix/*` от `main`, не реализуя весь release flow заранее.

**Реализовано:** встроенная policy `git-flow` создаёт `feature/{task}` от
`develop`, синхронизирует её через rebase на `refs/remotes/origin/develop`,
публикует feature-ветку и требует подтверждённой интеграции в `develop` перед
локальным удалением. В policy сохранены отдельные внутренние slots для
`release/{task}` от `develop` и `hotfix/{task}` от `main`.

**Решения и границы:**

- `WorkflowSettings::resolve(None)` разрешает Git Flow и частичные overrides
  `custom extends = "git-flow"`, не материализуя defaults в config;
- custom overrides обычной task policy наследуют зарезервированные release/hotfix
  slots; их внешняя настройка появится только вместе с реальным потребителем;
- `plan()` продолжает строить только план обычной задачи; `--kind`, release,
  hotfix, Git-команды, MR и runtime preflight не добавлены;
- новых config-полей, пользовательских строк и dependencies нет.

**Проверено:** `cargo fmt --check`, `cargo check`,
`cargo clippy --all-targets --all-features -- -D warnings`, 65 unit-тестов и 58
integration-тестов в изолированном соседнем playground — успешно. Workflow
integration-тесты table-driven проверяют Trunk, Git Flow, custom overrides,
детерминированные планы и RU/EN CLI validation.

## T11 — GitHub Flow preset

**Статус:** `DONE`
**Зависит от:** T08

Short-lived branch от `main`, публикация ветки и PR/MR integration policy.

**Реализовано:** встроенная policy `github-flow` создаёт short-lived ветку
`feature/{task}` от `main`, синхронизирует её через rebase на
`refs/remotes/origin/main`, публикует ветку и требует подтверждённой интеграции в
`main` перед локальным удалением.

**Решения и границы:**

- GitHub Flow использует provider-neutral план: требование PR/MR выражено через
  публикацию task-ветки и `require-integrated`, без зависимости от GitHub API;
- `WorkflowSettings::resolve(None)` разрешает preset и частичные overrides
  `custom extends = "github-flow"`, не материализуя defaults в config;
- создание PR/MR, Git-команды и runtime preflight остаются задачами основного VCS
  UX; новых config-полей, пользовательских строк и dependencies нет.

**Проверено:** `cargo fmt --check`, `cargo check`,
`cargo clippy --all-targets --all-features -- -D warnings`, 66 unit-тестов и 57
integration-тестов в изолированном соседнем playground — успешно. Table-driven
workflow integration-тест покрывает все три presets, их custom overrides,
детерминированный plan и RU/EN CLI validation.

**Готовность workflow:** одинаковые входные данные дают детерминированный plan;
presets и custom overrides покрыты table-driven tests, пока без выполнения команд
повседневного UX.
