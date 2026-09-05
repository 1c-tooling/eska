# Анализ, delivery и integrations

## T24 — `affected` analysis

**Статус:** `PLANNED`  
**Зависит от:** T20, T21, T23

Определять potentially affected objects/tests для changed objects; интегрировать с
`check --affected` и будущим `test --affected`. Human и JSON output обязательны.

## T25 — Versioning проекта 1С

**Статус:** `PLANNED`  
**Зависит от:** T03, T20

`version`, `version bump patch|minor|major`; позднее `auto` по Conventional Commits,
semantic changes и policy. Никогда не смешивать project version с версией бинарника
`eska`.

## T32 — Release pipeline

**Статус:** `PLANNED`  
**Зависит от:** T25, T27, T28

Policy-driven pipeline: determine/validate/update version, changelog, commit, tag,
build, artifacts. Полный `--dry-run` обязателен до write/destructive действий.

## T33 — CI/CD integration

**Статус:** `PLANNED`  
**Зависит от:** T23, T26–T28, T32

Одинаковые `fmt --check`, `check` и `build` локально и в CI; `test` включается,
когда отдельная implementation task по принятой T23 добавит backend. Будущий
`ci init` генерирует тонкие adapters для GitLab CI/GitHub Actions без business
logic; `eska` не становится CI server.

## T41 — VS Code extension

**Статус:** `PLANNED`  
**Зависит от:** стабильных T12–T18, T21, T27, T37–T39 и JSON protocol

Тонкий frontend: status, start/sync/publish, locking, diagnostics, command palette,
status bar. Не реализовывать VCS повторно в TypeScript; все операции идут через
стабильный core/CLI protocol.
