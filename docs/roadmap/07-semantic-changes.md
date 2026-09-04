# Semantic changes и commit messages

## T23 — Semantic `ChangeSet`

**Статус:** `PLANNED`  
**Зависит от:** T14, T22

Ввести reusable pipeline `ChangeSet → SemanticChangeAnalyzer → ChangeSummary`.
Он обслуживает diff, locks, commit messages, changelog, affected и CI, а не одну
команду.

## T24 — Semantic diff

**Статус:** `PLANNED`  
**Зависит от:** T23

Начать с надёжных событий: object added/removed/changed, module changed,
method/function changed, form changed, metadata attribute changed. Добавить human,
raw и стабильный JSON presentation. Three-way semantic merge не входит.

## T25 — Генератор commit message

**Статус:** `PLANNED`  
**Зависит от:** T15, T23

Для точного сохраняемого ChangeSet построить deterministic semantic draft, открыть
editor и затем commit. `save -m` обходит генератор; поздний `--auto` подтверждается
отдельным UX-решением. AI необязателен и в будущем получает краткий structured
summary, не огромный raw XML diff.

