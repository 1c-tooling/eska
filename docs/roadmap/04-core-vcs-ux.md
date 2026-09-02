# Основной VCS UX

Все задачи зависят от T07–T11. Human output локализуется; JSON является стабильным
нелокализованным API.

## T12 — `eska status`

**Статус:** `PLANNED`

Показывать состояние проекта, workflow, task/branch/base, ChangeSet summary,
ahead/behind, locks и readiness к save/publish. Это не копия `git status`.
Обязателен `--format json` со schema tests.

## T13 — `eska start <task>`

**Статус:** `PLANNED`  
**Зависит от:** T12

Выполнять policy plan: fetch/update base, создать и переключить task branch,
зарегистрировать task. Dirty workspace не уничтожать; отказ или явно безопасный
путь. `--kind hotfix` добавлять только вместе с готовой policy.

## T14 — `eska diff`

**Статус:** `PLANNED`  
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

