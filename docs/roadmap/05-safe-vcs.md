# Незавершённые и безопасные VCS-операции

## T18 — `eska history`

**Статус:** `DONE`
**Зависит от:** T07, T12, T17

Показать локальную commit/task history через `gix`, без fetch и без изменения
repository. Первая версия имеет ограничение количества записей, локализованный
human output и стабильный JSON; commit ID, parents, author/time и subject
возвращаются структурированно. Связь с task показывается только там, где она
однозначно выводится из refs и workflow policy.

Не добавлять собственный history index, graph renderer и provider API до
подтверждённой необходимости.

**Реализовано:** `eska history [--limit 1..=1000] [--format human|json]` читает
достижимые из локального HEAD commits через `gix`, сортирует их по времени commit
и не выполняет fetch. Human-вывод локализован, JSON версии 1 не зависит от locale
и сохраняет commit ID, parents, author/time, subject и task структурированно;
произвольные Git-байты передаются с явной обратимой кодировкой.

Task attribution консервативен: commit должен находиться вне локальной base и
быть достижим ровно из одной локальной task-ветки, соответствующей workflow
policy. Отсутствующий workflow или base, влитая история и неоднозначные refs дают
`task = null`. Собственный индекс, graph renderer и provider API не добавлены.

## T34 — `eska switch`

**Статус:** `NEXT`
**Зависит от:** T13, T15, T18

Безопасно переключать текущую task/workspace по workflow policy:

```text
eska switch <task>
eska switch --base
```

Первая команда активирует уже существующую локальную task-ветку, вторая временно
возвращает на policy base без завершения задачи. Команда не создаёт ветки, не
выполняет fetch и не хранит отдельный task state: активная ветка остаётся
источником истины, а task ID извлекается через workflow policy.

До изменения worktree проверяется вся repository, включая файлы вне вложенного
проекта. Dirty state отклоняется с предложением выполнить `eska save`; скрытый
shelve не создаётся. Отсутствующую task-ветку нужно создавать через `eska start`.
Проверка refs выполняется через `gix`, а изменение worktree — через существующий
изолированный system Git layer, пока `gix` не даёт равноценной orchestration
HEAD/index/worktree.

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
