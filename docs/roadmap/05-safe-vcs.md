# Незавершённые и безопасные VCS-операции

## T18 — `eska history`

**Статус:** `PLANNED`
**Зависит от:** T07, T12, T17

Показать локальную commit/task history через `gix`, без fetch и без изменения
repository. Первая версия имеет ограничение количества записей, локализованный
human output и стабильный JSON; commit ID, parents, author/time и subject
возвращаются структурированно. Связь с task показывается только там, где она
однозначно выводится из refs и workflow policy.

Не добавлять собственный history index, graph renderer и provider API до
подтверждённой необходимости.

## T34 — `eska switch`

**Статус:** `PLANNED`
**Зависит от:** T13, T15, T18

Безопасно переключать текущую task/workspace по workflow policy. До изменения
worktree выполнить preflight, не уничтожать dirty state и не создавать скрытый
shelve. Пока `gix` не предоставляет равноценную высокоуровневую orchestration
index/worktree, system Git допустим только через единый infrastructure layer.

## T35 — `shelve` / `unshelve` / `shelves`

**Статус:** `PLANNED`
**Зависит от:** T15, T34

Дать UX «отложенных изменений» без утечки термина `stash` в public API.
Первичный backend может использовать Git stash через изолированный fallback.
Обязательны точный scope, preview, обнаружение конфликтов и сохранность
несвязанных staged changes.

## T36 — `eska restore`

**Статус:** `PLANNED`
**Зависит от:** T14, T15, T18

Восстанавливать явно выбранный project/object-aware scope с предварительным
preview и safeguards. Потенциально разрушающий путь требует явного `--force`,
если безопасной альтернативы нет; `reset --hard` не используется как общий
механизм. Backend выбирается по тем же gix-first правилам и документирует
оставшийся system Git fallback.
