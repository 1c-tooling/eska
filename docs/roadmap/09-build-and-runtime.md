# Build и development runtime

## T28 — Build subsystem через `ibcmd`

**Статус:** `PLANNED`  
**Зависит от:** T03, T22

`eska build` преобразует Designer XML через актуальные возможности `ibcmd` в
artifact соответствующего project type. Перед реализацией заново исследовать
поддерживаемые версии платформы (включая 8.5). Generated `build/` не source of truth
и игнорируется VCS; subprocess и credentials имеют безопасные diagnostics.

## T29 — `eska doctor`

**Статус:** `PLANNED`  
**Зависит от:** T07, T28

Диагностировать config/source, требуемую и установленную 1С, `ibcmd`, repository,
remote/locking и development environment. По умолчанию read-only; fix/setup только
явным отдельным режимом.

## T30 — Development environments

**Статус:** `PLANNED`  
**Зависит от:** T28

`env list/create/use/reset` стандартизируют dev/test infobases и их project state.
Credentials нельзя хранить открытым текстом в `eska.toml`.

## T31 — `eska apply` / `eska run`

**Статус:** `PLANNED`  
**Зависит от:** T22, T23, T28, T30

`apply` определяет changed objects и выбирает минимально достаточный partial/full
update с DB update при необходимости. `run` запускает 1С для активного environment;
отдельный `designer` уточнить при проектировании CLI.

