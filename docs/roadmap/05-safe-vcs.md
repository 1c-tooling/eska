# Незавершённые и безопасные VCS-операции

## T19 — `eska continue` / `eska abort`

**Статус:** `PLANNED`  
**Зависит от:** T16

Унифицировать продолжение и отмену поддерживаемых незавершённых workflow-операций
(первой будет sync/rebase). Определять реальное repository state, не хранить
ложную копию состояния и не скрывать последствия abort.

## T20 — `switch`, shelves, history, restore

**Статус:** `PLANNED`  
**Зависит от:** T13, T15, T19

- `switch` безопасно меняет текущую задачу/workspace;
- `shelve`, `unshelve`, `shelves` дают UX «отложенных изменений», первоначально
  поверх Git stash без утечки термина в public API;
- `history` сначала показывает commit/task history;
- `restore` имеет preview, точный scope и safeguards/явный `--force`.

Разбить T20 на отдельные implementation tasks, если единый diff станет большим.

