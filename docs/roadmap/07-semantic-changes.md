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

**Статус:** `NEXT`
**Зависит от:** T20

Начать с надёжных событий: object added/removed/changed, module changed,
method/function changed, form changed, metadata attribute changed. Добавить human,
raw и стабильный JSON presentation. Three-way semantic merge не входит.

## T22 — Генератор commit message

**Статус:** `PLANNED`  
**Зависит от:** T15, T20

Для точного сохраняемого ChangeSet построить deterministic semantic draft, открыть
editor и затем commit. `save -m` обходит генератор; поздний `--auto` подтверждается
отдельным UX-решением. AI необязателен и в будущем получает краткий structured
summary, не огромный raw XML diff.
