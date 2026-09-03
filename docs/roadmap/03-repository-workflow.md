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

**Статус:** `IN-PROGRESS`
**Зависит от:** T02, T07

Модель определяет base branch, working branch policy, task naming, sync strategy,
integration target, publish и finish behavior. `custom` переопределяет preset через
config, а не через новый Rust enum на каждую компанию.

## T09 — Trunk preset

**Статус:** `PLANNED`  
**Зависит от:** T08

Short-lived task branch от `main`, rebase на `origin/main`, publish task branch,
integration через MR. Direct-trunk оставить отложенным вариантом.

## T10 — Git Flow preset

**Статус:** `PLANNED`  
**Зависит от:** T08

`feature/*` от `develop`; заложить policy slots для будущих `release/*` и
`hotfix/*` от `main`, не реализуя весь release flow заранее.

## T11 — GitHub Flow preset

**Статус:** `PLANNED`  
**Зависит от:** T08

Short-lived branch от `main`, публикация ветки и PR/MR integration policy.

**Готовность workflow:** одинаковые входные данные дают детерминированный plan;
presets и custom overrides покрыты table-driven tests, пока без выполнения команд
повседневного UX.
