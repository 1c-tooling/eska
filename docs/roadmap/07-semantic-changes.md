# Semantic changes и commit messages

## T20 — Semantic `ChangeSet`

**Статус:** `DONE`
**Зависит от:** T14, T19

Ввести reusable pipeline `ChangeSet → SemanticChangeAnalyzer → ChangeSummary`.
Он обслуживает diff, locks, commit messages, changelog, affected и CI, а не одну
команду.

Принятые решения T20:

- `ChangeSet` нормализует существующие file-level результаты без повторного
  чтения repository: workspace сохраняет отдельные `HEAD → index` и
  `index → worktree` stages, revision comparison использует единый stage;
- project-relative пути остаются Git byte strings без lossy UTF-8 conversion,
  сортируются детерминированно по path/stage, а повтор одного stage/path
  объединяется консервативно с приоритетом conflict;
- `SemanticChangeAnalyzer` связывает `ChangeSet` с `Project` и T19
  `ObjectModel`. `ChangeSummary` содержит число уникальных файлов, counts по
  состояниям, затронутые `ObjectId`, тип/имя объекта и роль каждого пути:
  descriptor, module, form или artifact;
- изменения вне source или без объекта не отбрасываются, а остаются отдельным
  `unowned_changes`, чтобы downstream consumers могли применять собственную
  policy;
- pipeline одинаково проверен на реальных маленьких Git repositories для staged
  + unstaged workspace и commit-to-commit revision comparison;
- T20 не читает содержимое изменённых файлов, не создаёт semantic events и не
  меняет human/raw/JSON output. Object added/removed/changed, BSL methods, forms
  и metadata attributes остаются scope T21.

## T21 — Semantic diff

**Статус:** `DONE`
**Зависит от:** T20

Реализован opt-in режим `eska diff --semantic`:

- exact snapshot pairs HEAD/index/worktree и blob-пары revisions превращаются в
  deterministic events object added/removed/changed, module changed,
  method/function added/removed/changed, form changed и metadata attribute changed;
- Designer XML properties сравниваются структурно; форматирование вне значимых
  узлов не создаёт события. BSL parser консервативно принимает только завершённые
  русские и английские процедуры/функции, сохраняя module-level fallback;
- revision-анализ не зависит от текущего worktree. Для workspace T19
  `ObjectModel` даёт точные ownership и `ObjectId`, а удалённые объекты
  восстанавливаются из snapshot/path;
- human output локализован для `ru-RU` и `en-US`, сгруппирован по верхнему типу
  метаданных, событию и comparison stage и использует TTY/`NO_COLOR`-aware
  палитру обычного diff; raw имеет стабильные пять колонок; JSON schema version 3
  отделена от неизменившихся file-level schemas 1/2;
- three-way semantic merge и сравнение отдельных элементов формы не входят в T21.

## T22 — Генератор commit message

**Статус:** `IN-PROGRESS`
**Зависит от:** T15, T20

Для точного сохраняемого ChangeSet построить deterministic semantic draft, открыть
editor и затем commit. `save -m` обходит генератор; поздний `--auto` подтверждается
отдельным UX-решением. AI необязателен и в будущем получает краткий structured
summary, не огромный raw XML diff.
