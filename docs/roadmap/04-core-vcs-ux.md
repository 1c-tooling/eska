# Основной VCS UX

Все задачи зависят от T07–T11. Human output локализуется; JSON является стабильным
нелокализованным API.

## T12 — `eska status`

**Статус:** `DONE`

Показывать состояние проекта, workflow, task/branch/base, ChangeSet summary,
ahead/behind, locks и readiness к save/publish. Это не копия `git status`.
Обязателен `--format json` со schema tests.

Принятые границы T12:

- task определяется по точному совпадению текущей ветки с task-шаблоном policy;
- ahead/behind считается относительно remote-tracking ref базовой ветки без fetch;
- ChangeSet остаётся file-level и ограничивается корнем проекта внутри worktree;
- отсутствующие remote base и locking представлены как недоступные данные, а не нули;
- JSON schema версии 1 не локализуется и проверяется end-to-end в обеих локалях.

## T13 — `eska start <task>`

**Статус:** `DONE`
**Зависит от:** T12

Выполнять policy plan: fetch/update base, создать и переключить task branch,
зарегистрировать task. Dirty workspace не уничтожать; отказ или явно безопасный
путь. `--kind hotfix` добавлять только вместе с готовой policy.

Принятые границы T13:

- task регистрируется активной веткой по policy, без отдельного state-файла;
- preflight требует чистоты всего worktree и attached HEAD с первым commit;
- при настроенном remote из policy выполняется fetch, локальная base обновляется
  только fast-forward; локальная base впереди remote сохраняется, divergence
  отклоняется;
- отсутствие remote не блокирует локальный старт: fetch пропускается, а task-ветка
  создаётся от локальной base; ошибка доступа к настроенному remote содержит его
  имя, безопасный URL и сохранённую причину Git;
- уже существующая task-ветка не переиспользуется и не перезаписывается;
- системный Git из единого infrastructure layer используется для network/mutating
  операций без разбора human output; Git credentials и transport environment
  сохраняются, repository redirects удаляются;
- `--kind hotfix`, shelve, JSON и отдельное task-state хранилище не добавлены.

**Проверено:** `cargo fmt --check`, `cargo check`,
`cargo clippy --all-targets --all-features -- -D warnings`,
`ESKA_TEST_ROOT="$(realpath ../eska-playground)" cargo test` — успешно
(69 unit, 76 integration). Реальные Git-сценарии покрывают актуальную, отстающую,
опережающую и разошедшуюся base, существующую task-ветку и dirty worktree.
CLI end-to-end проверяет Trunk/Git Flow, RU/EN, exit code, stdout/stderr и
регистрацию task через последующий JSON `status`. Отдельно проверены локальный
старт без remote, подробная ошибка недоступного remote и редактирование пароля
в URL. Ручные Trunk RU и Git Flow EN сценарии выполнены в отдельных временных
проектах playground.

## T14 — `eska diff`

**Статус:** `NEXT`
**Зависит от:** T12

Сначала file-level representation. Режимы: human, `--raw`, `--format json`.
Внутренний результат должен допускать последующее object-aware расширение без
изменения назначения команды.

## T15 — `eska save`

**Статус:** `PLANNED`  
**Зависит от:** T14

Сохранять точно выбранный ChangeSet. Поддержать `-m`; без него допустим configured
editor. Не заставлять пользователя понимать staging и не анализировать изменения,
которые не войдут в commit. Interactive/auto generation отложены до T25.

## T16 — `eska sync`

**Статус:** `PLANNED`  
**Зависит от:** T13, T15

По policy выполнять fetch и rebase/merge на актуальную base. Конфликт переводит
операцию в явное resumable state и сообщает `eska continue` / `eska abort`.

## T17 — `eska publish`

**Статус:** `PLANNED`  
**Зависит от:** T16

Preflight, требуемая policy синхронизация, push и настройка upstream. Создание
MR/PR и provider integrations оставить следующей задачей.

## T18 — `eska finish`

**Статус:** `PLANNED`  
**Зависит от:** T17

Проверить отсутствие unsaved work и выполнение publish/integration policy; снять
locks, перейти на base, обновить её, удалить локальную task branch и очистить task
state. Remote branch удаляется только по явной policy.
