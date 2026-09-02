# Анализ, delivery и integrations

## T32 — `affected` analysis

**Статус:** `PLANNED`  
**Зависит от:** T23, T24

Определять potentially affected objects/tests для changed objects; интегрировать с
`check --affected` и будущим `test --affected`. Human и JSON output обязательны.

## T33 — Versioning проекта 1С

**Статус:** `PLANNED`  
**Зависит от:** T03, T23

`version`, `version bump patch|minor|major`; позднее `auto` по Conventional Commits,
semantic changes и policy. Никогда не смешивать project version с версией бинарника
`eska`.

## T34 — Release pipeline

**Статус:** `PLANNED`  
**Зависит от:** T27, T28, T33

Policy-driven pipeline: determine/validate/update version, changelog, commit, tag,
build, artifacts. Полный `--dry-run` обязателен до write/destructive действий.

## T35 — CI/CD integration

**Статус:** `PLANNED`  
**Зависит от:** T26–T28, T34

Одинаковые `fmt --check`, `check`, `test`, `build` локально и в CI. Будущий
`ci init` генерирует тонкие adapters для GitLab CI/GitHub Actions без business
logic; `eska` не становится CI server.

## T36 — VS Code extension

**Статус:** `PLANNED`  
**Зависит от:** стабильных T12–T27 и JSON protocol

Тонкий frontend: status, start/sync/publish, locking, diagnostics, command palette,
status bar. Не реализовывать VCS повторно в TypeScript; все операции идут через
стабильный core/CLI protocol.

